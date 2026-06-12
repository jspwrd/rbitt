mod error;
mod events;
mod metadata;
mod peer_connection;
mod peer_info;
pub mod persistence;
mod pex;
pub mod rss;
pub mod search;
pub mod settings;
mod stats;
mod torrent;
mod tracker_info;
pub mod watch;

pub use error::EngineError;
pub use rss::{RssDownloadRule, RssFeed};
pub use search::SearchStatus;
pub use settings::{FilePriority, LimitAction, WatchFolder};
pub use torrent::TorrentState;

use events::PeerEvent;
use metadata::fetch_metadata_from_peers;
use oxidebt_constants::{
    CHECKING_SLEEP_INTERVAL, CHOKING_INTERVAL, CONNECTION_RETRY_SLEEP, CONNECTION_TIMEOUT,
    DEFAULT_PORT, DHT_INTERVAL_CRITICAL, DHT_INTERVAL_HIGH, DHT_INTERVAL_LOW, DHT_INTERVAL_MEDIUM,
    DHT_QUERY_SLEEP, LOOP_INTERVAL_FAST, LOOP_INTERVAL_NORMAL, LOOP_INTERVAL_STABLE,
    LSD_ANNOUNCE_INTERVAL, MAX_GLOBAL_CONNECTIONS, MAX_HALF_OPEN, MAX_PEERS_PER_TORRENT,
    MAX_PEER_RETRY_ATTEMPTS, MAX_UNCHOKED_PEERS, OPTIMISTIC_UNCHOKE_INTERVAL,
    PAUSED_SLEEP_INTERVAL, PEER_THRESHOLD_CRITICAL, PEER_THRESHOLD_LOW, PEER_THRESHOLD_MEDIUM,
    REANNOUNCE_CHECK_INTERVAL, TRACKER_AGGRESSIVE_INTERVAL, TRACKER_ANNOUNCE_INTERVAL,
    TRACKER_MIN_INTERVAL, TRACKER_MODERATE_INTERVAL,
};
use oxidebt_dht::DhtServer;
use oxidebt_disk::{DiskManager, FileEntry, PieceInfo, TorrentStorage};
use oxidebt_net::{BandwidthLimiter, LsdService, PortMapper, PortMapping, Protocol};
use oxidebt_peer::{Bitfield, PeerConnection, PeerId};
use oxidebt_torrent::{InfoHash, MagnetLink, Metainfo};
use oxidebt_tracker::{AnnounceParams, TrackerClient, TrackerEvent};
use parking_lot::RwLock;
use peer_info::{FailedPeer, PeerInfo};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use torrent::ManagedTorrent;
use tracker_info::TrackerState;

use crate::TrackerStatusInfo;

/// State shared between the engine facade and every background task.
/// Tasks hold a single `Arc<EngineShared>` instead of a dozen individually
/// cloned handles, and all torrent-add sources (UI, watch folders, RSS,
/// session restore) go through `EngineShared::add_torrent`.
pub struct EngineShared {
    peer_id: PeerId,
    download_dir: PathBuf,
    listen_port: Arc<AtomicU16>,
    torrents: Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    tracker_client: Arc<TrackerClient>,
    disk_manager: Arc<DiskManager>,
    bandwidth_limiter: Arc<RwLock<BandwidthLimiter>>,
    dht: Option<Arc<DhtServer>>,
    lsd: Option<Arc<LsdService>>,
    event_tx: mpsc::UnboundedSender<PeerEvent>,
    global_connections: Arc<AtomicUsize>,
    max_active_downloads: Arc<AtomicUsize>,
    max_active_uploads: Arc<AtomicUsize>,
    /// When enabled, rejects all incoming block requests and prevents seeding
    no_seed_mode: Arc<AtomicBool>,
    /// When enabled, disconnect all peers when a torrent completes (only applies when no_seed_mode is enabled)
    disconnect_on_complete: Arc<AtomicBool>,
    /// Categories for torrent organization
    categories: Arc<RwLock<HashMap<String, settings::Category>>>,
    /// Auto-add tracker settings
    auto_tracker_settings: Arc<RwLock<settings::AutoTrackerSettings>>,
    /// Move on completion settings
    move_on_complete_settings: Arc<RwLock<settings::MoveOnCompleteSettings>>,
    /// External program settings
    external_program_settings: Arc<RwLock<settings::ExternalProgramSettings>>,
    /// Default share limits for new torrents
    default_share_limits: Arc<RwLock<settings::ShareLimits>>,
    /// Magnet links whose metadata is still being fetched, keyed by info
    /// hash. Surfaced to the UI as synthetic metaDL/error entries.
    pending_magnets: RwLock<HashMap<String, PendingMagnet>>,
    /// Configured bandwidth limits in bytes/sec (0 = unlimited); mirrors the
    /// limiter, which does not expose its configuration.
    bandwidth_limits: RwLock<(u64, u64)>,
    /// Watch folder manager
    watch_manager: Arc<watch::WatchFolderManager>,
    /// RSS manager
    rss_manager: Arc<rss::RssManager>,
    /// Search engine
    search_engine: Arc<search::SearchEngine>,
    /// Queue for the coalescing persistence writer task.
    persist_tx: mpsc::UnboundedSender<PersistKind>,
}

/// A magnet add that does not yet have its metainfo.
struct PendingMagnet {
    /// Original URI, persisted so an interrupted fetch resumes next launch.
    uri: String,
    name: String,
    /// Set when discovery or the metadata fetch failed; the entry then shows
    /// as an error state in the UI until the user removes it.
    error: Option<String>,
}

/// What needs writing to disk; bursts are coalesced by the writer task.
#[derive(Clone, Copy)]
enum PersistKind {
    Settings,
    Session,
}

/// Options applied when adding a torrent. Every add source builds one of
/// these; defaults mirror a plain UI add into the download directory.
#[derive(Default)]
pub struct AddTorrentOptions {
    /// Explicit save directory; falls back to the category's path, then the
    /// engine download directory.
    pub save_path: Option<PathBuf>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub add_paused: bool,
    /// Per-torrent share limits; None applies the engine default.
    pub share_limits: Option<settings::ShareLimits>,
}

pub struct TorrentEngine {
    shared: Arc<EngineShared>,
    _port_mapper: Arc<RwLock<PortMapper>>,
    event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<PeerEvent>>>>,
}

/// The facade derefs to the shared state so its accessor methods can read
/// engine fields directly and call `EngineShared` methods without explicit
/// delegation. Intentional deref-to-member: TorrentEngine is a thin app-local
/// handle over EngineShared, not a public library type.
impl std::ops::Deref for TorrentEngine {
    type Target = Arc<EngineShared>;

    fn deref(&self) -> &Arc<EngineShared> {
        &self.shared
    }
}

impl EngineShared {
    /// Start processing watch folder events
    fn start_watch_folder_processor(
        self: &Arc<Self>,
        mut rx: mpsc::UnboundedReceiver<watch::WatchEvent>,
    ) {
        let shared = self.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    watch::WatchEvent::TorrentFound {
                        path,
                        category,
                        tags,
                        ..
                    } => {
                        tracing::info!("Watch folder: Found torrent file {:?}", path);
                        let data = match tokio::fs::read(&path).await {
                            Ok(data) => data,
                            Err(e) => {
                                tracing::warn!("Watch folder: Failed to read {:?}: {}", path, e);
                                continue;
                            }
                        };
                        let meta = match Metainfo::from_bytes(&data) {
                            Ok(meta) => meta,
                            Err(e) => {
                                tracing::warn!("Watch folder: Failed to parse {:?}: {}", path, e);
                                continue;
                            }
                        };

                        let name = meta.info.name.clone();
                        let opts = AddTorrentOptions {
                            category,
                            tags,
                            ..Default::default()
                        };
                        match shared.add_torrent(meta, opts).await {
                            Ok(hash) => {
                                tracing::info!("Watch folder: Added torrent '{}' ({})", name, hash);
                            }
                            Err(e) => {
                                tracing::warn!("Watch folder: Failed to add {:?}: {}", path, e);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Start processing RSS match events
    fn start_rss_processor(self: &Arc<Self>, mut rx: mpsc::UnboundedReceiver<rss::RssMatchEvent>) {
        let shared = self.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                tracing::info!(
                    "RSS: Matched '{}' from feed {} with rule {}",
                    event.item.title,
                    event.feed_id,
                    event.rule_id
                );

                let torrent_url = &event.item.torrent_url;
                let opts = AddTorrentOptions {
                    save_path: event.save_path.as_ref().map(PathBuf::from),
                    category: event.category.clone(),
                    tags: event.tags.clone(),
                    add_paused: event.add_paused,
                    share_limits: None,
                };

                // Handle magnet links vs torrent URLs
                if torrent_url.starts_with("magnet:") {
                    match shared.add_magnet(torrent_url, opts).await {
                        Ok(hash) => {
                            tracing::info!("RSS: Added magnet {} ({})", torrent_url, hash);
                        }
                        Err(e) => {
                            tracing::warn!("RSS: Failed to add magnet {}: {}", torrent_url, e);
                        }
                    }
                    continue;
                }

                // Maximum torrent file size (10 MB - should be more than enough)
                const MAX_TORRENT_FILE_SIZE: u64 = 10 * 1024 * 1024;

                let response = match reqwest::get(torrent_url).await {
                    Ok(response) => response,
                    Err(e) => {
                        tracing::warn!(
                            "RSS: Failed to download torrent from {}: {}",
                            torrent_url,
                            e
                        );
                        continue;
                    }
                };

                // Check content-length before downloading
                if let Some(content_length) = response.content_length() {
                    if content_length > MAX_TORRENT_FILE_SIZE {
                        tracing::warn!(
                            "RSS: Torrent file too large ({} bytes, max {}): {}",
                            content_length,
                            MAX_TORRENT_FILE_SIZE,
                            torrent_url
                        );
                        continue;
                    }
                }

                let data = match response.bytes().await {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::warn!("RSS: Failed to read torrent from {}: {}", torrent_url, e);
                        continue;
                    }
                };

                // Also check after download in case content-length was missing
                if data.len() as u64 > MAX_TORRENT_FILE_SIZE {
                    tracing::warn!(
                        "RSS: Downloaded torrent file too large ({} bytes, max {}): {}",
                        data.len(),
                        MAX_TORRENT_FILE_SIZE,
                        torrent_url
                    );
                    continue;
                }

                let meta = match Metainfo::from_bytes(&data) {
                    Ok(meta) => meta,
                    Err(e) => {
                        tracing::warn!("RSS: Failed to parse torrent from {}: {}", torrent_url, e);
                        continue;
                    }
                };

                let name = meta.info.name.clone();
                match shared.add_torrent(meta, opts).await {
                    Ok(hash) => {
                        tracing::info!("RSS: Added torrent '{}' ({})", name, hash);
                    }
                    Err(e) => {
                        tracing::warn!("RSS: Failed to add torrent from {}: {}", torrent_url, e);
                    }
                }
            }
        });
    }

    /// Start checking share limits periodically
    fn start_share_limits_checker(self: &Arc<Self>) {
        let shared = self.clone();

        tokio::spawn(async move {
            let check_interval = Duration::from_secs(60);

            loop {
                tokio::time::sleep(check_interval).await;

                let mut actions: Vec<(String, settings::LimitAction)> = Vec::new();

                {
                    let guard = shared.torrents.read();
                    for (hash, torrent) in guard.iter() {
                        if matches!(
                            torrent.state,
                            TorrentState::Seeding | TorrentState::Completed
                        ) && torrent.share_limits_reached()
                        {
                            actions.push((hash.clone(), torrent.share_limits.limit_action));
                        }
                    }
                }

                for (hash, action) in actions {
                    match action {
                        settings::LimitAction::Pause => {
                            tracing::info!("Share limit reached for {}, pausing", hash);
                            let _ = shared.pause_torrent(&hash).await;
                        }
                        settings::LimitAction::Remove => {
                            tracing::info!(
                                "Share limit reached for {}, removing (keeping files)",
                                hash
                            );
                            let _ = shared.remove_torrent(&hash, false).await;
                        }
                        settings::LimitAction::RemoveWithFiles => {
                            tracing::info!("Share limit reached for {}, removing with files", hash);
                            let _ = shared.remove_torrent(&hash, true).await;
                        }
                    }
                }
            }
        });
    }

    /// Loads settings.json (if present) and applies it to live engine state.
    async fn apply_persisted_settings(self: &Arc<Self>) {
        let Some(s) = persistence::load_settings().await else {
            return;
        };

        {
            let mut categories = self.categories.write();
            for c in s.categories {
                categories.insert(c.name.clone(), c);
            }
        }
        *self.auto_tracker_settings.write() = s.auto_trackers;
        *self.move_on_complete_settings.write() = s.move_on_complete;
        *self.external_program_settings.write() = s.external_program;
        *self.default_share_limits.write() = s.default_share_limits;
        self.max_active_downloads
            .store(s.max_active_downloads, Ordering::Relaxed);
        self.max_active_uploads
            .store(s.max_active_uploads, Ordering::Relaxed);
        self.no_seed_mode.store(s.no_seed_mode, Ordering::Release);
        self.disconnect_on_complete
            .store(s.disconnect_on_complete, Ordering::Release);
        {
            let mut limiter = self.bandwidth_limiter.write();
            limiter.set_download_limit(s.download_limit);
            limiter.set_upload_limit(s.upload_limit);
        }
        *self.bandwidth_limits.write() = (s.download_limit, s.upload_limit);

        for folder in s.watch_folders {
            self.watch_manager.add_folder(folder).await;
        }
        for feed in s.rss_feeds {
            self.rss_manager.add_feed(feed).await;
        }
        for rule in s.rss_rules {
            self.rss_manager.add_rule(rule).await;
        }
        self.rss_manager
            .restore_downloaded_history(s.rss_downloaded)
            .await;

        tracing::info!("Loaded persisted settings");
    }

    /// Restores the previous session's torrents (in queue order) and pending
    /// magnets. Progress is recovered by the normal on-add verification.
    async fn restore_session(self: Arc<Self>) {
        let Some(session) = persistence::load_session().await else {
            return;
        };

        tracing::info!(
            "Restoring session: {} torrents, {} pending magnets",
            session.torrents.len(),
            session.pending_magnets.len()
        );

        for saved in session.torrents {
            let Some(blob) = persistence::load_torrent_blob(&saved.info_hash).await else {
                tracing::warn!("Missing torrent blob for {}, skipping", saved.info_hash);
                continue;
            };

            let meta = match Metainfo::from_info_dict(&blob, &saved.trackers) {
                Ok(meta) => meta,
                Err(e) => {
                    tracing::warn!("Corrupt torrent blob for {}: {}", saved.info_hash, e);
                    continue;
                }
            };

            let opts = AddTorrentOptions {
                save_path: Some(saved.save_path.clone()),
                category: saved.category.clone(),
                tags: saved.tags.clone(),
                add_paused: saved.paused,
                share_limits: Some(saved.share_limits.clone()),
            };

            match self.add_torrent(meta, opts).await {
                Ok(hash) => {
                    let mut torrents = self.torrents.write();
                    if let Some(torrent) = torrents.get_mut(&hash) {
                        // Downloaded bytes are re-derived from verification;
                        // uploaded must be restored for ratio accounting.
                        torrent.stats.uploaded = saved.uploaded;
                        torrent.sequential_download = saved.sequential_download;
                        torrent.move_on_complete = saved.move_on_complete.clone();
                        if saved.file_priorities.len() == torrent.file_priorities.len() {
                            torrent.file_priorities = saved
                                .file_priorities
                                .iter()
                                .copied()
                                .map(FilePriority::from_u8)
                                .collect();
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to restore torrent {}: {}", saved.info_hash, e);
                }
            }
        }

        for uri in session.pending_magnets {
            if let Err(e) = self.add_magnet(&uri, AddTorrentOptions::default()).await {
                tracing::warn!("Failed to restore magnet {}: {}", uri, e);
            }
        }
    }

    /// Single writer task that coalesces bursts of persist requests into one
    /// disk write per kind.
    fn start_persistence_writer(self: &Arc<Self>, mut rx: mpsc::UnboundedReceiver<PersistKind>) {
        let shared = self.clone();
        tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                let mut save_settings = matches!(first, PersistKind::Settings);
                let mut save_session = matches!(first, PersistKind::Session);

                tokio::time::sleep(Duration::from_millis(500)).await;
                while let Ok(kind) = rx.try_recv() {
                    match kind {
                        PersistKind::Settings => save_settings = true,
                        PersistKind::Session => save_session = true,
                    }
                }

                if save_settings {
                    let snapshot = shared.snapshot_settings().await;
                    if let Err(e) = persistence::save_settings(&snapshot).await {
                        tracing::warn!("Failed to save settings: {}", e);
                    }
                }
                if save_session {
                    let snapshot = shared.snapshot_session();
                    if let Err(e) = persistence::save_session(&snapshot).await {
                        tracing::warn!("Failed to save session: {}", e);
                    }
                }
            }
        });
    }

    /// Periodic session flush so transfer counters survive a crash.
    fn start_periodic_session_flush(self: &Arc<Self>) {
        let shared = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.tick().await; // consume the immediate first tick
            loop {
                interval.tick().await;
                shared.persist_session();
            }
        });
    }

    /// Queues a coalesced settings save.
    pub fn persist_settings(&self) {
        let _ = self.persist_tx.send(PersistKind::Settings);
    }

    /// Queues a coalesced session save.
    pub fn persist_session(&self) {
        let _ = self.persist_tx.send(PersistKind::Session);
    }

    /// Writes settings and session immediately; used on shutdown.
    pub async fn save_all_now(&self) {
        let settings = self.snapshot_settings().await;
        if let Err(e) = persistence::save_settings(&settings).await {
            tracing::warn!("Failed to save settings on shutdown: {}", e);
        }
        let session = self.snapshot_session();
        if let Err(e) = persistence::save_session(&session).await {
            tracing::warn!("Failed to save session on shutdown: {}", e);
        }
    }

    async fn snapshot_settings(&self) -> persistence::PersistedSettings {
        // Take everything behind sync locks first; the guards must not live
        // across the awaits below.
        let (download_limit, upload_limit) = *self.bandwidth_limits.read();
        let categories: Vec<settings::Category> =
            self.categories.read().values().cloned().collect();
        let auto_trackers = self.auto_tracker_settings.read().clone();
        let move_on_complete = self.move_on_complete_settings.read().clone();
        let external_program = self.external_program_settings.read().clone();
        let default_share_limits = self.default_share_limits.read().clone();

        persistence::PersistedSettings {
            download_dir: Some(self.download_dir.clone()),
            categories,
            auto_trackers,
            move_on_complete,
            external_program,
            default_share_limits,
            watch_folders: self.watch_manager.get_folders().await,
            rss_feeds: self.rss_manager.get_feeds().await,
            rss_rules: self.rss_manager.get_rules().await,
            rss_downloaded: self.rss_manager.get_downloaded_history().await,
            max_active_downloads: self.max_active_downloads.load(Ordering::Relaxed),
            max_active_uploads: self.max_active_uploads.load(Ordering::Relaxed),
            download_limit,
            upload_limit,
            no_seed_mode: self.no_seed_mode.load(Ordering::Acquire),
            disconnect_on_complete: self.disconnect_on_complete.load(Ordering::Acquire),
        }
    }

    fn snapshot_session(&self) -> persistence::PersistedSession {
        let torrents = self.torrents.read();
        let mut entries: Vec<&ManagedTorrent> = torrents.values().collect();
        // Preserve queue (FIFO) order across restarts.
        entries.sort_by_key(|t| t.added_at);

        let torrent_entries = entries
            .into_iter()
            .map(|t| persistence::PersistedTorrent {
                info_hash: t.info_hash_hex(),
                save_path: t.save_path.clone(),
                category: t.category.clone(),
                tags: t.tags.iter().cloned().collect(),
                share_limits: t.share_limits.clone(),
                file_priorities: t.file_priorities.iter().map(|p| *p as u8).collect(),
                sequential_download: t.sequential_download,
                paused: t.state == TorrentState::Paused,
                uploaded: t.stats.uploaded,
                trackers: t.trackers.clone(),
                move_on_complete: t.move_on_complete.clone(),
            })
            .collect();

        let pending_magnets = self
            .pending_magnets
            .read()
            .values()
            .filter(|p| p.error.is_none())
            .map(|p| p.uri.clone())
            .collect();

        persistence::PersistedSession {
            torrents: torrent_entries,
            pending_magnets,
        }
    }
}

#[allow(dead_code)]
impl TorrentEngine {
    /// Creates a new TorrentEngine with the specified download directory.
    pub async fn new(download_dir: PathBuf) -> Result<Self, EngineError> {
        tokio::fs::create_dir_all(&download_dir).await?;

        let peer_id = PeerId::generate();
        let listen_port = Arc::new(AtomicU16::new(DEFAULT_PORT));

        let dht = match DhtServer::bind(0).await {
            Ok(server) => {
                let server = Arc::new(server);

                let server_for_run = server.clone();
                tokio::spawn(async move {
                    if let Err(e) = server_for_run.run().await {
                        tracing::error!("DHT run loop error: {}", e);
                    }
                });

                let server_for_bootstrap = server.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(CONNECTION_RETRY_SLEEP).await;
                    if let Err(e) = server_for_bootstrap.bootstrap().await {
                        tracing::error!("DHT bootstrap error: {}", e);
                    }
                });

                Some(server)
            }
            Err(e) => {
                tracing::warn!("Failed to start DHT: {}", e);
                None
            }
        };

        let initial_port = listen_port.load(Ordering::Relaxed);
        let mut port_mapper = PortMapper::new();
        if port_mapper.discover().await.is_ok() {
            let mapping = PortMapping {
                internal_port: initial_port,
                external_port: initial_port,
                protocol: Protocol::Tcp,
                lifetime: 3600,
            };
            if let Err(e) = port_mapper.add_mapping(&mapping).await {
                tracing::warn!("Failed to map port: {}", e);
            }
        }

        let lsd = match LsdService::new().await {
            Ok(service) => Some(Arc::new(service)),
            Err(e) => {
                tracing::warn!("Failed to start LSD: {}", e);
                None
            }
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Create watch folder manager
        let (watch_manager, watch_rx) = watch::WatchFolderManager::new();
        let watch_manager = Arc::new(watch_manager);

        // Create RSS manager
        let (rss_manager, rss_rx) = rss::RssManager::new();
        let rss_manager = Arc::new(rss_manager);

        // Create search engine
        let plugin_dir = download_dir
            .parent()
            .unwrap_or(&download_dir)
            .join(".rbitt")
            .join("search_plugins");
        let search_engine = Arc::new(search::SearchEngine::new(plugin_dir));

        let (persist_tx, persist_rx) = mpsc::unbounded_channel();

        let shared = Arc::new(EngineShared {
            peer_id,
            download_dir: download_dir.clone(),
            listen_port,
            torrents: Arc::new(RwLock::new(HashMap::new())),
            tracker_client: Arc::new(TrackerClient::new()),
            disk_manager: Arc::new(DiskManager::new()),
            bandwidth_limiter: Arc::new(RwLock::new(BandwidthLimiter::unlimited())),
            dht,
            lsd,
            event_tx,
            global_connections: Arc::new(AtomicUsize::new(0)),
            max_active_downloads: Arc::new(AtomicUsize::new(5)),
            max_active_uploads: Arc::new(AtomicUsize::new(5)),
            no_seed_mode: Arc::new(AtomicBool::new(false)),
            disconnect_on_complete: Arc::new(AtomicBool::new(false)),
            categories: Arc::new(RwLock::new(HashMap::new())),
            auto_tracker_settings: Arc::new(RwLock::new(settings::AutoTrackerSettings::default())),
            move_on_complete_settings: Arc::new(RwLock::new(
                settings::MoveOnCompleteSettings::default(),
            )),
            external_program_settings: Arc::new(RwLock::new(
                settings::ExternalProgramSettings::default(),
            )),
            default_share_limits: Arc::new(RwLock::new(settings::ShareLimits::default())),
            pending_magnets: RwLock::new(HashMap::new()),
            bandwidth_limits: RwLock::new((0, 0)),
            watch_manager: watch_manager.clone(),
            rss_manager: rss_manager.clone(),
            search_engine,
            persist_tx,
        });

        let engine = Self {
            shared: shared.clone(),
            _port_mapper: Arc::new(RwLock::new(port_mapper)),
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
        };

        // Apply persisted settings before anything starts producing events.
        shared.apply_persisted_settings().await;

        engine.start_background_tasks();
        shared.start_watch_folder_processor(watch_rx);
        shared.start_rss_processor(rss_rx);
        shared.start_share_limits_checker();
        shared.start_persistence_writer(persist_rx);
        shared.start_periodic_session_flush();

        // Start watch folder and RSS managers
        watch_manager.start();
        rss_manager.start();

        // Restore the previous session in the background; torrents reappear
        // as they re-verify.
        {
            let shared = shared.clone();
            tokio::spawn(async move {
                shared.restore_session().await;
            });
        }

        // Capture download_dir (and defaults) on first run.
        shared.persist_settings();

        Ok(engine)
    }

    fn start_background_tasks(&self) {
        self.start_listener();
        self.start_event_processor();
        self.start_lsd_announcer();
        self.start_lsd_receiver();
        self.start_reannounce_task();
        self.start_dht_discovery_task();
        self.start_stale_piece_cleanup_task();
    }

    fn start_listener(&self) {
        let listen_port = self.listen_port.clone();
        let initial_port = listen_port.load(Ordering::Relaxed);
        let torrents = self.torrents.clone();
        let peer_id = self.peer_id;
        let disk_manager = self.disk_manager.clone();
        let bandwidth_limiter = self.bandwidth_limiter.clone();
        let event_tx = self.event_tx.clone();
        let global_connections = self.global_connections.clone();
        let no_seed_mode = self.no_seed_mode.clone();

        tokio::spawn(async move {
            let listener = match TcpListener::bind(format!("0.0.0.0:{}", initial_port)).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!("Failed to bind listener on port {}: {}", initial_port, e);
                    match TcpListener::bind("0.0.0.0:0").await {
                        Ok(l) => {
                            let actual_port = l.local_addr().unwrap().port();
                            listen_port.store(actual_port, Ordering::Relaxed);
                            tracing::info!("Listening on fallback port {}", actual_port);
                            l
                        }
                        Err(e) => {
                            tracing::error!("Failed to bind any port: {}", e);
                            return;
                        }
                    }
                }
            };

            tracing::info!(
                "Listening for incoming connections on {}",
                listener.local_addr().unwrap()
            );

            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        // Check global connection limit before accepting
                        let current = global_connections.load(Ordering::Relaxed);
                        if current >= MAX_GLOBAL_CONNECTIONS {
                            tracing::debug!(
                                "Rejecting incoming connection from {} (global limit {} reached)",
                                addr,
                                MAX_GLOBAL_CONNECTIONS
                            );
                            drop(stream);
                            continue;
                        }

                        tracing::debug!("Incoming connection from {}", addr);
                        let torrents = torrents.clone();
                        let disk_manager = disk_manager.clone();
                        let bandwidth_limiter = bandwidth_limiter.clone();
                        let event_tx = event_tx.clone();
                        let port = listen_port.load(Ordering::Relaxed);
                        let global_conns = global_connections.clone();
                        let no_seed = no_seed_mode.clone();

                        // Increment global connection counter
                        global_conns.fetch_add(1, Ordering::Relaxed);

                        tokio::spawn(async move {
                            let _ = peer_connection::handle_incoming_connection(
                                stream,
                                addr,
                                peer_id,
                                torrents,
                                disk_manager,
                                bandwidth_limiter,
                                event_tx,
                                port,
                                no_seed,
                            )
                            .await;
                            // Decrement global connection counter when done
                            global_conns.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to accept connection: {}", e);
                    }
                }
            }
        });
    }

    fn start_event_processor(&self) {
        let shared = self.shared.clone();
        let torrents = self.torrents.clone();
        let event_rx = self.event_rx.clone();
        let no_seed_mode = self.no_seed_mode.clone();
        let disconnect_on_complete = self.disconnect_on_complete.clone();

        tokio::spawn(async move {
            let mut rx = match event_rx.write().take() {
                Some(rx) => rx,
                None => return,
            };

            while let Some(event) = rx.recv().await {
                match event {
                    PeerEvent::Connected {
                        torrent_hash,
                        peer_addr,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            torrent.connecting_peers.remove(&peer_addr);
                            torrent.failed_peers.remove(&peer_addr);
                            torrent.peers.insert(peer_addr, PeerInfo::new());
                            tracing::debug!("Peer {} connected for {}", peer_addr, torrent_hash);
                        }
                    }
                    PeerEvent::Disconnected {
                        torrent_hash,
                        peer_addr,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            if let Some(peer_info) = torrent.peers.get(&peer_addr) {
                                if let Some(ref bf) = peer_info.bitfield {
                                    torrent.piece_manager.decrement_availability(bf);
                                }
                            }
                            torrent.peers.remove(&peer_addr);
                            torrent.connecting_peers.remove(&peer_addr);
                            torrent.unchoked_peers.remove(&peer_addr);
                            tracing::debug!(
                                "Peer {} disconnected from {}",
                                peer_addr,
                                torrent_hash
                            );
                        }
                    }
                    PeerEvent::PieceCompleted {
                        torrent_hash,
                        piece_index,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            torrent.piece_manager.mark_piece_complete(piece_index);
                            tracing::debug!("Piece {} completed for {}", piece_index, torrent_hash,);

                            if torrent.piece_manager.is_complete()
                                && torrent.state == TorrentState::Downloading
                            {
                                torrent.state = TorrentState::Completed;
                                if torrent.seeding_started_at.is_none() {
                                    torrent.seeding_started_at = Some(Instant::now());
                                }
                                tracing::info!("Torrent {} download complete!", torrent_hash);

                                if !torrent.completion_processed {
                                    torrent.completion_processed = true;
                                    let shared = shared.clone();
                                    let hash = torrent_hash.clone();
                                    tokio::spawn(async move {
                                        shared.handle_torrent_completed(hash).await;
                                    });
                                }

                                // If no_seed_mode and disconnect_on_complete are both enabled,
                                // disconnect all peers for this completed torrent
                                if no_seed_mode.load(Ordering::Acquire)
                                    && disconnect_on_complete.load(Ordering::Acquire)
                                {
                                    tracing::info!(
                                        "Disconnecting all peers for completed torrent {} (no-seed mode with disconnect on complete)",
                                        torrent_hash
                                    );
                                    let _ = torrent.shutdown_tx.send(());
                                    torrent.peers.clear();
                                    torrent.connecting_peers.clear();
                                    torrent.unchoked_peers.clear();
                                    torrent.stats.download_rate = 0.0;
                                    torrent.stats.upload_rate = 0.0;
                                }
                            }
                        }
                    }
                    PeerEvent::BlockReceived {
                        torrent_hash,
                        peer_addr,
                        size,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            torrent.stats.downloaded += size;
                            if let Some(peer) = torrent.peers.get_mut(&peer_addr) {
                                peer.download_bytes += size;
                                peer.last_active = Instant::now();
                            }
                        }
                    }
                    PeerEvent::BlockSent {
                        torrent_hash,
                        peer_addr,
                        size,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            torrent.stats.uploaded += size;
                            if let Some(peer) = torrent.peers.get_mut(&peer_addr) {
                                peer.upload_bytes += size;
                                peer.last_active = Instant::now();
                            }
                        }
                    }
                    PeerEvent::PeerBitfield {
                        torrent_hash,
                        peer_addr,
                        bitfield,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            torrent.piece_manager.update_availability(&bitfield);

                            if let Some(peer) = torrent.peers.get_mut(&peer_addr) {
                                peer.bitfield = Some(bitfield);
                            } else {
                                tracing::debug!(
                                    "Received bitfield for unregistered peer {} on torrent {}",
                                    peer_addr,
                                    torrent_hash
                                );
                            }
                        }
                    }
                    PeerEvent::PeerState {
                        torrent_hash,
                        peer_addr,
                        is_choking_us,
                        is_interested,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            if let Some(peer) = torrent.peers.get_mut(&peer_addr) {
                                peer.is_choking_us = is_choking_us;
                                peer.is_interested = is_interested;
                                peer.last_active = Instant::now();
                            }
                        }
                    }
                    PeerEvent::NewPeers {
                        torrent_hash,
                        peers,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            let mut added = 0;
                            for peer in peers {
                                if !torrent.peers.contains_key(&peer)
                                    && !torrent.connecting_peers.contains(&peer)
                                    && !torrent.failed_peers.contains_key(&peer)
                                    && !torrent.known_peers.contains(&peer)
                                {
                                    torrent.known_peers.insert(peer);
                                    added += 1;
                                }
                            }
                            if added > 0 {
                                tracing::debug!(
                                    "PEX: Added {} new peers for {} (total known: {})",
                                    added,
                                    &torrent_hash[..8.min(torrent_hash.len())],
                                    torrent.known_peers.len()
                                );
                            }
                        }
                    }
                    PeerEvent::PeerHave {
                        torrent_hash,
                        peer_addr,
                        piece_index,
                    } => {
                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&torrent_hash) {
                            if let Some(peer) = torrent.peers.get_mut(&peer_addr) {
                                if let Some(ref mut bf) = peer.bitfield {
                                    bf.set_piece(piece_index as usize);
                                } else {
                                    let piece_count = torrent.meta.piece_count();
                                    let mut bf = Bitfield::new(piece_count);
                                    bf.set_piece(piece_index as usize);
                                    peer.bitfield = Some(bf);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    fn start_lsd_announcer(&self) {
        let lsd = match &self.lsd {
            Some(lsd) => lsd.clone(),
            None => return,
        };

        let torrents = self.torrents.clone();
        let listen_port = self.listen_port.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(LSD_ANNOUNCE_INTERVAL);

            loop {
                interval.tick().await;

                let hashes: Vec<[u8; 20]> = {
                    let torrents = torrents.read();
                    torrents
                        .values()
                        .filter(|t| {
                            matches!(
                                t.state,
                                TorrentState::Downloading
                                    | TorrentState::Seeding
                                    | TorrentState::Completed
                            )
                        })
                        .filter_map(|t| t.info_hash_bytes())
                        .collect()
                };

                let port = listen_port.load(Ordering::Relaxed);
                for info_hash in hashes {
                    if let Err(e) = lsd.announce(&info_hash, port).await {
                        tracing::debug!("LSD announce failed: {}", e);
                    }
                }
            }
        });
    }

    fn start_lsd_receiver(&self) {
        let lsd = match &self.lsd {
            Some(lsd) => lsd.clone(),
            None => return,
        };

        // Single receive loop inside the service feeds the broadcast channel.
        lsd.start_receiver();

        let torrents = self.torrents.clone();
        let mut rx = lsd.subscribe();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(announce) => {
                        let info_hash_hex = announce.info_hash.iter().fold(
                            String::with_capacity(40),
                            |mut s, b| {
                                use std::fmt::Write;
                                let _ = write!(s, "{:02x}", b);
                                s
                            },
                        );

                        let mut torrents = torrents.write();
                        if let Some(torrent) = torrents.get_mut(&info_hash_hex) {
                            let peer_addr = SocketAddr::new(announce.source.ip(), announce.port);

                            if !torrent.peers.contains_key(&peer_addr)
                                && !torrent.connecting_peers.contains(&peer_addr)
                                && !torrent.known_peers.contains(&peer_addr)
                            {
                                tracing::info!(
                                    "LSD: Discovered local peer {} for torrent {}",
                                    peer_addr,
                                    &info_hash_hex[..8]
                                );
                                torrent.known_peers.insert(peer_addr);
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("LSD receiver lagged by {} messages", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!("LSD broadcast channel closed");
                        break;
                    }
                }
            }
        });
    }

    fn start_dht_discovery_task(&self) {
        let dht = match &self.dht {
            Some(dht) => dht.clone(),
            None => return,
        };

        let torrents = self.torrents.clone();
        let listen_port = self.listen_port.clone();

        tokio::spawn(async move {
            tokio::time::sleep(CONNECTION_TIMEOUT).await;

            loop {
                let (next_interval, torrents_to_query): (Duration, Vec<(String, [u8; 20], usize)>) = {
                    let torrents = torrents.read();

                    let mut min_peers = usize::MAX;
                    let mut queries = Vec::new();

                    for (hash, t) in torrents.iter() {
                        if t.state != TorrentState::Downloading && t.state != TorrentState::Seeding
                        {
                            continue;
                        }

                        let connected_peers = t.peers.len();
                        min_peers = min_peers.min(connected_peers);

                        let torrent_interval = if connected_peers < PEER_THRESHOLD_CRITICAL {
                            DHT_INTERVAL_CRITICAL
                        } else if connected_peers < PEER_THRESHOLD_LOW {
                            DHT_INTERVAL_LOW
                        } else if connected_peers < PEER_THRESHOLD_MEDIUM {
                            DHT_INTERVAL_MEDIUM
                        } else {
                            DHT_INTERVAL_HIGH
                        };

                        let should_query = t
                            .last_dht_query
                            .map(|last| last.elapsed() >= torrent_interval)
                            .unwrap_or(true);

                        if should_query {
                            if let Some(ih) = t.info_hash_bytes() {
                                queries.push((hash.clone(), ih, connected_peers));
                            }
                        }
                    }

                    let interval = if min_peers < PEER_THRESHOLD_CRITICAL {
                        DHT_INTERVAL_CRITICAL
                    } else if min_peers < PEER_THRESHOLD_LOW {
                        DHT_INTERVAL_LOW
                    } else if min_peers < PEER_THRESHOLD_MEDIUM {
                        DHT_INTERVAL_MEDIUM
                    } else {
                        DHT_INTERVAL_HIGH
                    };

                    (interval, queries)
                };

                for (hash, info_hash, peer_count) in torrents_to_query {
                    let port = listen_port.load(Ordering::Relaxed);

                    match dht.get_peers_with_tokens(info_hash).await {
                        Ok((peers, tokens)) => {
                            let found_count = peers.len();

                            {
                                let mut torrents = torrents.write();
                                if let Some(torrent) = torrents.get_mut(&hash) {
                                    let mut added = 0;
                                    for peer in peers {
                                        if !torrent.peers.contains_key(&peer)
                                            && !torrent.connecting_peers.contains(&peer)
                                            && !torrent.known_peers.contains(&peer)
                                            && !torrent.failed_peers.contains_key(&peer)
                                        {
                                            torrent.known_peers.insert(peer);
                                            added += 1;
                                        }
                                    }
                                    torrent.last_dht_query = Some(Instant::now());

                                    if added > 0 {
                                        tracing::info!(
                                            "DHT discovery: Added {} new peers for {} (found {}, connected: {}, known: {})",
                                            added,
                                            &hash[..8],
                                            found_count,
                                            peer_count,
                                            torrent.known_peers.len()
                                        );
                                    }
                                }
                            }

                            if !tokens.is_empty() {
                                dht.announce_to_nodes(info_hash, port, tokens).await;
                            }
                        }
                        Err(e) => {
                            tracing::debug!("DHT discovery failed for {}: {}", &hash[..8], e);
                        }
                    }

                    tokio::time::sleep(DHT_QUERY_SLEEP).await;
                }

                tokio::time::sleep(next_interval).await;
            }
        });
    }

    /// Periodically cleans up stale piece states to prevent memory leaks.
    /// Pieces with expired requests are removed from active_pieces and
    /// made available for re-download.
    fn start_stale_piece_cleanup_task(&self) {
        let torrents = self.torrents.clone();

        tokio::spawn(async move {
            const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

            loop {
                tokio::time::sleep(CLEANUP_INTERVAL).await;

                let hashes: Vec<String> = {
                    let torrents = torrents.read();
                    torrents
                        .iter()
                        .filter(|(_, t)| t.state == TorrentState::Downloading)
                        .map(|(h, _)| h.clone())
                        .collect()
                };

                for hash in hashes {
                    let stale_count = {
                        let torrents = torrents.read();
                        if let Some(torrent) = torrents.get(&hash) {
                            let stale = torrent.piece_manager.cleanup_stale_pieces();
                            if !stale.is_empty() {
                                tracing::debug!(
                                    "Cleaned up {} stale pieces for torrent {}",
                                    stale.len(),
                                    hash
                                );
                            }
                            stale.len()
                        } else {
                            0
                        }
                    };

                    if stale_count > 0 {
                        tracing::info!(
                            "Cleaned {} stale piece states for {} - freeing memory",
                            stale_count,
                            hash
                        );
                    }
                }
            }
        });
    }

    fn start_reannounce_task(&self) {
        let torrents = self.torrents.clone();
        let tracker_client = self.tracker_client.clone();
        let peer_id = self.peer_id;
        let listen_port = self.listen_port.clone();
        let dht = self.dht.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(REANNOUNCE_CHECK_INTERVAL);

            loop {
                interval.tick().await;

                let torrents_to_announce: Vec<_> = {
                    let torrents = torrents.read();
                    torrents
                        .iter()
                        .filter(|(_, t)| {
                            t.state == TorrentState::Downloading || t.state == TorrentState::Seeding
                        })
                        .filter(|(_, t)| {
                            let connected_peers = t.peers.len();
                            let reannounce_interval = if connected_peers < PEER_THRESHOLD_CRITICAL {
                                TRACKER_AGGRESSIVE_INTERVAL.max(TRACKER_MIN_INTERVAL)
                            } else if connected_peers < PEER_THRESHOLD_LOW {
                                TRACKER_MODERATE_INTERVAL.max(TRACKER_MIN_INTERVAL)
                            } else {
                                TRACKER_ANNOUNCE_INTERVAL
                            };

                            t.last_announce
                                .map(|la| la.elapsed() >= reannounce_interval)
                                .unwrap_or(true)
                        })
                        .map(|(hash, t)| {
                            let verified_bytes =
                                t.piece_manager.have_count() as u64 * t.meta.info.piece_length;
                            let left = if t.piece_manager.is_complete() {
                                0
                            } else {
                                t.meta.info.total_length.saturating_sub(verified_bytes)
                            };
                            (
                                hash.clone(),
                                t.trackers.clone(),
                                t.info_hash_bytes(),
                                t.meta.info.total_length,
                                t.stats.uploaded,
                                t.stats.downloaded,
                                left,
                                t.peers.len(),
                            )
                        })
                        .collect()
                };

                for (
                    hash,
                    trackers,
                    info_hash_bytes,
                    _total_length,
                    uploaded,
                    downloaded,
                    left,
                    peer_count,
                ) in torrents_to_announce
                {
                    let Some(info_hash) = info_hash_bytes else {
                        continue;
                    };

                    if peer_count < PEER_THRESHOLD_LOW {
                        tracing::debug!(
                            "Tracker reannounce for {} (low peers: {})",
                            &hash[..8],
                            peer_count
                        );
                    }

                    for url in &trackers {
                        let v1_hash = match oxidebt_torrent::InfoHashV1::from_bytes(&info_hash) {
                            Ok(h) => h,
                            Err(_) => continue,
                        };
                        let params = AnnounceParams {
                            url,
                            info_hash: &v1_hash,
                            peer_id: peer_id.as_bytes(),
                            port: listen_port.load(Ordering::Relaxed),
                            uploaded,
                            downloaded,
                            left,
                            event: TrackerEvent::None,
                        };

                        match tracker_client.announce(params).await {
                            Ok(response) => {
                                let mut torrents = torrents.write();
                                if let Some(torrent) = torrents.get_mut(&hash) {
                                    for peer in response.peers.iter().chain(response.peers6.iter())
                                    {
                                        if !torrent.peers.contains_key(&peer.addr)
                                            && !torrent.connecting_peers.contains(&peer.addr)
                                        {
                                            torrent.known_peers.insert(peer.addr);
                                        }
                                    }
                                    torrent.last_announce = Some(Instant::now());
                                }
                                break;
                            }
                            Err(e) => {
                                tracing::debug!("Tracker {} reannounce failed: {}", url, e);
                            }
                        }
                    }

                    if let Some(ref dht) = dht {
                        let port = listen_port.load(Ordering::Relaxed);
                        match dht.get_peers_with_tokens(info_hash).await {
                            Ok((peers, tokens)) => {
                                {
                                    let mut torrents = torrents.write();
                                    if let Some(torrent) = torrents.get_mut(&hash) {
                                        for peer in peers {
                                            if !torrent.peers.contains_key(&peer)
                                                && !torrent.connecting_peers.contains(&peer)
                                            {
                                                torrent.known_peers.insert(peer);
                                            }
                                        }
                                    }
                                }
                                if !tokens.is_empty() {
                                    tracing::debug!(
                                        "DHT: Announcing {} to {} nodes",
                                        &hash[..8],
                                        tokens.len()
                                    );
                                    dht.announce_to_nodes(info_hash, port, tokens).await;
                                }
                            }
                            Err(e) => {
                                tracing::debug!("DHT get_peers failed for {}: {}", &hash[..8], e);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Sets global bandwidth limits.
    pub fn set_bandwidth_limits(&self, download_limit: u64, upload_limit: u64) {
        {
            let mut limiter = self.bandwidth_limiter.write();
            limiter.set_download_limit(download_limit);
            limiter.set_upload_limit(upload_limit);
        }
        *self.bandwidth_limits.write() = (download_limit, upload_limit);
        self.persist_settings();
    }

    /// Returns the configured bandwidth limits (download, upload) in
    /// bytes/sec; 0 means unlimited.
    pub fn get_bandwidth_limits(&self) -> (u64, u64) {
        *self.bandwidth_limits.read()
    }

    /// Sets no seed mode. When enabled, the client rejects all upload requests
    /// and torrents will not transition to seeding state.
    pub fn set_no_seed_mode(&self, enabled: bool) {
        self.no_seed_mode.store(enabled, Ordering::Release);
        tracing::info!(
            "No Seed Mode {}",
            if enabled { "enabled" } else { "disabled" }
        );
        self.persist_settings();
    }

    /// Returns whether no seed mode is currently enabled.
    pub fn is_no_seed_mode(&self) -> bool {
        self.no_seed_mode.load(Ordering::Acquire)
    }

    /// Sets disconnect on complete mode. When enabled along with no_seed_mode,
    /// all peers will be disconnected when a torrent completes.
    pub fn set_disconnect_on_complete(&self, enabled: bool) {
        self.disconnect_on_complete
            .store(enabled, Ordering::Release);
        tracing::info!(
            "Disconnect on Complete {}",
            if enabled { "enabled" } else { "disabled" }
        );
        self.persist_settings();
    }

    /// Returns whether disconnect on complete mode is currently enabled.
    pub fn is_disconnect_on_complete(&self) -> bool {
        self.disconnect_on_complete.load(Ordering::Acquire)
    }

    /// Adds a torrent from a file path.
    pub async fn add_torrent_file(&self, path: &str) -> Result<String, EngineError> {
        let data = tokio::fs::read(path).await?;
        let meta = Metainfo::from_bytes(&data)?;
        self.add_torrent(meta, AddTorrentOptions::default()).await
    }

    /// Adds a torrent from raw bytes.
    pub async fn add_torrent_bytes(&self, data: &[u8]) -> Result<String, EngineError> {
        let meta = Metainfo::from_bytes(data)?;
        self.add_torrent(meta, AddTorrentOptions::default()).await
    }

    /// Adds a torrent from a magnet link URI.
    pub async fn add_magnet(&self, uri: &str) -> Result<String, EngineError> {
        self.shared
            .add_magnet(uri, AddTorrentOptions::default())
            .await
    }
}

impl EngineShared {
    /// Starts adding a torrent from a magnet link. Returns the info hash
    /// immediately; metadata discovery and fetch run in the background, with
    /// progress surfaced through `pending_magnets` as a metaDL entry that
    /// turns into a real torrent (or an error entry) when the fetch ends.
    pub async fn add_magnet(
        self: &Arc<Self>,
        uri: &str,
        opts: AddTorrentOptions,
    ) -> Result<String, EngineError> {
        let magnet = MagnetLink::parse(uri)?;

        let hash = match &magnet.info_hash {
            InfoHash::V1(h) => h.to_hex(),
            InfoHash::V2(h) => h.to_hex(),
            InfoHash::Hybrid { v1, .. } => v1.to_hex(),
        };

        if self.torrents.read().contains_key(&hash) {
            return Err(EngineError::Duplicate(hash));
        }

        let name = magnet
            .display_name
            .clone()
            .unwrap_or_else(|| hash[..16.min(hash.len())].to_string());

        {
            let mut pending = self.pending_magnets.write();
            if pending.contains_key(&hash) {
                return Err(EngineError::Duplicate(hash));
            }
            pending.insert(
                hash.clone(),
                PendingMagnet {
                    uri: uri.to_string(),
                    name,
                    error: None,
                },
            );
        }
        self.persist_session();

        let shared = self.clone();
        let hash_for_fetch = hash.clone();
        tokio::spawn(async move {
            if let Err(msg) = shared
                .fetch_magnet_metadata(&hash_for_fetch, magnet, opts)
                .await
            {
                tracing::warn!("Magnet {}: {}", hash_for_fetch, msg);
                let mut pending = shared.pending_magnets.write();
                if let Some(entry) = pending.get_mut(&hash_for_fetch) {
                    entry.error = Some(msg);
                }
            }
        });

        Ok(hash)
    }

    /// Background half of a magnet add: discover peers via DHT and trackers,
    /// fetch the metainfo over ut_metadata, verify it against the info hash,
    /// then route into the normal add path.
    async fn fetch_magnet_metadata(
        self: &Arc<Self>,
        hash: &str,
        magnet: MagnetLink,
        opts: AddTorrentOptions,
    ) -> Result<(), String> {
        let info_hash_bytes = match &magnet.info_hash {
            InfoHash::V1(h) => *h.as_bytes(),
            // BEP 52: DHT/tracker operations for v2 use the truncated hash.
            InfoHash::V2(h) => {
                let mut arr = [0u8; 20];
                arr.copy_from_slice(&h.as_bytes()[..20]);
                arr
            }
            InfoHash::Hybrid { v1, .. } => *v1.as_bytes(),
        };

        let mut peers = HashSet::new();

        if let Some(ref dht) = self.dht {
            if let Ok(dht_peers) = dht.get_peers(info_hash_bytes).await {
                peers.extend(dht_peers);
            }
        }

        for url in &magnet.trackers {
            if let Ok(v1_hash) = oxidebt_torrent::InfoHashV1::from_bytes(&info_hash_bytes) {
                let params = AnnounceParams {
                    url,
                    info_hash: &v1_hash,
                    peer_id: self.peer_id.as_bytes(),
                    port: self.listen_port.load(Ordering::Relaxed),
                    uploaded: 0,
                    downloaded: 0,
                    // Size unknown until we have metadata; any non-zero value
                    // avoids being counted as a seed.
                    left: 1,
                    event: TrackerEvent::Started,
                };

                if let Ok(response) = self.tracker_client.announce(params).await {
                    for peer in response.peers.iter().chain(response.peers6.iter()) {
                        peers.insert(peer.addr);
                    }
                }
            }
        }

        if peers.is_empty() {
            return Err("no peers found".to_string());
        }

        let metadata =
            fetch_metadata_from_peers(peers.into_iter().collect(), info_hash_bytes, self.peer_id)
                .await
                .ok_or_else(|| "failed to fetch metadata from peers".to_string())?;

        let mut hasher = Sha1::new();
        hasher.update(&metadata);
        let computed_hash: [u8; 20] = hasher.finalize().into();
        if computed_hash != info_hash_bytes {
            return Err("metadata hash mismatch".to_string());
        }

        let meta = Metainfo::from_info_dict(&metadata, &magnet.trackers)
            .map_err(|e| format!("failed to parse fetched metadata: {}", e))?;

        // The user may have removed the pending entry while we fetched.
        if !self.pending_magnets.read().contains_key(hash) {
            tracing::debug!("Magnet {} was removed during metadata fetch", hash);
            return Ok(());
        }

        self.add_torrent(meta, opts)
            .await
            .map_err(|e| format!("failed to add torrent from fetched metadata: {}", e))?;

        self.pending_magnets.write().remove(hash);
        Ok(())
    }
    /// Adds a torrent to the engine. Every add source (UI, watch folders,
    /// RSS, magnet metadata, session restore) routes through here so disk
    /// registration, verification, auto-trackers, and queueing behave the
    /// same everywhere.
    pub async fn add_torrent(
        self: &Arc<Self>,
        meta: Metainfo,
        opts: AddTorrentOptions,
    ) -> Result<String, EngineError> {
        let hash = match &meta.info_hash {
            InfoHash::V1(h) => h.to_hex(),
            InfoHash::V2(h) => h.to_hex(),
            InfoHash::Hybrid { v1, .. } => v1.to_hex(),
        };

        // A re-add would silently overwrite live state (peers, progress,
        // disk registration) — reject it instead.
        if self.torrents.read().contains_key(&hash) {
            return Err(EngineError::Duplicate(hash));
        }

        // The wire protocol, trackers, and DHT all run on v1 hashes here; a
        // v2-only torrent would sit in the list forever unable to connect.
        if meta.info_hash.v1().is_none() {
            return Err(EngineError::UnsupportedV2Only);
        }

        let tracker_count = meta.tracker_urls().len();
        tracing::info!(
            "add_torrent: Adding torrent '{}' with hash {}, {} pieces, {} trackers",
            meta.info.name,
            hash,
            meta.piece_count(),
            tracker_count
        );

        // Resolve the save directory: explicit override > category path >
        // engine download directory.
        let save_dir = if let Some(path) = opts.save_path {
            path
        } else if let Some(ref cat_name) = opts.category {
            let categories = self.categories.read();
            categories
                .get(cat_name)
                .map(|c| c.save_path.clone())
                .unwrap_or_else(|| self.download_dir.clone())
        } else {
            self.download_dir.clone()
        };

        let storage = Self::build_storage(&meta, &save_dir)?;

        storage.preallocate().await?;
        self.disk_manager.register(hash.clone(), storage);

        let mut managed = ManagedTorrent::with_save_path(meta, save_dir);
        managed.category = opts.category;
        managed.tags = opts.tags.into_iter().collect();
        managed.share_limits = opts
            .share_limits
            .unwrap_or_else(|| self.default_share_limits.read().clone());

        // Inject configured auto-trackers into the live tracker list.
        {
            let auto = self.auto_tracker_settings.read();
            if auto.enabled {
                for url in &auto.trackers {
                    if !managed.trackers.contains(url) {
                        managed.trackers.push(url.clone());
                        managed
                            .tracker_info
                            .push(tracker_info::TrackerInfo::new(url.clone()));
                    }
                }
            }
        }

        let add_paused = opts.add_paused;
        if add_paused {
            managed.state = TorrentState::Paused;
        }

        // Capture the info dict for session persistence before the metainfo
        // moves into the torrent map.
        let raw_info = bytes::Bytes::copy_from_slice(managed.meta.raw_info());

        self.torrents.write().insert(hash.clone(), managed);

        {
            let shared = self.clone();
            let hash = hash.clone();
            tokio::spawn(async move {
                if let Err(e) = persistence::save_torrent_blob(&hash, &raw_info).await {
                    tracing::warn!("Failed to persist torrent blob for {}: {}", hash, e);
                }
                shared.persist_session();
            });
        }

        // Verify pre-existing data on disk. This restores progress for
        // re-added torrents and runs for paused adds too, so resume knows
        // what is already present.
        let shared = self.clone();
        let hash_for_verify = hash.clone();
        tokio::spawn(async move {
            shared.verify_existing_pieces(hash_for_verify).await;
        });

        if !add_paused {
            let shared = self.clone();
            let hash_for_start = hash.clone();
            tokio::spawn(async move {
                if let Err(e) = shared.start_torrent(&hash_for_start).await {
                    tracing::error!("Failed to start torrent {}: {}", hash_for_start, e);
                }
            });
        }

        Ok(hash)
    }

    /// Streaming verification of whatever already exists on disk for this
    /// torrent, batched for parallelism. Transitions the torrent out of
    /// Checking when it finishes (unless it was added paused).
    async fn verify_existing_pieces(self: Arc<Self>, hash: String) {
        tracing::info!("Starting streaming piece verification for {}", hash);

        let piece_count = match self.disk_manager.piece_count(&hash) {
            Ok(count) => count,
            Err(e) => {
                tracing::warn!("Failed to get piece count for {}: {}", hash, e);
                let torrents = self.torrents.read();
                if let Some(torrent) = torrents.get(&hash) {
                    torrent.piece_manager.mark_verification_complete();
                }
                return;
            }
        };

        let mut valid_count = 0usize;
        let mut downloaded_bytes = 0u64;

        const BATCH_SIZE: usize = 32;

        for batch_start in (0..piece_count).step_by(BATCH_SIZE) {
            let batch_end = (batch_start + BATCH_SIZE).min(piece_count);

            // Create futures for all pieces in this batch
            let mut futures = Vec::with_capacity(batch_end - batch_start);
            for i in batch_start..batch_end {
                let dm = self.disk_manager.clone();
                let hash = hash.clone();
                futures.push(async move {
                    (
                        i as u32,
                        dm.verify_piece(&hash, i as u32).await.unwrap_or(false),
                    )
                });
            }

            // Run all verifications in parallel
            let batch_results = futures::future::join_all(futures).await;

            // Update piece manager with results
            {
                let torrents = self.torrents.read();
                if let Some(torrent) = torrents.get(&hash) {
                    for (piece_idx, is_valid) in batch_results {
                        if is_valid {
                            torrent.piece_manager.mark_piece_complete(piece_idx);
                            valid_count += 1;
                            downloaded_bytes += torrent.piece_size(piece_idx);
                        }
                        torrent.piece_manager.mark_piece_verified(piece_idx);
                    }
                } else {
                    return;
                }
            }

            // Progress logging
            if piece_count > 100 && batch_end % 100 < BATCH_SIZE {
                tracing::debug!(
                    "Verified {}/{} pieces for {} ({} valid so far)",
                    batch_end,
                    piece_count,
                    hash,
                    valid_count
                );
            }
        }

        {
            let mut torrents = self.torrents.write();
            // Get queue limits
            let max_dl = self.max_active_downloads.load(Ordering::Relaxed);
            let max_ul = self.max_active_uploads.load(Ordering::Relaxed);

            // First, update stats and mark verification complete, get info for queue calculation
            let transition_info = if let Some(torrent) = torrents.get_mut(&hash) {
                // Use set_downloaded_baseline to avoid a bogus download rate spike
                // (verified data wasn't downloaded this session, so shouldn't affect rate)
                torrent.stats.set_downloaded_baseline(downloaded_bytes);
                torrent.piece_manager.mark_verification_complete();

                let is_complete = torrent.piece_manager.is_complete();
                let added_at = torrent.added_at;
                let should_transition = torrent.state == TorrentState::Checking;
                tracing::info!(
                    "Piece verification complete for {}: {}/{} pieces valid, is_complete={}",
                    hash,
                    valid_count,
                    piece_count,
                    is_complete
                );
                if should_transition {
                    Some((is_complete, added_at))
                } else {
                    None
                }
            } else {
                None
            };

            // If we need to transition, calculate queue position and update state
            if let Some((is_complete, added_at)) = transition_info {
                // Calculate queue position
                let queue_position = if is_complete {
                    Self::queue_position_upload_from_map(&torrents, added_at)
                } else {
                    Self::queue_position_download_from_map(&torrents, added_at)
                };

                let should_queue = if is_complete {
                    max_ul > 0 && queue_position >= max_ul
                } else {
                    max_dl > 0 && queue_position >= max_dl
                };

                // Now update the state
                if let Some(torrent) = torrents.get_mut(&hash) {
                    // Data was complete before this session; never fire
                    // completion actions for it.
                    if is_complete {
                        torrent.completion_processed = true;
                    }
                    if should_queue {
                        torrent.state = TorrentState::Queued;
                        tracing::info!(
                            "Verification done: Torrent {} queued (position {} >= limit {})",
                            hash,
                            queue_position,
                            if is_complete { max_ul } else { max_dl }
                        );
                    } else if is_complete {
                        torrent.state = TorrentState::Completed;
                        if torrent.seeding_started_at.is_none() {
                            torrent.seeding_started_at = Some(Instant::now());
                        }
                        tracing::info!(
                            "Verification done: Torrent {} is complete, transitioning to Completed (position {})",
                            hash,
                            queue_position
                        );
                    } else {
                        torrent.state = TorrentState::Downloading;
                        tracing::info!(
                            "Verification done: Torrent {} transitioning to Downloading (position {})",
                            hash,
                            queue_position
                        );
                    }
                }
            }
        }
    }

    /// Builds the disk storage layout for a torrent rooted at `save_dir`.
    /// Single-file torrents live directly in `save_dir`; multi-file torrents
    /// get a `save_dir/<name>/` directory.
    fn build_storage(meta: &Metainfo, save_dir: &Path) -> Result<TorrentStorage, EngineError> {
        let is_single_file = meta.info.files.len() == 1
            && meta.info.files[0].path.to_string_lossy() == meta.info.name;

        let base_path = if is_single_file {
            save_dir.to_path_buf()
        } else {
            save_dir.join(&meta.info.name)
        };

        let files: Vec<FileEntry> = {
            let mut offset = 0u64;
            meta.info
                .files
                .iter()
                .map(|f| {
                    let entry = FileEntry::new(f.path.clone(), f.length, offset);
                    offset += f.length;
                    entry
                })
                .collect()
        };

        let is_v2 = matches!(meta.version, oxidebt_torrent::TorrentVersion::V2);
        let pieces: Vec<PieceInfo> = (0..meta.piece_count())
            .map(|i| {
                let offset = i as u64 * meta.info.piece_length;
                let length = if i == meta.piece_count() - 1 {
                    let rem = meta.info.total_length % meta.info.piece_length;
                    if rem == 0 {
                        meta.info.piece_length
                    } else {
                        rem
                    }
                } else {
                    meta.info.piece_length
                };
                let default_hash = [0u8; 20];
                let piece_hash = meta.piece_hash(i).unwrap_or(default_hash);
                if is_v2 {
                    let mut hash = [0u8; 32];
                    let len = piece_hash.len().min(32);
                    hash[..len].copy_from_slice(&piece_hash[..len]);
                    PieceInfo::v2(i as u32, hash, offset, length)
                } else {
                    let mut hash = [0u8; 20];
                    let len = piece_hash.len().min(20);
                    hash[..len].copy_from_slice(&piece_hash[..len]);
                    PieceInfo::v1(i as u32, hash, offset, length)
                }
            })
            .collect();

        Ok(TorrentStorage::new(
            base_path,
            files,
            pieces,
            meta.info.total_length,
            is_v2,
        )?)
    }

    /// Runs once per fresh download completion: moves the data if configured
    /// (per-torrent override > global target/category path), then runs the
    /// external completion program.
    async fn handle_torrent_completed(self: Arc<Self>, hash: String) {
        let (current_save, per_torrent_target, category) = {
            let torrents = self.torrents.read();
            let Some(torrent) = torrents.get(&hash) else {
                return;
            };
            (
                torrent.save_path.clone(),
                torrent.move_on_complete.clone(),
                torrent.category.clone(),
            )
        };

        let global = self.move_on_complete_settings.read().clone();
        let target = per_torrent_target.or_else(|| {
            if !global.enabled {
                return None;
            }
            if global.use_category_path {
                category
                    .and_then(|name| {
                        self.categories
                            .read()
                            .get(&name)
                            .map(|c| c.save_path.clone())
                    })
                    .or(global.target_path)
            } else {
                global.target_path
            }
        });

        if let Some(target) = target {
            if target != current_save {
                tracing::info!("Moving completed torrent {} to {:?}", hash, target);
                if let Err(e) = self.move_torrent_data(&hash, target).await {
                    tracing::error!("Failed to move completed torrent {}: {}", hash, e);
                }
            }
        }

        if let Err(e) = self.run_completion_program(&hash).await {
            tracing::error!("Completion program for {} failed: {}", hash, e);
        }
    }

    /// Moves a completed torrent's data to `target` and re-roots its disk
    /// storage there. Peers are disconnected for the duration (the torrent
    /// sits in Moving); the run loop reconnects them afterwards.
    async fn move_torrent_data(
        self: &Arc<Self>,
        hash: &str,
        target: PathBuf,
    ) -> Result<(), EngineError> {
        let (name, old_save) = {
            let mut torrents = self.torrents.write();
            let torrent = torrents
                .get_mut(hash)
                .ok_or_else(|| EngineError::NotFound(hash.to_string()))?;
            torrent.state = TorrentState::Moving;
            let _ = torrent.shutdown_tx.send(());
            torrent.peers.clear();
            torrent.connecting_peers.clear();
            torrent.unchoked_peers.clear();
            let (new_shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
            torrent.shutdown_tx = new_shutdown_tx;
            (torrent.meta.info.name.clone(), torrent.save_path.clone())
        };

        // Close file handles before touching the files.
        self.disk_manager.unregister(hash);

        let src = old_save.join(&name);
        let dst = target.join(&name);
        let move_result = Self::move_path(src, dst, &target).await;

        // Re-root storage at the new location on success, or back at the old
        // one on failure.
        let final_save = if move_result.is_ok() {
            target
        } else {
            old_save
        };

        {
            let mut torrents = self.torrents.write();
            if let Some(torrent) = torrents.get_mut(hash) {
                match Self::build_storage(&torrent.meta, &final_save) {
                    Ok(storage) => self.disk_manager.register(hash.to_string(), storage),
                    Err(e) => {
                        tracing::error!("Failed to rebuild storage for {}: {}", hash, e);
                    }
                }
                torrent.save_path = final_save;
                // Leave the state alone if the user paused/removed mid-move.
                if torrent.state == TorrentState::Moving {
                    torrent.state = TorrentState::Completed;
                }
            }
        }

        move_result
    }

    /// Escape a string for safe use in shell commands.
    /// This prevents command injection by escaping shell metacharacters.
    #[cfg(unix)]
    fn shell_escape(s: &str) -> String {
        // For Unix shells, wrap in single quotes and escape any single quotes
        // Single quotes preserve everything literally except single quotes themselves
        let escaped = s.replace('\'', "'\"'\"'");
        format!("'{}'", escaped)
    }

    #[cfg(windows)]
    fn shell_escape(s: &str) -> String {
        // For Windows cmd.exe, escape special characters
        // The safest approach is to wrap in double quotes and escape problematic chars
        let escaped = s
            .replace('^', "^^")
            .replace('&', "^&")
            .replace('<', "^<")
            .replace('>', "^>")
            .replace('|', "^|")
            .replace('%', "%%")
            .replace('"', "\"\"");
        format!("\"{}\"", escaped)
    }

    /// Run external program for a completed torrent
    pub async fn run_completion_program(&self, hash: &str) -> Result<(), String> {
        let settings = self.external_program_settings.read().clone();
        if !settings.on_completion_enabled {
            return Ok(());
        }

        let Some(command_template) = settings.on_completion_command else {
            return Ok(());
        };

        let (name, save_path) = {
            let torrents = self.torrents.read();
            let torrent = torrents.get(hash).ok_or("Torrent not found")?;
            (torrent.meta.info.name.clone(), torrent.save_path.clone())
        };

        // Escape all user-controlled values to prevent command injection
        let escaped_name = Self::shell_escape(&name);
        let escaped_full_path = Self::shell_escape(&save_path.join(&name).to_string_lossy());
        let escaped_save_path = Self::shell_escape(&save_path.to_string_lossy());
        let escaped_hash = Self::shell_escape(hash);

        // Replace placeholders with escaped values
        let command = command_template
            .replace("%N", &escaped_name)
            .replace("%F", &escaped_full_path)
            .replace("%R", &escaped_save_path)
            .replace("%D", &escaped_save_path)
            .replace("%I", &escaped_hash);

        tracing::info!("Running completion program: {}", command);

        // Execute command
        #[cfg(unix)]
        let result = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .spawn();

        #[cfg(windows)]
        let result = tokio::process::Command::new("cmd")
            .arg("/C")
            .arg(&command)
            .spawn();

        match result {
            Ok(mut child) => {
                // Don't wait for completion, just spawn it
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
                Ok(())
            }
            Err(e) => Err(format!("Failed to run command: {}", e)),
        }
    }

    /// Renames `src` to `dst`, falling back to copy-and-delete for
    /// cross-filesystem moves.
    async fn move_path(src: PathBuf, dst: PathBuf, dst_dir: &Path) -> Result<(), EngineError> {
        if src == dst {
            return Ok(());
        }
        tokio::fs::create_dir_all(dst_dir).await?;

        match tokio::fs::rename(&src, &dst).await {
            Ok(()) => Ok(()),
            Err(rename_err) => {
                tracing::debug!(
                    "rename {:?} -> {:?} failed ({}), falling back to copy",
                    src,
                    dst,
                    rename_err
                );
                let src_clone = src.clone();
                let dst_clone = dst.clone();
                tokio::task::spawn_blocking(move || copy_recursively(&src_clone, &dst_clone))
                    .await
                    .map_err(|e| {
                        EngineError::Io(std::io::Error::other(format!("copy task panicked: {e}")))
                    })??;
                if src.is_dir() {
                    tokio::fs::remove_dir_all(&src).await?;
                } else {
                    tokio::fs::remove_file(&src).await?;
                }
                Ok(())
            }
        }
    }

    /// Announces to trackers and the DHT, performs the initial state
    /// transition, and spawns the torrent's run loop (unless queued).
    async fn start_torrent(self: &Arc<Self>, hash: &str) -> Result<(), EngineError> {
        tracing::info!("start_torrent called for {}", hash);

        let torrents = &self.torrents;
        let tracker_client = &self.tracker_client;
        let dht = &self.dht;
        let peer_id = self.peer_id;
        let listen_port = &self.listen_port;
        let max_active_downloads = &self.max_active_downloads;
        let max_active_uploads = &self.max_active_uploads;

        let (trackers, info_hash_bytes, total_length, is_complete) = {
            let torrents_guard = torrents.read();
            let torrent = torrents_guard
                .get(hash)
                .ok_or_else(|| EngineError::NotFound(hash.to_string()))?;
            (
                torrent.trackers.clone(),
                torrent.info_hash_bytes(),
                torrent.meta.info.total_length,
                torrent.piece_manager.is_complete(),
            )
        };

        tracing::info!(
            "Torrent {} has {} trackers, info_hash_bytes={}, total_length={}, is_complete={}",
            hash,
            trackers.len(),
            info_hash_bytes.is_some(),
            total_length,
            is_complete
        );

        let info_hash = match info_hash_bytes {
            Some(h) => h,
            None => {
                // add_torrent rejects v2-only torrents, so every mapped
                // torrent has a v1 hash; this is defensive only.
                tracing::warn!("Torrent {} has no v1 info hash; cannot start", hash);
                return Ok(());
            }
        };

        let mut found_peers = false;

        tracing::info!(
            "Announcing to {} trackers for torrent {}",
            trackers.len(),
            hash
        );
        for (idx, url) in trackers.iter().enumerate() {
            tracing::info!("Trying tracker: {}", url);

            {
                let mut torrents_guard = torrents.write();
                if let Some(torrent) = torrents_guard.get_mut(hash) {
                    if let Some(info) = torrent.tracker_info.get_mut(idx) {
                        info.status = TrackerState::Updating;
                    }
                }
            }

            let v1_hash = match oxidebt_torrent::InfoHashV1::from_bytes(&info_hash) {
                Ok(h) => h,
                Err(_) => continue,
            };
            let params = AnnounceParams {
                url,
                info_hash: &v1_hash,
                peer_id: peer_id.as_bytes(),
                port: listen_port.load(Ordering::Relaxed),
                uploaded: 0,
                downloaded: 0,
                left: total_length,
                event: TrackerEvent::Started,
            };

            match tracker_client.as_ref().announce(params).await {
                Ok(response) => {
                    let peer_count = response.peers.len() + response.peers6.len();
                    tracing::info!(
                        "Tracker {} returned {} peers (seeders: {:?}, leechers: {:?})",
                        url,
                        peer_count,
                        response.complete,
                        response.incomplete
                    );

                    let mut torrents_guard = torrents.write();
                    if let Some(torrent) = torrents_guard.get_mut(hash) {
                        if let Some(info) = torrent.tracker_info.get_mut(idx) {
                            info.status = TrackerState::Working;
                            info.peers = peer_count as u32;
                            info.seeds = response.complete.unwrap_or(0);
                            info.leechers = response.incomplete.unwrap_or(0);
                            info.last_announce = Some(Instant::now());
                            info.next_announce = Some(
                                Instant::now() + Duration::from_secs(response.interval as u64),
                            );
                            info.message = response.warning_message.clone();
                        }

                        for peer in response.peers.iter().chain(response.peers6.iter()) {
                            torrent.known_peers.insert(peer.addr);
                        }
                        if peer_count > 0 {
                            found_peers = true;
                        }
                        torrent.last_announce = Some(Instant::now());
                    }
                }
                Err(e) => {
                    tracing::warn!("Tracker {} failed: {}", url, e);
                    let mut torrents_guard = torrents.write();
                    if let Some(torrent) = torrents_guard.get_mut(hash) {
                        if let Some(info) = torrent.tracker_info.get_mut(idx) {
                            info.status = TrackerState::Error;
                            info.message = Some(e.to_string());
                            info.last_announce = Some(Instant::now());
                        }
                    }
                }
            }
        }

        if let Some(ref dht) = dht {
            match dht.get_peers(info_hash).await {
                Ok(peers) => {
                    let mut torrents_guard = torrents.write();
                    if let Some(torrent) = torrents_guard.get_mut(hash) {
                        for peer in peers {
                            torrent.known_peers.insert(peer);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("DHT get_peers failed for {}: {}", hash, e);
                }
            }
        }
        let _ = found_peers;

        {
            let mut torrents_guard = torrents.write();
            // Check queue limits before transitioning
            let max_dl = max_active_downloads.load(Ordering::Relaxed);
            let max_ul = max_active_uploads.load(Ordering::Relaxed);

            // First, check if we need to transition and get the info needed
            let transition_info = if let Some(torrent) = torrents_guard.get(hash) {
                let known_peers_count = torrent.known_peers.len();
                let verification_complete = torrent.piece_manager.is_verification_complete();
                let current_is_complete = torrent.piece_manager.is_complete();
                tracing::info!(
                    "Torrent {} state transition: current={:?}, is_complete={}, known_peers={}, verification_complete={}",
                    hash,
                    torrent.state,
                    current_is_complete,
                    known_peers_count,
                    verification_complete
                );
                if torrent.state == TorrentState::Checking && verification_complete {
                    let is_complete = current_is_complete;
                    let added_at = torrent.added_at;
                    Some((is_complete, added_at, known_peers_count))
                } else {
                    if torrent.state == TorrentState::Checking {
                        tracing::info!(
                            "Torrent {} still checking, deferring state transition",
                            hash
                        );
                    }
                    None
                }
            } else {
                None
            };

            // If we need to transition, calculate queue position and update state
            if let Some((is_complete, added_at, known_peers_count)) = transition_info {
                let queue_position = if is_complete {
                    Self::queue_position_upload_from_map(&torrents_guard, added_at)
                } else {
                    Self::queue_position_download_from_map(&torrents_guard, added_at)
                };

                let should_queue = if is_complete {
                    max_ul > 0 && queue_position >= max_ul
                } else {
                    max_dl > 0 && queue_position >= max_dl
                };

                if let Some(torrent) = torrents_guard.get_mut(hash) {
                    if should_queue {
                        torrent.state = TorrentState::Queued;
                        tracing::info!(
                            "Torrent {} queued (position {} >= limit {})",
                            hash,
                            queue_position,
                            if is_complete { max_ul } else { max_dl }
                        );
                    } else if is_complete {
                        torrent.state = TorrentState::Completed;
                        if torrent.seeding_started_at.is_none() {
                            torrent.seeding_started_at = Some(Instant::now());
                        }
                        tracing::info!(
                            "Torrent {} is complete, transitioning to Completed (position {})",
                            hash,
                            queue_position
                        );
                    } else {
                        torrent.state = TorrentState::Downloading;
                        tracing::info!(
                            "Torrent {} transitioning to Downloading (position {}, found {} known peers)",
                            hash,
                            queue_position,
                            known_peers_count
                        );
                    }
                }
            }
        }

        // Only spawn run_torrent if not queued
        let is_queued = {
            let torrents_guard = torrents.read();
            torrents_guard
                .get(hash)
                .map(|t| t.state == TorrentState::Queued)
                .unwrap_or(false)
        };

        if !is_queued {
            // Exactly one run loop per torrent: the loop survives pause and
            // resume (idling while parked), so a resume or queue-dequeue
            // must not stack a second one on top of it.
            let should_spawn = {
                let mut torrents_guard = torrents.write();
                match torrents_guard.get_mut(hash) {
                    Some(torrent) if !torrent.run_loop_active => {
                        torrent.run_loop_active = true;
                        true
                    }
                    _ => false,
                }
            };

            if should_spawn {
                let shared = self.clone();
                let hash = hash.to_string();
                tokio::spawn(async move {
                    shared.run_torrent(hash).await;
                });
            }
        }

        Ok(())
    }

    /// Per-torrent control loop: dials peers, runs choke rounds, performs
    /// state transitions, and reannounces opportunistically until the
    /// torrent is removed.
    async fn run_torrent(self: Arc<Self>, hash: String) {
        tracing::info!("run_torrent started for {}", hash);

        let torrents = &self.torrents;
        let disk_manager = &self.disk_manager;
        let bandwidth_limiter = &self.bandwidth_limiter;
        let peer_id = self.peer_id;
        let event_tx = &self.event_tx;
        let tracker_client = &self.tracker_client;
        let listen_port = &self.listen_port;
        let global_connections = &self.global_connections;
        let no_seed_mode = &self.no_seed_mode;
        let disconnect_on_complete = &self.disconnect_on_complete;
        loop {
            let result = {
                let torrents = torrents.read();
                let Some(torrent) = torrents.get(&hash) else {
                    return;
                };
                match torrent.state {
                    TorrentState::Paused
                    | TorrentState::Queued
                    | TorrentState::Error
                    | TorrentState::Moving => None,
                    TorrentState::MetadataDownloading | TorrentState::Checking => Some((
                        torrent.state,
                        torrent.meta.piece_count(),
                        None,
                        HashSet::new(),
                        0,
                        0,
                    )),
                    TorrentState::Downloading | TorrentState::Seeding | TorrentState::Completed => {
                        // Skip peer connections for Completed torrents when both no_seed_mode
                        // and disconnect_on_complete are enabled
                        let skip_connections = torrent.state == TorrentState::Completed
                            && no_seed_mode.load(Ordering::Acquire)
                            && disconnect_on_complete.load(Ordering::Acquire);

                        let mut peers_to_connect: HashSet<_> = if skip_connections {
                            HashSet::new()
                        } else {
                            torrent
                                .known_peers
                                .iter()
                                .filter(|p| {
                                    !torrent.peers.contains_key(p)
                                        && !torrent.connecting_peers.contains(p)
                                })
                                .cloned()
                                .collect()
                        };

                        if !skip_connections {
                            for (addr, failed) in &torrent.failed_peers {
                                if failed.is_ready_for_retry()
                                    && !torrent.peers.contains_key(addr)
                                    && !torrent.connecting_peers.contains(addr)
                                {
                                    peers_to_connect.insert(*addr);
                                }
                            }
                        }

                        Some((
                            torrent.state,
                            torrent.meta.piece_count(),
                            torrent.info_hash_bytes(),
                            peers_to_connect,
                            torrent.peers.len(),
                            torrent.connecting_peers.len(),
                        ))
                    }
                }
            };

            let Some((
                state,
                piece_count,
                info_hash_bytes,
                peers_to_connect,
                connected_peers,
                connecting_peers,
            )) = result
            else {
                tokio::time::sleep(PAUSED_SLEEP_INTERVAL).await;
                continue;
            };

            if state == TorrentState::Checking || state == TorrentState::MetadataDownloading {
                tokio::time::sleep(CHECKING_SLEEP_INTERVAL).await;
                continue;
            }

            let Some(info_hash) = info_hash_bytes else {
                return;
            };

            // Get available connection slots from rate limiter and check global limit
            let current_global = global_connections.load(Ordering::Relaxed);

            // Detailed peer state logging for diagnostics
            let peer_states = {
                let torrents = torrents.read();
                if let Some(torrent) = torrents.get(&hash) {
                    let unchoked_count =
                        torrent.peers.values().filter(|p| !p.is_choking_us).count();
                    let interested_count =
                        torrent.peers.values().filter(|p| p.is_interested).count();
                    let with_bitfield = torrent
                        .peers
                        .values()
                        .filter(|p| p.bitfield.is_some())
                        .count();
                    let seeds = torrent
                        .peers
                        .values()
                        .filter(|p| {
                            p.bitfield
                                .as_ref()
                                .map(|bf| bf.is_complete())
                                .unwrap_or(false)
                        })
                        .count();
                    let active_pieces = torrent.piece_manager.active_piece_count();
                    let have_count = torrent.piece_manager.have_count();
                    let piece_count = torrent.meta.piece_count();
                    let download_rate = torrent.stats.download_rate;
                    Some((
                        unchoked_count,
                        interested_count,
                        with_bitfield,
                        seeds,
                        active_pieces,
                        have_count,
                        piece_count,
                        download_rate,
                    ))
                } else {
                    None
                }
            };

            if let Some((unchoked, interested, with_bf, seeds, active, have, total, dl_rate)) =
                peer_states
            {
                tracing::info!(
                    "[DIAG] {} | peers: conn={} half={} queue={} | state: unchoked={} interested_in_us={} seeds={} bf={} | pieces: {}/{} active={} | speed: {:.1} KB/s | global={}",
                    &hash[..8],
                    connected_peers,
                    connecting_peers,
                    peers_to_connect.len(),
                    unchoked,
                    interested,
                    seeds,
                    with_bf,
                    have,
                    total,
                    active,
                    dl_rate / 1024.0,
                    current_global
                );
            }
            let global_slots = MAX_GLOBAL_CONNECTIONS.saturating_sub(current_global);

            let available_slots = {
                let mut torrents = torrents.write();
                if let Some(torrent) = torrents.get_mut(&hash) {
                    torrent
                        .connection_limiter
                        .available_slots()
                        .min(global_slots)
                } else {
                    0
                }
            };

            // Check both limits separately:
            // - connected_peers < MAX_PEERS_PER_TORRENT (actual peer limit)
            // - connecting_peers < MAX_HALF_OPEN (half-open connection limit)
            let need_more_peers = connected_peers < MAX_PEERS_PER_TORRENT;
            let can_open_more = connecting_peers < MAX_HALF_OPEN;

            if need_more_peers
                && can_open_more
                && !peers_to_connect.is_empty()
                && available_slots > 0
            {
                // Limit by both how many peers we need and how many half-open slots we have
                let peer_slots = MAX_PEERS_PER_TORRENT.saturating_sub(connected_peers);
                let half_open_slots = MAX_HALF_OPEN.saturating_sub(connecting_peers);
                let to_connect = peer_slots.min(half_open_slots).min(available_slots);
                let peers: Vec<_> = peers_to_connect.into_iter().take(to_connect).collect();
                tracing::info!(
                    "Connecting to {} new peers for torrent {}",
                    peers.len(),
                    hash
                );

                {
                    let mut torrents = torrents.write();
                    if let Some(torrent) = torrents.get_mut(&hash) {
                        // Record connection attempts for rate limiting
                        torrent.connection_limiter.record_attempts(peers.len());
                        for peer in &peers {
                            torrent.known_peers.remove(peer);
                            torrent.connecting_peers.insert(*peer);
                        }
                    }
                }

                for peer_addr in peers {
                    let hash_clone = hash.clone();
                    let torrents_clone = torrents.clone();
                    let disk_manager_clone = disk_manager.clone();
                    let bandwidth_limiter_clone = bandwidth_limiter.clone();
                    let event_tx_clone = event_tx.clone();
                    let port = listen_port.load(Ordering::Relaxed);
                    let global_conns = global_connections.clone();
                    let no_seed = no_seed_mode.clone();

                    // Increment global connection counter
                    global_conns.fetch_add(1, Ordering::Relaxed);

                    tokio::spawn(async move {
                        tracing::debug!("Attempting to connect to peer {}", peer_addr);
                        match PeerConnection::connect(peer_addr, info_hash, peer_id, piece_count)
                            .await
                        {
                            Ok(conn) => {
                                tracing::debug!("TCP connection established with {}", peer_addr);
                                match peer_connection::handle_peer_connection(
                                    hash_clone.clone(),
                                    peer_addr,
                                    conn,
                                    torrents_clone.clone(),
                                    disk_manager_clone,
                                    bandwidth_limiter_clone,
                                    event_tx_clone.clone(),
                                    port,
                                    no_seed,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        tracing::debug!(
                                            "Peer {} connection ended normally",
                                            peer_addr
                                        );
                                    }
                                    Err(e) => {
                                        tracing::debug!(
                                            "Peer {} connection error: {}",
                                            peer_addr,
                                            e
                                        );
                                    }
                                }
                                // Decrement global connection counter when connection ends
                                global_conns.fetch_sub(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                // Decrement global connection counter on connection failure
                                global_conns.fetch_sub(1, Ordering::Relaxed);
                                // Categorize the error for diagnostics
                                let error_type = if e.to_string().contains("timed out")
                                    || e.to_string().contains("Timeout")
                                {
                                    "TIMEOUT"
                                } else if e.to_string().contains("refused") {
                                    "REFUSED"
                                } else if e.to_string().contains("reset") {
                                    "RESET"
                                } else if e.to_string().contains("unreachable") {
                                    "UNREACHABLE"
                                } else {
                                    "OTHER"
                                };
                                tracing::debug!(
                                    "[CONN-FAIL] {} -> {} | {}: {}",
                                    peer_addr,
                                    error_type,
                                    error_type,
                                    e
                                );
                                let mut torrents = torrents_clone.write();
                                if let Some(torrent) = torrents.get_mut(&hash_clone) {
                                    torrent.connecting_peers.remove(&peer_addr);

                                    if let Some(failed) = torrent.failed_peers.get_mut(&peer_addr) {
                                        if failed.should_give_up() {
                                            torrent.failed_peers.remove(&peer_addr);
                                            tracing::debug!(
                                                "Giving up on peer {} after {} attempts",
                                                peer_addr,
                                                MAX_PEER_RETRY_ATTEMPTS
                                            );
                                        } else {
                                            failed.increment_attempt();
                                            tracing::debug!(
                                                "Will retry peer {} (attempt {})",
                                                peer_addr,
                                                failed.attempts
                                            );
                                        }
                                    } else {
                                        torrent.failed_peers.insert(peer_addr, FailedPeer::new());
                                        tracing::debug!("Added peer {} to retry queue", peer_addr);
                                    }
                                }
                            }
                        }
                    });
                }
            }

            {
                let mut torrents = torrents.write();
                if let Some(torrent) = torrents.get_mut(&hash) {
                    torrent.stats.update_rates();

                    if torrent.piece_manager.is_complete()
                        && torrent.state == TorrentState::Downloading
                    {
                        torrent.state = TorrentState::Completed;
                        // Start tracking seeding time
                        if torrent.seeding_started_at.is_none() {
                            torrent.seeding_started_at = Some(Instant::now());
                        }
                        tracing::info!(
                            "Torrent {} download complete, transitioning to Completed",
                            hash
                        );

                        if !torrent.completion_processed {
                            torrent.completion_processed = true;
                            let shared = self.clone();
                            let hash = hash.clone();
                            tokio::spawn(async move {
                                shared.handle_torrent_completed(hash).await;
                            });
                        }
                    }

                    // Handle Seeding <-> Completed transitions based on peer interest
                    if torrent.piece_manager.is_complete() {
                        let peers_interested = torrent.peers.values().any(|p| p.is_interested);

                        if torrent.state == TorrentState::Seeding && !peers_interested {
                            torrent.state = TorrentState::Completed;
                            tracing::info!(
                                "Torrent {} has no interested peers, transitioning to Completed",
                                hash
                            );
                        } else if torrent.state == TorrentState::Completed
                            && peers_interested
                            && !no_seed_mode.load(Ordering::Acquire)
                        {
                            torrent.state = TorrentState::Seeding;
                            // Start tracking seeding time if not already started
                            if torrent.seeding_started_at.is_none() {
                                torrent.seeding_started_at = Some(Instant::now());
                            }
                            tracing::info!(
                                "Torrent {} has interested peers, transitioning to Seeding",
                                hash
                            );
                        }
                    }

                    if torrent.last_choke_round.elapsed() >= CHOKING_INTERVAL {
                        let rotate_optimistic = torrent.last_optimistic_unchoke.elapsed()
                            >= OPTIMISTIC_UNCHOKE_INTERVAL;
                        Self::run_choking_algorithm(torrent, rotate_optimistic);
                        torrent.last_choke_round = Instant::now();
                        if rotate_optimistic {
                            torrent.last_optimistic_unchoke = Instant::now();
                        }
                    }

                    let stale_pieces = torrent.piece_manager.cleanup_stale_pieces();
                    if !stale_pieces.is_empty() {
                        tracing::debug!(
                            "Cleaned up {} stale pieces for torrent {}: {:?}",
                            stale_pieces.len(),
                            hash,
                            stale_pieces
                        );
                    }
                }
            }

            let reannounce_info = {
                let torrents_guard = torrents.read();
                if let Some(torrent) = torrents_guard.get(&hash) {
                    // Re-announce more aggressively when we have few peers
                    let min_interval = if torrent.peers.len() < PEER_THRESHOLD_CRITICAL {
                        TRACKER_MIN_INTERVAL // 60s when desperate
                    } else if torrent.peers.len() < PEER_THRESHOLD_LOW {
                        TRACKER_AGGRESSIVE_INTERVAL // 5min when low
                    } else {
                        TRACKER_MODERATE_INTERVAL // 15min normally
                    };

                    let should_reannounce = torrent.peers.len() < PEER_THRESHOLD_LOW
                        && torrent
                            .last_announce
                            .map(|t| t.elapsed() > min_interval)
                            .unwrap_or(true)
                        && state != TorrentState::Checking;

                    if should_reannounce {
                        // Report "left" from verified pieces, consistent with
                        // the periodic reannounce task; raw downloaded bytes
                        // overcount (duplicates, failed pieces).
                        let verified_bytes = torrent.piece_manager.have_count() as u64
                            * torrent.meta.info.piece_length;
                        let left = if torrent.piece_manager.is_complete() {
                            0
                        } else {
                            torrent
                                .meta
                                .info
                                .total_length
                                .saturating_sub(verified_bytes)
                        };
                        Some((
                            torrent.trackers.clone(),
                            left,
                            torrent.stats.downloaded,
                            torrent.stats.uploaded,
                            torrent.info_hash_bytes(),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some((trackers, left, downloaded, uploaded, Some(info_hash))) = reannounce_info {
                {
                    let mut torrents_guard = torrents.write();
                    if let Some(torrent) = torrents_guard.get_mut(&hash) {
                        torrent.last_announce = Some(Instant::now());
                    }
                }

                let hash_clone = hash.clone();
                let torrents_clone = torrents.clone();
                let tracker_client_clone = tracker_client.clone();
                let listen_port_clone = listen_port.clone();

                tokio::spawn(async move {
                    for url in trackers {
                        let v1_hash = match oxidebt_torrent::InfoHashV1::from_bytes(&info_hash) {
                            Ok(h) => h,
                            Err(_) => continue,
                        };

                        let params = AnnounceParams {
                            url: &url,
                            info_hash: &v1_hash,
                            peer_id: peer_id.as_bytes(),
                            port: listen_port_clone.load(Ordering::Relaxed),
                            uploaded,
                            downloaded,
                            left,
                            event: TrackerEvent::None,
                        };

                        if let Ok(response) = tracker_client_clone.announce(params).await {
                            let peer_count = response.peers.len() + response.peers6.len();
                            if peer_count > 0 {
                                tracing::debug!("Re-announce to {} got {} peers", url, peer_count);
                                let mut torrents = torrents_clone.write();
                                if let Some(torrent) = torrents.get_mut(&hash_clone) {
                                    for peer in response.peers.iter().chain(response.peers6.iter())
                                    {
                                        torrent.known_peers.insert(peer.addr);
                                    }
                                }
                                break;
                            }
                        }
                    }
                });
            }

            let sleep_duration = if connected_peers < PEER_THRESHOLD_MEDIUM {
                LOOP_INTERVAL_FAST
            } else if connected_peers < PEER_THRESHOLD_MEDIUM * 2 {
                LOOP_INTERVAL_NORMAL
            } else {
                LOOP_INTERVAL_STABLE
            };
            tokio::time::sleep(sleep_duration).await;
        }
    }

    /// Tit-for-tat choke round. Ranks interested peers by bytes transferred
    /// since the previous round (a rate, not lifetime totals), unchokes the
    /// top MAX_UNCHOKED_PEERS - 1, and keeps one optimistic slot that rotates
    /// on the optimistic interval.
    fn run_choking_algorithm(torrent: &mut ManagedTorrent, rotate_optimistic: bool) {
        // When complete (Seeding or Completed) reciprocate upload demand;
        // while leeching, reward peers that send to us.
        let is_complete = torrent.piece_manager.is_complete();

        let mut interested_peers: Vec<_> = torrent
            .peers
            .iter()
            .filter(|(_, info)| info.is_interested)
            .map(|(addr, info)| {
                let metric = if is_complete {
                    info.upload_bytes - info.choke_round_upload_bytes
                } else {
                    info.download_bytes - info.choke_round_download_bytes
                };
                (*addr, metric)
            })
            .collect();

        interested_peers.sort_by_key(|a| std::cmp::Reverse(a.1));

        let mut new_unchoked = HashSet::new();
        for (addr, _) in interested_peers.iter().take(MAX_UNCHOKED_PEERS - 1) {
            new_unchoked.insert(*addr);
        }

        let remaining: Vec<_> = interested_peers
            .iter()
            .skip(MAX_UNCHOKED_PEERS - 1)
            .map(|(addr, _)| *addr)
            .collect();

        // The optimistic peer keeps its slot across regular rounds; replace it
        // when the rotation interval elapses or it is gone/promoted/uninterested.
        let optimistic_still_valid = torrent
            .optimistic_unchoke_peer
            .map(|p| remaining.contains(&p))
            .unwrap_or(false);

        if rotate_optimistic || !optimistic_still_valid {
            torrent.optimistic_unchoke_peer = if remaining.is_empty() {
                None
            } else {
                use std::collections::hash_map::DefaultHasher;

                let mut hasher = DefaultHasher::new();
                Instant::now().hash(&mut hasher);
                let idx = hasher.finish() as usize % remaining.len();
                Some(remaining[idx])
            };
        }

        if let Some(optimistic) = torrent.optimistic_unchoke_peer {
            new_unchoked.insert(optimistic);
        }

        torrent.unchoked_peers = new_unchoked;

        // Start the next measurement window.
        for info in torrent.peers.values_mut() {
            info.choke_round_download_bytes = info.download_bytes;
            info.choke_round_upload_bytes = info.upload_bytes;
        }
    }
}

#[allow(dead_code)]
impl TorrentEngine {
    /// Returns the status of all torrents, including synthetic entries for
    /// magnets still fetching metadata.
    pub async fn get_all_status(&self) -> Vec<crate::TorrentStatus> {
        let mut statuses: Vec<crate::TorrentStatus> = self
            .torrents
            .read()
            .values()
            .map(|t| t.to_status())
            .collect();

        for (hash, pending) in self.pending_magnets.read().iter() {
            statuses.push(crate::TorrentStatus {
                info_hash: hash.clone(),
                name: pending.name.clone(),
                state: if pending.error.is_some() {
                    "error".to_string()
                } else {
                    "metaDL".to_string()
                },
                progress: 0.0,
                download_rate: 0.0,
                upload_rate: 0.0,
                downloaded: 0,
                uploaded: 0,
                total_size: 0,
                peers: 0,
                seeds: 0,
            });
        }

        statuses
    }

    /// Returns tracker information for a torrent.
    pub fn get_tracker_info(&self, hash: &str) -> Option<Vec<TrackerStatusInfo>> {
        let torrents = self.torrents.read();
        let torrent = torrents.get(hash)?;
        Some(
            torrent
                .tracker_info
                .iter()
                .map(|t| TrackerStatusInfo {
                    url: t.url.clone(),
                    status: t.status.as_str().to_string(),
                    peers: t.peers,
                    seeds: t.seeds,
                    leechers: t.leechers,
                    last_announce: t.last_announce.map(|i| i.elapsed().as_secs()),
                    next_announce: t.next_announce.map(|i| {
                        if i > Instant::now() {
                            (i - Instant::now()).as_secs()
                        } else {
                            0
                        }
                    }),
                    message: t.message.clone(),
                })
                .collect(),
        )
    }

    /// Returns file information for a torrent including per-file progress.
    pub fn get_torrent_files(&self, hash: &str) -> Option<Vec<crate::TorrentFileInfo>> {
        let torrents = self.torrents.read();
        let torrent = torrents.get(hash)?;

        let piece_length = torrent.meta.info.piece_length;
        let bitfield = torrent.piece_manager.bitfield();

        let files: Vec<crate::TorrentFileInfo> = torrent
            .meta
            .info
            .files
            .iter()
            .scan(0u64, |file_offset, file| {
                let start_offset = *file_offset;
                let end_offset = start_offset + file.length;
                *file_offset = end_offset;

                // Calculate which pieces this file spans
                let first_piece = (start_offset / piece_length) as usize;
                let last_piece = if file.length == 0 {
                    first_piece
                } else {
                    ((end_offset - 1) / piece_length) as usize
                };

                // Calculate downloaded bytes for this file
                let mut downloaded: u64 = 0;

                for piece_idx in first_piece..=last_piece {
                    if bitfield.has_piece(piece_idx) {
                        // Calculate how much of this piece belongs to this file
                        let piece_start = (piece_idx as u64) * piece_length;
                        let piece_end = piece_start + torrent.piece_size(piece_idx as u32);

                        // Overlap between [piece_start, piece_end) and [start_offset, end_offset)
                        let overlap_start = piece_start.max(start_offset);
                        let overlap_end = piece_end.min(end_offset);

                        if overlap_end > overlap_start {
                            downloaded += overlap_end - overlap_start;
                        }
                    }
                }

                let progress = if file.length > 0 {
                    (downloaded as f64 / file.length as f64) * 100.0
                } else {
                    100.0
                };

                Some(crate::TorrentFileInfo {
                    path: file.path.display().to_string(),
                    size: file.length,
                    progress,
                    downloaded,
                })
            })
            .collect();

        Some(files)
    }

    pub async fn pause(&self, hash: &str) -> Result<(), EngineError> {
        self.shared.pause_torrent(hash).await
    }

    pub async fn resume(&self, hash: &str) -> Result<(), EngineError> {
        self.shared.resume_torrent(hash).await
    }

    /// Removes a torrent, optionally deleting files.
    pub async fn remove(&self, hash: &str, delete_files: bool) -> Result<(), EngineError> {
        self.shared.remove_torrent(hash, delete_files).await
    }
}

impl EngineShared {
    pub async fn pause_torrent(self: &Arc<Self>, hash: &str) -> Result<(), EngineError> {
        {
            let mut torrents = self.torrents.write();
            let torrent = torrents
                .get_mut(hash)
                .ok_or_else(|| EngineError::NotFound(hash.to_string()))?;

            torrent.state = TorrentState::Paused;

            let _ = torrent.shutdown_tx.send(());

            torrent.peers.clear();
            torrent.connecting_peers.clear();
            torrent.unchoked_peers.clear();
            torrent.stats.download_rate = 0.0;
            torrent.stats.upload_rate = 0.0;

            let (new_shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
            torrent.shutdown_tx = new_shutdown_tx;

            tracing::info!("Paused torrent {}", hash);
        }

        self.persist_session();
        self.start_next_queued().await;

        Ok(())
    }

    pub async fn resume_torrent(self: &Arc<Self>, hash: &str) -> Result<(), EngineError> {
        let is_complete = {
            let torrents = self.torrents.read();
            let torrent = torrents
                .get(hash)
                .ok_or_else(|| EngineError::NotFound(hash.to_string()))?;
            torrent.piece_manager.is_complete()
        };

        let max_dl = self.max_active_downloads.load(Ordering::Relaxed);
        let max_ul = self.max_active_uploads.load(Ordering::Relaxed);
        let active_dl = self.count_active_downloads();
        let active_ul = self.count_active_uploads();

        let should_queue = if is_complete {
            max_ul > 0 && active_ul >= max_ul
        } else {
            max_dl > 0 && active_dl >= max_dl
        };

        {
            let mut torrents = self.torrents.write();
            let torrent = torrents
                .get_mut(hash)
                .ok_or_else(|| EngineError::NotFound(hash.to_string()))?;

            if should_queue {
                torrent.state = TorrentState::Queued;
                tracing::info!("Torrent {} queued (limits reached)", &hash[..8]);
                return Ok(());
            }

            if is_complete {
                torrent.state = TorrentState::Completed;
                if torrent.seeding_started_at.is_none() {
                    torrent.seeding_started_at = Some(Instant::now());
                }
            } else {
                torrent.state = TorrentState::Downloading;
            }
        }

        self.persist_session();
        self.start_torrent(hash).await
    }

    pub async fn remove_torrent(
        self: &Arc<Self>,
        hash: &str,
        delete_files: bool,
    ) -> Result<(), EngineError> {
        // A magnet still fetching metadata has no torrent or files yet; the
        // in-flight fetch task notices the missing entry and aborts.
        if self.pending_magnets.write().remove(hash).is_some() {
            return Ok(());
        }

        let (meta, save_path) = {
            let mut torrents = self.torrents.write();
            let torrent = torrents
                .remove(hash)
                .ok_or_else(|| EngineError::NotFound(hash.to_string()))?;
            (torrent.meta, torrent.save_path)
        };

        self.disk_manager.unregister(hash);

        if delete_files {
            // Content lives at save_path/name: a file for single-file
            // torrents, a directory otherwise.
            let path = save_path.join(&meta.info.name);
            if path.exists() {
                if path.is_dir() {
                    tokio::fs::remove_dir_all(&path).await?;
                } else {
                    tokio::fs::remove_file(&path).await?;
                }
            }
        }

        persistence::delete_torrent_blob(hash).await;
        self.persist_session();
        self.start_next_queued().await;

        Ok(())
    }

    fn count_active_downloads(&self) -> usize {
        let torrents = self.torrents.read();
        torrents
            .values()
            .filter(|t| {
                matches!(t.state, TorrentState::Downloading | TorrentState::Checking)
                    && !t.piece_manager.is_complete()
            })
            .count()
    }

    fn count_active_uploads(&self) -> usize {
        let torrents = self.torrents.read();
        torrents
            .values()
            .filter(|t| {
                matches!(
                    t.state,
                    TorrentState::Seeding | TorrentState::Completed | TorrentState::Checking
                ) && t.piece_manager.is_complete()
            })
            .count()
    }

    /// Determines the queue position for downloads (0-indexed).
    /// Position is based on how many other incomplete torrents (that are active or will become active)
    /// were added before this one.
    fn queue_position_download_from_map(
        torrents: &HashMap<String, ManagedTorrent>,
        added_at: std::time::Instant,
    ) -> usize {
        torrents
            .values()
            .filter(|t| {
                // Count torrents that:
                // 1. Are incomplete (downloads)
                // 2. Were added before this one
                // 3. Are active (Downloading/Checking) or waiting (Queued)
                !t.piece_manager.is_complete()
                    && t.added_at < added_at
                    && matches!(
                        t.state,
                        TorrentState::Downloading | TorrentState::Checking | TorrentState::Queued
                    )
            })
            .count()
    }

    /// Determines the queue position for uploads (0-indexed).
    /// Position is based on how many other complete torrents (that are active or will become active)
    /// were added before this one.
    fn queue_position_upload_from_map(
        torrents: &HashMap<String, ManagedTorrent>,
        added_at: std::time::Instant,
    ) -> usize {
        torrents
            .values()
            .filter(|t| {
                // Count torrents that:
                // 1. Are complete (uploads/seeds)
                // 2. Were added before this one
                // 3. Are active (Seeding/Completed/Checking) or waiting (Queued)
                t.piece_manager.is_complete()
                    && t.added_at < added_at
                    && matches!(
                        t.state,
                        TorrentState::Seeding
                            | TorrentState::Completed
                            | TorrentState::Checking
                            | TorrentState::Queued
                    )
            })
            .count()
    }

    async fn start_next_queued(self: &Arc<Self>) {
        let max_dl = self.max_active_downloads.load(Ordering::Relaxed);
        let max_ul = self.max_active_uploads.load(Ordering::Relaxed);

        if max_dl == 0 && max_ul == 0 {
            return;
        }

        let active_dl = self.count_active_downloads();
        let active_ul = self.count_active_uploads();

        // Find the oldest queued torrent that fits within limits (FIFO order)
        let queued_hash: Option<String> = {
            let torrents = self.torrents.read();
            torrents
                .iter()
                .filter(|(_, t)| {
                    if t.state != TorrentState::Queued {
                        return false;
                    }
                    let is_complete = t.piece_manager.is_complete();
                    if is_complete {
                        max_ul == 0 || active_ul < max_ul
                    } else {
                        max_dl == 0 || active_dl < max_dl
                    }
                })
                .min_by_key(|(_, t)| t.added_at)
                .map(|(hash, _)| hash.clone())
        };

        if let Some(hash) = queued_hash {
            let is_complete = {
                let mut torrents = self.torrents.write();
                if let Some(torrent) = torrents.get_mut(&hash) {
                    let complete = torrent.piece_manager.is_complete();
                    if complete {
                        torrent.state = TorrentState::Completed;
                    } else {
                        torrent.state = TorrentState::Downloading;
                    }
                    complete
                } else {
                    return;
                }
            };

            tracing::info!(
                "Starting queued torrent {} (complete={})",
                &hash[..8],
                is_complete
            );
            let _ = self.start_torrent(&hash).await;
        }
    }
}

#[allow(dead_code)]
impl TorrentEngine {
    pub fn set_queue_settings(&self, max_downloads: usize, max_uploads: usize) {
        self.max_active_downloads
            .store(max_downloads, Ordering::Relaxed);
        self.max_active_uploads
            .store(max_uploads, Ordering::Relaxed);
        self.persist_settings();
    }

    pub fn get_queue_settings(&self) -> (usize, usize) {
        (
            self.max_active_downloads.load(Ordering::Relaxed),
            self.max_active_uploads.load(Ordering::Relaxed),
        )
    }

    /// Returns peer details for a torrent.
    pub fn get_torrent_peers(&self, hash: &str) -> Option<Vec<crate::PeerStatusInfo>> {
        let torrents = self.torrents.read();
        let torrent = torrents.get(hash)?;
        Some(
            torrent
                .peers
                .iter()
                .map(|(addr, info)| crate::PeerStatusInfo {
                    address: addr.to_string(),
                    download_bytes: info.download_bytes,
                    upload_bytes: info.upload_bytes,
                    is_choking_us: info.is_choking_us,
                    is_interested: info.is_interested,
                    progress: info
                        .bitfield
                        .as_ref()
                        .map(|bf| (bf.count() as f64 / torrent.meta.piece_count() as f64) * 100.0)
                        .unwrap_or(0.0),
                })
                .collect(),
        )
    }

    /// Returns global bandwidth statistics.
    pub fn get_global_stats(&self) -> crate::GlobalStats {
        let torrents = self.torrents.read();
        let mut total_download_rate = 0.0;
        let mut total_upload_rate = 0.0;
        let mut total_downloaded = 0u64;
        let mut total_uploaded = 0u64;
        let mut active_torrents = 0usize;
        let mut total_peers = 0usize;

        for torrent in torrents.values() {
            total_download_rate += torrent.stats.download_rate;
            total_upload_rate += torrent.stats.upload_rate;
            total_downloaded += torrent.stats.downloaded;
            total_uploaded += torrent.stats.uploaded;
            total_peers += torrent.peers.len();

            if torrent.state.is_active_for_limits() {
                active_torrents += 1;
            }
        }

        crate::GlobalStats {
            download_rate: total_download_rate,
            upload_rate: total_upload_rate,
            total_downloaded,
            total_uploaded,
            active_torrents,
            total_peers,
            global_connections: self.global_connections.load(Ordering::Relaxed),
        }
    }

    // ========== Sequential Download ==========

    /// Set sequential download mode for a torrent
    pub fn set_sequential_download(&self, hash: &str, enabled: bool) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            torrent.sequential_download = enabled;
            tracing::info!(
                "Sequential download for {}: {}",
                &hash[..8.min(hash.len())],
                enabled
            );
            true
        } else {
            false
        }
    }

    /// Get sequential download mode for a torrent
    pub fn get_sequential_download(&self, hash: &str) -> Option<bool> {
        self.torrents
            .read()
            .get(hash)
            .map(|t| t.sequential_download)
    }

    // ========== File Priorities ==========

    /// Set file priority for a specific file in a torrent
    pub fn set_file_priority(&self, hash: &str, file_index: usize, priority: FilePriority) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            if file_index < torrent.file_priorities.len() {
                torrent.file_priorities[file_index] = priority;
                tracing::info!(
                    "File {} priority for {}: {:?}",
                    file_index,
                    &hash[..8.min(hash.len())],
                    priority
                );
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Set priorities for all files in a torrent
    pub fn set_all_file_priorities(&self, hash: &str, priorities: Vec<u8>) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            if priorities.len() == torrent.file_priorities.len() {
                torrent.file_priorities =
                    priorities.into_iter().map(FilePriority::from_u8).collect();
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get file priorities for a torrent
    pub fn get_file_priorities(&self, hash: &str) -> Option<Vec<u8>> {
        self.torrents
            .read()
            .get(hash)
            .map(|t| t.file_priorities.iter().map(|p| *p as u8).collect())
    }

    // ========== Categories ==========

    /// Add a category
    pub fn add_category(&self, name: String, save_path: PathBuf) {
        let category = settings::Category {
            name: name.clone(),
            save_path,
        };
        self.categories.write().insert(name, category);
        self.persist_settings();
    }

    /// Remove a category
    pub fn remove_category(&self, name: &str) -> bool {
        let removed = self.categories.write().remove(name).is_some();
        if removed {
            self.persist_settings();
        }
        removed
    }

    /// Get all categories
    pub fn get_categories(&self) -> Vec<settings::Category> {
        self.categories.read().values().cloned().collect()
    }

    /// Set torrent category
    pub fn set_torrent_category(&self, hash: &str, category: Option<String>) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            torrent.category = category;
            true
        } else {
            false
        }
    }

    /// Get torrent category
    pub fn get_torrent_category(&self, hash: &str) -> Option<Option<String>> {
        self.torrents.read().get(hash).map(|t| t.category.clone())
    }

    // ========== Tags ==========

    /// Add a tag to a torrent
    pub fn add_torrent_tag(&self, hash: &str, tag: String) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            torrent.tags.insert(tag);
            true
        } else {
            false
        }
    }

    /// Remove a tag from a torrent
    pub fn remove_torrent_tag(&self, hash: &str, tag: &str) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            torrent.tags.remove(tag)
        } else {
            false
        }
    }

    /// Get all tags for a torrent
    pub fn get_torrent_tags(&self, hash: &str) -> Option<Vec<String>> {
        self.torrents
            .read()
            .get(hash)
            .map(|t| t.tags.iter().cloned().collect())
    }

    /// Set all tags for a torrent
    pub fn set_torrent_tags(&self, hash: &str, tags: Vec<String>) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            torrent.tags = tags.into_iter().collect();
            true
        } else {
            false
        }
    }

    // ========== Share Limits ==========

    /// Set share limits for a torrent
    pub fn set_torrent_share_limits(
        &self,
        hash: &str,
        max_ratio: Option<f64>,
        max_seeding_time: Option<u64>,
        limit_action: settings::LimitAction,
    ) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            torrent.share_limits = settings::ShareLimits {
                max_ratio,
                max_seeding_time,
                limit_action,
            };
            true
        } else {
            false
        }
    }

    /// Get share limits for a torrent
    pub fn get_torrent_share_limits(&self, hash: &str) -> Option<settings::ShareLimits> {
        self.torrents
            .read()
            .get(hash)
            .map(|t| t.share_limits.clone())
    }

    /// Set default share limits for new torrents
    pub fn set_default_share_limits(&self, limits: settings::ShareLimits) {
        *self.default_share_limits.write() = limits;
        self.persist_settings();
    }

    /// Get default share limits
    pub fn get_default_share_limits(&self) -> settings::ShareLimits {
        self.default_share_limits.read().clone()
    }

    /// Get current share ratio for a torrent
    pub fn get_torrent_ratio(&self, hash: &str) -> Option<f64> {
        self.torrents.read().get(hash).map(|t| t.share_ratio())
    }

    /// Get seeding time for a torrent in seconds
    pub fn get_torrent_seeding_time(&self, hash: &str) -> Option<u64> {
        self.torrents.read().get(hash).map(|t| t.seeding_time())
    }

    // ========== Auto-Add Trackers ==========

    /// Set auto-add tracker settings
    pub fn set_auto_tracker_settings(&self, enabled: bool, trackers: Vec<String>) {
        {
            let mut settings = self.auto_tracker_settings.write();
            settings.enabled = enabled;
            settings.trackers = trackers;
        }
        self.persist_settings();
    }

    /// Get auto-add tracker settings
    pub fn get_auto_tracker_settings(&self) -> settings::AutoTrackerSettings {
        self.auto_tracker_settings.read().clone()
    }

    // ========== Move on Completion ==========

    /// Set move-on-completion settings
    pub fn set_move_on_complete_settings(
        &self,
        enabled: bool,
        target_path: Option<PathBuf>,
        use_category_path: bool,
    ) {
        {
            let mut settings = self.move_on_complete_settings.write();
            settings.enabled = enabled;
            settings.target_path = target_path;
            settings.use_category_path = use_category_path;
        }
        self.persist_settings();
    }

    /// Get move-on-completion settings
    pub fn get_move_on_complete_settings(&self) -> settings::MoveOnCompleteSettings {
        self.move_on_complete_settings.read().clone()
    }

    /// Set move-on-completion path for a specific torrent
    pub fn set_torrent_move_on_complete(&self, hash: &str, path: Option<PathBuf>) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            torrent.move_on_complete = path;
            true
        } else {
            false
        }
    }

    // ========== External Program ==========

    /// Set external program settings
    pub fn set_external_program_settings(
        &self,
        on_completion_enabled: bool,
        command: Option<String>,
    ) {
        {
            let mut settings = self.external_program_settings.write();
            settings.on_completion_enabled = on_completion_enabled;
            settings.on_completion_command = command;
        }
        self.persist_settings();
    }

    /// Get external program settings
    pub fn get_external_program_settings(&self) -> settings::ExternalProgramSettings {
        self.external_program_settings.read().clone()
    }

    // ========== Watch Folders ==========

    /// Get watch folder manager
    pub fn watch_manager(&self) -> &Arc<watch::WatchFolderManager> {
        &self.watch_manager
    }

    /// Add a watch folder
    pub async fn add_watch_folder(&self, folder: settings::WatchFolder) {
        // watch::WatchFolder is a re-export of settings::WatchFolder.
        self.watch_manager.add_folder(folder).await;
        self.persist_settings();
    }

    /// Remove a watch folder
    pub async fn remove_watch_folder(&self, id: &str) -> bool {
        let removed = self.watch_manager.remove_folder(id).await.is_some();
        if removed {
            self.persist_settings();
        }
        removed
    }

    /// Get all watch folders
    pub async fn get_watch_folders(&self) -> Vec<watch::WatchFolder> {
        self.watch_manager.get_folders().await
    }

    // ========== RSS ==========

    /// Get RSS manager
    pub fn rss_manager(&self) -> &Arc<rss::RssManager> {
        &self.rss_manager
    }

    /// Add an RSS feed
    pub async fn add_rss_feed(&self, feed: rss::RssFeed) {
        self.rss_manager.add_feed(feed).await;
        self.persist_settings();
    }

    /// Remove an RSS feed
    pub async fn remove_rss_feed(&self, id: &str) -> bool {
        let removed = self.rss_manager.remove_feed(id).await.is_some();
        if removed {
            self.persist_settings();
        }
        removed
    }

    /// Get all RSS feeds
    pub async fn get_rss_feeds(&self) -> Vec<rss::RssFeed> {
        self.rss_manager.get_feeds().await
    }

    /// Add an RSS download rule
    pub async fn add_rss_rule(&self, rule: rss::RssDownloadRule) {
        self.rss_manager.add_rule(rule).await;
        self.persist_settings();
    }

    /// Remove an RSS download rule
    pub async fn remove_rss_rule(&self, id: &str) -> bool {
        let removed = self.rss_manager.remove_rule(id).await.is_some();
        if removed {
            self.persist_settings();
        }
        removed
    }

    /// Get all RSS download rules
    pub async fn get_rss_rules(&self) -> Vec<rss::RssDownloadRule> {
        self.rss_manager.get_rules().await
    }

    /// Get items for an RSS feed
    pub async fn get_rss_feed_items(&self, feed_id: &str) -> Vec<rss::RssItem> {
        self.rss_manager.get_feed_items(feed_id).await
    }

    /// Refresh a specific RSS feed
    pub async fn refresh_rss_feed(&self, feed_id: &str) -> Result<(), String> {
        self.rss_manager.refresh_feed(feed_id).await
    }

    // ========== Search Engine ==========

    /// Get search engine
    pub fn search_engine(&self) -> &Arc<search::SearchEngine> {
        &self.search_engine
    }

    /// Load search plugins
    pub async fn load_search_plugins(&self) -> Result<usize, String> {
        self.search_engine.load_plugins().await
    }

    /// Get all search plugins
    pub async fn get_search_plugins(&self) -> Vec<search::SearchPlugin> {
        self.search_engine.get_plugins().await
    }

    /// Install a search plugin from URL
    pub async fn install_search_plugin(&self, url: &str) -> Result<search::SearchPlugin, String> {
        self.search_engine.install_plugin(url).await
    }

    /// Remove a search plugin
    pub async fn remove_search_plugin(&self, name: &str) -> Result<(), String> {
        self.search_engine.remove_plugin(name).await
    }

    /// Enable or disable a search plugin
    pub async fn set_search_plugin_enabled(&self, name: &str, enabled: bool) -> bool {
        self.search_engine.set_plugin_enabled(name, enabled).await
    }

    /// Start a search
    pub async fn start_search(
        &self,
        query: &str,
        plugins: Vec<String>,
        category: Option<String>,
    ) -> String {
        self.search_engine
            .start_search(query, plugins, category)
            .await
    }

    /// Stop a search
    pub async fn stop_search(&self, search_id: &str) -> bool {
        self.search_engine.stop_search(search_id).await
    }

    /// Get search status
    pub async fn get_search(&self, search_id: &str) -> Option<search::SearchJob> {
        self.search_engine.get_search(search_id).await
    }

    /// Get search results
    pub async fn get_search_results(&self, search_id: &str) -> Vec<search::SearchResult> {
        self.search_engine.get_search_results(search_id).await
    }

    /// Delete a search
    pub async fn delete_search(&self, search_id: &str) -> bool {
        self.search_engine.delete_search(search_id).await
    }

    /// Get all active searches
    pub async fn get_searches(&self) -> Vec<search::SearchJob> {
        self.search_engine.get_searches().await
    }

    // ========== Trackers ==========

    /// Add trackers to a torrent
    pub fn add_torrent_trackers(&self, hash: &str, trackers: Vec<String>) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            for url in trackers {
                if !torrent.trackers.contains(&url) {
                    torrent.trackers.push(url.clone());
                    torrent
                        .tracker_info
                        .push(tracker_info::TrackerInfo::new(url));
                }
            }
            true
        } else {
            false
        }
    }

    /// Remove a tracker from a torrent
    pub fn remove_torrent_tracker(&self, hash: &str, tracker_url: &str) -> bool {
        let mut torrents = self.torrents.write();
        if let Some(torrent) = torrents.get_mut(hash) {
            if let Some(pos) = torrent.trackers.iter().position(|u| u == tracker_url) {
                torrent.trackers.remove(pos);
                if pos < torrent.tracker_info.len() {
                    torrent.tracker_info.remove(pos);
                }
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

/// Recursively copies a file or directory tree (blocking; callers run it via
/// spawn_blocking). Used as the cross-filesystem fallback for moves.
fn copy_recursively(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursively(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}
