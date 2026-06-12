//! Settings and session persistence.
//!
//! Layout under the platform config directory (`<config>/rbitt/`):
//! - `settings.json` — engine configuration (categories, limits, RSS, watch
//!   folders, ...)
//! - `session.json` — the torrent list and per-torrent state
//! - `torrents/<info_hash>.info` — each torrent's raw bencoded info dict;
//!   together with the persisted tracker list this reconstructs a `Metainfo`
//!   via `Metainfo::from_info_dict` (creation date/comment are not retained)

use super::rss::{RssDownloadRule, RssFeed};
use super::settings::{
    AutoTrackerSettings, ExternalProgramSettings, MoveOnCompleteSettings, ShareLimits, WatchFolder,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedSettings {
    pub download_dir: Option<PathBuf>,
    pub categories: Vec<super::settings::Category>,
    pub auto_trackers: AutoTrackerSettings,
    pub move_on_complete: MoveOnCompleteSettings,
    pub external_program: ExternalProgramSettings,
    pub default_share_limits: ShareLimits,
    pub watch_folders: Vec<WatchFolder>,
    pub rss_feeds: Vec<RssFeed>,
    pub rss_rules: Vec<RssDownloadRule>,
    /// `feed_id:torrent_url` keys of items already downloaded by RSS rules.
    pub rss_downloaded: Vec<String>,
    pub max_active_downloads: usize,
    pub max_active_uploads: usize,
    /// Bytes per second; 0 means unlimited.
    pub download_limit: u64,
    pub upload_limit: u64,
    pub no_seed_mode: bool,
    pub disconnect_on_complete: bool,
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            download_dir: None,
            categories: Vec::new(),
            auto_trackers: AutoTrackerSettings::default(),
            move_on_complete: MoveOnCompleteSettings::default(),
            external_program: ExternalProgramSettings::default(),
            default_share_limits: ShareLimits::default(),
            watch_folders: Vec::new(),
            rss_feeds: Vec::new(),
            rss_rules: Vec::new(),
            rss_downloaded: Vec::new(),
            max_active_downloads: 5,
            max_active_uploads: 5,
            download_limit: 0,
            upload_limit: 0,
            no_seed_mode: false,
            disconnect_on_complete: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PersistedSession {
    /// Torrents in queue (FIFO) order.
    pub torrents: Vec<PersistedTorrent>,
    /// Magnet URIs that were still fetching metadata; re-fetched on startup.
    pub pending_magnets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTorrent {
    pub info_hash: String,
    pub save_path: PathBuf,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub share_limits: ShareLimits,
    pub file_priorities: Vec<u8>,
    pub sequential_download: bool,
    pub paused: bool,
    /// Lifetime uploaded bytes, restored for share-ratio accounting.
    /// (Downloaded bytes are re-derived from verification on restore.)
    pub uploaded: u64,
    /// Full live tracker list, including trackers added after the original
    /// metainfo (and used to reconstruct the metainfo from the info dict).
    pub trackers: Vec<String>,
    pub move_on_complete: Option<PathBuf>,
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rbitt")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

fn session_path() -> PathBuf {
    config_dir().join("session.json")
}

fn torrent_blob_path(info_hash: &str) -> PathBuf {
    // Info hashes are hex strings, so they are safe as file names.
    config_dir()
        .join("torrents")
        .join(format!("{info_hash}.info"))
}

/// Writes via a unique temp file + rename so a crash mid-write never leaves
/// a truncated file, and concurrent writers cannot clobber each other's temp.
async fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), nonce));
    tokio::fs::write(&tmp, data).await?;
    match tokio::fs::rename(&tmp, path).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

pub async fn save_settings(settings: &PersistedSettings) -> std::io::Result<()> {
    let data = serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?;
    write_atomic(&settings_path(), &data).await
}

pub async fn load_settings() -> Option<PersistedSettings> {
    let data = tokio::fs::read(settings_path()).await.ok()?;
    match serde_json::from_slice(&data) {
        Ok(settings) => Some(settings),
        Err(e) => {
            tracing::warn!("Failed to parse settings.json, using defaults: {}", e);
            None
        }
    }
}

pub async fn save_session(session: &PersistedSession) -> std::io::Result<()> {
    let data = serde_json::to_vec_pretty(session).map_err(std::io::Error::other)?;
    write_atomic(&session_path(), &data).await
}

pub async fn load_session() -> Option<PersistedSession> {
    let data = tokio::fs::read(session_path()).await.ok()?;
    match serde_json::from_slice(&data) {
        Ok(session) => Some(session),
        Err(e) => {
            tracing::warn!("Failed to parse session.json, starting empty: {}", e);
            None
        }
    }
}

pub async fn save_torrent_blob(info_hash: &str, raw_info: &[u8]) -> std::io::Result<()> {
    write_atomic(&torrent_blob_path(info_hash), raw_info).await
}

pub async fn load_torrent_blob(info_hash: &str) -> Option<Vec<u8>> {
    tokio::fs::read(torrent_blob_path(info_hash)).await.ok()
}

pub async fn delete_torrent_blob(info_hash: &str) {
    let _ = tokio::fs::remove_file(torrent_blob_path(info_hash)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let mut settings = PersistedSettings {
            download_dir: Some(PathBuf::from("/downloads")),
            max_active_downloads: 3,
            download_limit: 1024,
            no_seed_mode: true,
            ..Default::default()
        };
        settings.rss_downloaded.push("feed:url".to_string());

        let json = serde_json::to_vec(&settings).unwrap();
        let parsed: PersistedSettings = serde_json::from_slice(&json).unwrap();
        assert_eq!(parsed.download_dir, settings.download_dir);
        assert_eq!(parsed.max_active_downloads, 3);
        assert_eq!(parsed.download_limit, 1024);
        assert!(parsed.no_seed_mode);
        assert_eq!(parsed.rss_downloaded, settings.rss_downloaded);
    }

    #[test]
    fn settings_tolerates_missing_fields() {
        // Older or hand-edited files must load with defaults, not fail.
        let parsed: PersistedSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.max_active_downloads, 5);
        assert_eq!(parsed.max_active_uploads, 5);
        assert!(!parsed.no_seed_mode);
    }

    #[test]
    fn session_round_trip() {
        let session = PersistedSession {
            torrents: vec![PersistedTorrent {
                info_hash: "ab".repeat(20),
                save_path: PathBuf::from("/downloads"),
                category: Some("tv".to_string()),
                tags: vec!["a".to_string()],
                share_limits: ShareLimits::default(),
                file_priorities: vec![0, 4, 7],
                sequential_download: true,
                paused: true,
                uploaded: 42,
                trackers: vec!["http://tracker/announce".to_string()],
                move_on_complete: None,
            }],
            pending_magnets: vec!["magnet:?xt=urn:btih:abcd".to_string()],
        };

        let json = serde_json::to_vec(&session).unwrap();
        let parsed: PersistedSession = serde_json::from_slice(&json).unwrap();
        assert_eq!(parsed.torrents.len(), 1);
        let t = &parsed.torrents[0];
        assert_eq!(t.file_priorities, vec![0, 4, 7]);
        assert!(t.paused);
        assert_eq!(t.uploaded, 42);
        assert_eq!(parsed.pending_magnets.len(), 1);
    }
}
