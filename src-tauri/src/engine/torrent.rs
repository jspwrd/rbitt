#![allow(dead_code)]

use crate::TorrentStatus;
use oxidebt_constants::CONNECTION_SPEED;
use oxidebt_peer::PieceManager;
use oxidebt_torrent::{InfoHash, Metainfo};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::peer_info::{FailedPeer, PeerInfo};
use super::settings::{FilePriority, ShareLimits};
use super::stats::TorrentStats;
use super::tracker_info::TrackerInfo;

/// Tracks connection attempts to enforce rate limiting (connections per second)
pub struct ConnectionRateLimiter {
    /// Timestamps of recent connection attempts
    attempts: VecDeque<Instant>,
    /// Maximum connections per second
    max_per_second: usize,
}

impl ConnectionRateLimiter {
    pub fn new(max_per_second: usize) -> Self {
        Self {
            attempts: VecDeque::with_capacity(max_per_second * 2),
            max_per_second,
        }
    }

    /// Returns how many connections we can make right now
    pub fn available_slots(&mut self) -> usize {
        let now = Instant::now();
        let one_second_ago = now - Duration::from_secs(1);

        // Remove attempts older than 1 second
        while let Some(&oldest) = self.attempts.front() {
            if oldest < one_second_ago {
                self.attempts.pop_front();
            } else {
                break;
            }
        }

        // Return available slots
        self.max_per_second.saturating_sub(self.attempts.len())
    }

    /// Record that we made connection attempts
    pub fn record_attempts(&mut self, count: usize) {
        let now = Instant::now();
        for _ in 0..count {
            self.attempts.push_back(now);
        }
    }
}

/// State of a torrent download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TorrentState {
    /// Downloading metadata for magnet links
    MetadataDownloading,
    /// Verifying existing pieces on disk
    Checking,
    /// Queued waiting for active slot
    Queued,
    /// Actively downloading pieces
    Downloading,
    /// Completed and actively uploading to peers
    Seeding,
    /// Completed but no peers requesting from us
    Completed,
    /// Paused by user
    Paused,
    /// Error occurred, torrent cannot continue
    Error,
    /// Files are being moved to a new location
    Moving,
}

impl TorrentState {
    /// Returns true if this state is considered "active" for limit counting
    pub fn is_active_for_limits(&self) -> bool {
        matches!(
            self,
            TorrentState::Downloading | TorrentState::Seeding | TorrentState::Checking
        )
    }
}

/// A torrent being managed by the engine.
pub struct ManagedTorrent {
    pub meta: Metainfo,
    pub state: TorrentState,
    pub piece_manager: Arc<PieceManager>,
    pub peers: HashMap<SocketAddr, PeerInfo>,
    pub known_peers: HashSet<SocketAddr>,
    pub connecting_peers: HashSet<SocketAddr>,
    pub failed_peers: HashMap<SocketAddr, FailedPeer>,
    pub stats: TorrentStats,
    pub last_announce: Option<Instant>,
    pub last_dht_query: Option<Instant>,
    pub trackers: Vec<String>,
    pub tracker_info: Vec<TrackerInfo>,
    pub last_optimistic_unchoke: Instant,
    pub unchoked_peers: HashSet<SocketAddr>,
    pub cancel_tx: tokio::sync::broadcast::Sender<u32>,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
    pub connection_limiter: ConnectionRateLimiter,
    /// Timestamp when the torrent was added, used for queue ordering (FIFO)
    pub added_at: Instant,
    /// Whether to download pieces in sequential order (for streaming)
    pub sequential_download: bool,
    /// Per-file download priorities
    pub file_priorities: Vec<FilePriority>,
    /// Assigned category (affects save path)
    pub category: Option<String>,
    /// Tags for organization
    pub tags: HashSet<String>,
    /// Share ratio and time limits
    pub share_limits: ShareLimits,
    /// Time when seeding started (for seeding time limit)
    pub seeding_started_at: Option<Instant>,
    /// Save path for this torrent (may differ from default if category-based)
    pub save_path: std::path::PathBuf,
    /// Whether to move files on completion
    pub move_on_complete: Option<std::path::PathBuf>,
}

impl ManagedTorrent {
    pub fn new(meta: Metainfo) -> Self {
        Self::with_save_path(meta, std::path::PathBuf::new())
    }

    pub fn with_save_path(meta: Metainfo, save_path: std::path::PathBuf) -> Self {
        let piece_manager = PieceManager::new(
            meta.piece_count(),
            meta.info.piece_length,
            meta.info.total_length,
        );

        let trackers = meta.tracker_urls();
        let tracker_info: Vec<TrackerInfo> = trackers
            .iter()
            .map(|url| TrackerInfo::new(url.clone()))
            .collect();

        let (cancel_tx, _) = tokio::sync::broadcast::channel::<u32>(64);
        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

        // Initialize file priorities to Normal for all files
        let file_priorities = vec![FilePriority::Normal; meta.info.files.len()];

        Self {
            meta,
            state: TorrentState::Checking,
            piece_manager,
            peers: HashMap::new(),
            known_peers: HashSet::new(),
            connecting_peers: HashSet::new(),
            failed_peers: HashMap::new(),
            stats: TorrentStats::new(),
            last_announce: None,
            last_dht_query: None,
            trackers,
            tracker_info,
            last_optimistic_unchoke: Instant::now(),
            unchoked_peers: HashSet::new(),
            cancel_tx,
            shutdown_tx,
            connection_limiter: ConnectionRateLimiter::new(CONNECTION_SPEED),
            added_at: Instant::now(),
            sequential_download: false,
            file_priorities,
            category: None,
            tags: HashSet::new(),
            share_limits: ShareLimits::default(),
            seeding_started_at: None,
            save_path,
            move_on_complete: None,
        }
    }

    /// Check if share limits have been reached
    pub fn share_limits_reached(&self) -> bool {
        // Check ratio limit
        if let Some(max_ratio) = self.share_limits.max_ratio {
            let downloaded = self.stats.downloaded.max(1); // Avoid division by zero
            let ratio = self.stats.uploaded as f64 / downloaded as f64;
            if ratio >= max_ratio {
                return true;
            }
        }

        // Check seeding time limit
        if let Some(max_time) = self.share_limits.max_seeding_time {
            if let Some(started) = self.seeding_started_at {
                if started.elapsed().as_secs() >= max_time {
                    return true;
                }
            }
        }

        false
    }

    /// Get current share ratio
    pub fn share_ratio(&self) -> f64 {
        let downloaded = self.stats.downloaded.max(1);
        self.stats.uploaded as f64 / downloaded as f64
    }

    /// Get seeding time in seconds
    pub fn seeding_time(&self) -> u64 {
        self.seeding_started_at
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// Compute piece priorities from file priorities.
    /// A piece is skipped (priority 0) only if ALL files that touch it are skipped.
    /// Otherwise, the piece gets the maximum priority of all files that touch it.
    pub fn compute_piece_priorities(&self) -> Vec<u8> {
        let piece_count = self.meta.piece_count();
        let piece_length = self.meta.info.piece_length;

        // If no file priorities set, all pieces are normal priority
        if self.file_priorities.is_empty() {
            return vec![4u8; piece_count]; // 4 = Normal
        }

        let mut piece_priorities = vec![0u8; piece_count];
        let mut piece_touched = vec![false; piece_count];

        let mut file_offset = 0u64;
        for (file_idx, file) in self.meta.info.files.iter().enumerate() {
            let file_priority = self
                .file_priorities
                .get(file_idx)
                .copied()
                .unwrap_or(FilePriority::Normal) as u8;

            // Calculate which pieces this file spans
            let file_start_piece = (file_offset / piece_length) as usize;
            let file_end_offset = file_offset + file.length;
            let file_end_piece = if file_end_offset == 0 {
                0
            } else {
                ((file_end_offset - 1) / piece_length) as usize
            };

            // Update priorities for pieces this file touches
            for piece_idx in file_start_piece..=file_end_piece.min(piece_count - 1) {
                piece_touched[piece_idx] = true;
                // Take the maximum priority (higher number = higher priority)
                if file_priority > piece_priorities[piece_idx] {
                    piece_priorities[piece_idx] = file_priority;
                }
            }

            file_offset = file_end_offset;
        }

        // Any piece not touched by any file gets normal priority
        for (idx, touched) in piece_touched.iter().enumerate() {
            if !touched {
                piece_priorities[idx] = 4; // Normal
            }
        }

        piece_priorities
    }

    /// Check if any files are set to skip
    pub fn has_skipped_files(&self) -> bool {
        self.file_priorities.contains(&FilePriority::Skip)
    }

    pub fn info_hash_hex(&self) -> String {
        match &self.meta.info_hash {
            InfoHash::V1(h) => h.to_hex(),
            InfoHash::V2(h) => h.to_hex(),
            InfoHash::Hybrid { v1, .. } => v1.to_hex(),
        }
    }

    pub fn info_hash_bytes(&self) -> Option<[u8; 20]> {
        self.meta.info_hash.v1().map(|h| *h.as_bytes())
    }

    pub fn progress(&self) -> f64 {
        let piece_count = self.meta.piece_count();
        if piece_count == 0 {
            return 0.0;
        }
        let have_count = self.piece_manager.have_count();
        (have_count as f64 / piece_count as f64) * 100.0
    }

    pub fn to_status(&self) -> TorrentStatus {
        let seeds = self
            .peers
            .values()
            .filter(|p| {
                p.bitfield
                    .as_ref()
                    .map(|bf| bf.is_complete())
                    .unwrap_or(false)
            })
            .count();

        // Cap downloaded at total_length to avoid showing impossible values
        let downloaded = self.stats.downloaded.min(self.meta.info.total_length);

        // Check if torrent is complete
        let is_complete = self.progress() >= 100.0;

        // When complete (seeding), download rate should be 0
        let download_rate = if is_complete {
            0.0
        } else {
            self.stats.download_rate
        };

        // Compute qBittorrent-compatible single state
        let state = self.compute_display_state(download_rate, self.stats.upload_rate, is_complete);

        TorrentStatus {
            info_hash: self.info_hash_hex(),
            name: self.meta.info.name.clone(),
            state,
            progress: self.progress(),
            download_rate,
            upload_rate: self.stats.upload_rate,
            downloaded,
            uploaded: self.stats.uploaded,
            total_size: self.meta.info.total_length,
            peers: self.peers.len(),
            seeds,
        }
    }

    /// Compute qBittorrent-compatible display state based on internal state and transfer rates.
    /// States follow qBittorrent's model: downloading, uploading, stalledDL, stalledUP,
    /// pausedDL, pausedUP, queuedDL, queuedUP, checkingDL, checkingUP, metaDL, moving, error
    fn compute_display_state(
        &self,
        download_rate: f64,
        upload_rate: f64,
        is_complete: bool,
    ) -> String {
        match self.state {
            TorrentState::MetadataDownloading => "metaDL".to_string(),
            TorrentState::Checking => {
                if is_complete {
                    "checkingUP".to_string()
                } else {
                    "checkingDL".to_string()
                }
            }
            TorrentState::Queued => {
                if is_complete {
                    "queuedUP".to_string()
                } else {
                    "queuedDL".to_string()
                }
            }
            TorrentState::Downloading => {
                // Downloading: active if transfer happening, stalled otherwise
                if download_rate > 0.0 {
                    "downloading".to_string()
                } else {
                    "stalledDL".to_string()
                }
            }
            TorrentState::Seeding => {
                // Seeding: uploading if transfer happening, stalledUP otherwise
                if upload_rate > 0.0 {
                    "uploading".to_string()
                } else {
                    "stalledUP".to_string()
                }
            }
            TorrentState::Completed => {
                // Completed: finished downloading, show as completed regardless of upload activity
                "completed".to_string()
            }
            TorrentState::Paused => {
                if is_complete {
                    "pausedUP".to_string()
                } else {
                    "pausedDL".to_string()
                }
            }
            TorrentState::Error => "error".to_string(),
            TorrentState::Moving => "moving".to_string(),
        }
    }

    pub fn piece_size(&self, index: u32) -> u64 {
        let piece_count = self.meta.piece_count();
        if piece_count == 0 {
            return 0;
        }
        if (index as usize) < piece_count - 1 {
            self.meta.info.piece_length
        } else {
            let remainder = self.meta.info.total_length % self.meta.info.piece_length;
            if remainder == 0 {
                self.meta.info.piece_length
            } else {
                remainder
            }
        }
    }
}
