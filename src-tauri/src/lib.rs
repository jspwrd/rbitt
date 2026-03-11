use oxidebt_torrent::{Metainfo, TorrentVersion};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

mod engine;

use engine::TorrentEngine;

pub struct AppState {
    engine: Arc<RwLock<Option<TorrentEngine>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            engine: Arc::new(RwLock::new(None)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub name: String,
    pub info_hash: String,
    pub version: String,
    pub total_size: u64,
    pub piece_count: usize,
    pub piece_length: u64,
    pub file_count: usize,
    pub files: Vec<FileInfo>,
    pub trackers: Vec<String>,
    pub is_private: bool,
    pub comment: Option<String>,
    pub created_by: Option<String>,
    pub creation_date: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
}

#[tauri::command]
async fn parse_torrent(path: String) -> Result<TorrentInfo, String> {
    let data = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let meta =
        Metainfo::from_bytes(&data).map_err(|e| format!("Failed to parse torrent: {}", e))?;

    let version = match meta.version {
        TorrentVersion::V1 => "v1",
        TorrentVersion::V2 => "v2",
        TorrentVersion::Hybrid => "hybrid",
    };

    let info_hash = match &meta.info_hash {
        oxidebt_torrent::InfoHash::V1(h) => h.to_hex(),
        oxidebt_torrent::InfoHash::V2(h) => h.to_hex(),
        oxidebt_torrent::InfoHash::Hybrid { v1, .. } => v1.to_hex(),
    };

    let files: Vec<FileInfo> = meta
        .info
        .files
        .iter()
        .map(|f| FileInfo {
            path: f.path.display().to_string(),
            size: f.length,
        })
        .collect();

    Ok(TorrentInfo {
        name: meta.info.name.clone(),
        info_hash,
        version: version.to_string(),
        total_size: meta.info.total_length,
        piece_count: meta.piece_count(),
        piece_length: meta.info.piece_length,
        file_count: files.len(),
        files,
        trackers: meta.tracker_urls(),
        is_private: meta.is_private(),
        comment: meta.comment.clone(),
        created_by: meta.created_by.clone(),
        creation_date: meta.creation_date,
    })
}

#[tauri::command]
async fn parse_magnet(uri: String) -> Result<MagnetInfo, String> {
    let magnet = oxidebt_torrent::MagnetLink::parse(&uri)
        .map_err(|e| format!("Failed to parse magnet: {}", e))?;

    let info_hash = match &magnet.info_hash {
        oxidebt_torrent::InfoHash::V1(h) => h.to_hex(),
        oxidebt_torrent::InfoHash::V2(h) => h.to_hex(),
        oxidebt_torrent::InfoHash::Hybrid { v1, .. } => v1.to_hex(),
    };

    Ok(MagnetInfo {
        info_hash,
        display_name: magnet.display_name,
        trackers: magnet.trackers,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagnetInfo {
    pub info_hash: String,
    pub display_name: Option<String>,
    pub trackers: Vec<String>,
}

#[tauri::command]
async fn init_engine(state: State<'_, AppState>, download_dir: String) -> Result<(), String> {
    let engine = TorrentEngine::new(PathBuf::from(download_dir))
        .await
        .map_err(|e| format!("Failed to init engine: {}", e))?;

    *state.engine.write().await = Some(engine);
    Ok(())
}

#[tauri::command]
async fn add_torrent(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .add_torrent_file(&path)
        .await
        .map_err(|e| format!("Failed to add torrent: {}", e))
}

#[tauri::command]
async fn add_magnet(state: State<'_, AppState>, uri: String) -> Result<String, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .add_magnet(&uri)
        .await
        .map_err(|e| format!("Failed to add magnet: {}", e))
}

#[tauri::command]
async fn get_torrents(state: State<'_, AppState>) -> Result<Vec<TorrentStatus>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.get_all_status().await)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentStatus {
    pub info_hash: String,
    pub name: String,
    /// qBittorrent-compatible state (e.g., "downloading", "uploading", "stalledDL", "stalledUP", "pausedDL", "pausedUP")
    pub state: String,
    pub progress: f64,
    pub download_rate: f64,
    pub upload_rate: f64,
    pub downloaded: u64,
    pub uploaded: u64,
    pub total_size: u64,
    pub peers: usize,
    pub seeds: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerStatusInfo {
    pub url: String,
    pub status: String,
    pub peers: u32,
    pub seeds: u32,
    pub leechers: u32,
    pub last_announce: Option<u64>,
    pub next_announce: Option<u64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatusInfo {
    pub address: String,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub is_choking_us: bool,
    pub is_interested: bool,
    pub progress: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalStats {
    pub download_rate: f64,
    pub upload_rate: f64,
    pub total_downloaded: u64,
    pub total_uploaded: u64,
    pub active_torrents: usize,
    pub total_peers: usize,
    pub global_connections: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFileInfo {
    pub path: String,
    pub size: u64,
    pub progress: f64,
    pub downloaded: u64,
}

#[tauri::command]
async fn get_torrent_trackers(
    state: State<'_, AppState>,
    info_hash: String,
) -> Result<Vec<TrackerStatusInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .get_tracker_info(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())
}

#[tauri::command]
async fn get_torrent_peers(
    state: State<'_, AppState>,
    info_hash: String,
) -> Result<Vec<PeerStatusInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .get_torrent_peers(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())
}

#[tauri::command]
async fn get_torrent_files(
    state: State<'_, AppState>,
    info_hash: String,
) -> Result<Vec<TorrentFileInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .get_torrent_files(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())
}

#[tauri::command]
async fn get_global_stats(state: State<'_, AppState>) -> Result<GlobalStats, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.get_global_stats())
}

#[tauri::command]
async fn pause_torrent(state: State<'_, AppState>, info_hash: String) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .pause(&info_hash)
        .await
        .map_err(|e| format!("Failed to pause: {}", e))
}

#[tauri::command]
async fn resume_torrent(state: State<'_, AppState>, info_hash: String) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .resume(&info_hash)
        .await
        .map_err(|e| format!("Failed to resume: {}", e))
}

#[tauri::command]
async fn remove_torrent(
    state: State<'_, AppState>,
    info_hash: String,
    delete_files: bool,
) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .remove(&info_hash, delete_files)
        .await
        .map_err(|e| format!("Failed to remove: {}", e))
}

#[tauri::command]
async fn set_bandwidth_limits(
    state: State<'_, AppState>,
    download_limit: u64,
    upload_limit: u64,
) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.set_bandwidth_limits(download_limit, upload_limit);
    Ok(())
}

#[tauri::command]
async fn set_queue_settings(
    state: State<'_, AppState>,
    max_downloads: usize,
    max_uploads: usize,
) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.set_queue_settings(max_downloads, max_uploads);
    Ok(())
}

#[tauri::command]
async fn get_queue_settings(state: State<'_, AppState>) -> Result<(usize, usize), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.get_queue_settings())
}

#[tauri::command]
async fn set_no_seed_mode(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.set_no_seed_mode(enabled);
    Ok(())
}

#[tauri::command]
async fn get_no_seed_mode(state: State<'_, AppState>) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.is_no_seed_mode())
}

#[tauri::command]
async fn set_disconnect_on_complete(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.set_disconnect_on_complete(enabled);
    Ok(())
}

#[tauri::command]
async fn get_disconnect_on_complete(state: State<'_, AppState>) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.is_disconnect_on_complete())
}

#[tauri::command]
async fn parse_torrent_bytes(data: Vec<u8>) -> Result<TorrentInfo, String> {
    let meta =
        Metainfo::from_bytes(&data).map_err(|e| format!("Failed to parse torrent: {}", e))?;

    let version = match meta.version {
        TorrentVersion::V1 => "v1",
        TorrentVersion::V2 => "v2",
        TorrentVersion::Hybrid => "hybrid",
    };

    let info_hash = match &meta.info_hash {
        oxidebt_torrent::InfoHash::V1(h) => h.to_hex(),
        oxidebt_torrent::InfoHash::V2(h) => h.to_hex(),
        oxidebt_torrent::InfoHash::Hybrid { v1, .. } => v1.to_hex(),
    };

    let files: Vec<FileInfo> = meta
        .info
        .files
        .iter()
        .map(|f| FileInfo {
            path: f.path.display().to_string(),
            size: f.length,
        })
        .collect();

    Ok(TorrentInfo {
        name: meta.info.name.clone(),
        info_hash,
        version: version.to_string(),
        total_size: meta.info.total_length,
        piece_count: meta.piece_count(),
        piece_length: meta.info.piece_length,
        file_count: files.len(),
        files,
        trackers: meta.tracker_urls(),
        is_private: meta.is_private(),
        comment: meta.comment.clone(),
        created_by: meta.created_by.clone(),
        creation_date: meta.creation_date,
    })
}

#[tauri::command]
async fn add_torrent_bytes(state: State<'_, AppState>, data: Vec<u8>) -> Result<String, String> {
    tracing::info!("add_torrent_bytes called with {} bytes", data.len());
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let result = engine
        .add_torrent_bytes(&data)
        .await
        .map_err(|e| format!("Failed to add torrent: {}", e));

    match &result {
        Ok(hash) => tracing::info!("add_torrent_bytes succeeded: {}", hash),
        Err(e) => tracing::error!("add_torrent_bytes failed: {}", e),
    }

    result
}

#[tauri::command]
fn get_default_download_dir() -> Result<String, String> {
    dirs::download_dir()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not determine default downloads directory".to_string())
}

// ========== Sequential Download ==========

#[tauri::command]
async fn set_sequential_download(
    state: State<'_, AppState>,
    info_hash: String,
    enabled: bool,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.set_sequential_download(&info_hash, enabled))
}

#[tauri::command]
async fn get_sequential_download(
    state: State<'_, AppState>,
    info_hash: String,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .get_sequential_download(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())
}

// ========== File Priorities ==========

#[tauri::command]
async fn set_file_priority(
    state: State<'_, AppState>,
    info_hash: String,
    file_index: usize,
    priority: u8,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.set_file_priority(
        &info_hash,
        file_index,
        engine::FilePriority::from_u8(priority),
    ))
}

#[tauri::command]
async fn set_all_file_priorities(
    state: State<'_, AppState>,
    info_hash: String,
    priorities: Vec<u8>,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.set_all_file_priorities(&info_hash, priorities))
}

#[tauri::command]
async fn get_file_priorities(
    state: State<'_, AppState>,
    info_hash: String,
) -> Result<Vec<u8>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .get_file_priorities(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())
}

// ========== Categories ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    pub name: String,
    pub save_path: String,
}

#[tauri::command]
async fn add_category(
    state: State<'_, AppState>,
    name: String,
    save_path: String,
) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.add_category(name, PathBuf::from(save_path));
    Ok(())
}

#[tauri::command]
async fn remove_category(state: State<'_, AppState>, name: String) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.remove_category(&name))
}

#[tauri::command]
async fn get_categories(state: State<'_, AppState>) -> Result<Vec<CategoryInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine
        .get_categories()
        .into_iter()
        .map(|c| CategoryInfo {
            name: c.name,
            save_path: c.save_path.to_string_lossy().to_string(),
        })
        .collect())
}

#[tauri::command]
async fn set_torrent_category(
    state: State<'_, AppState>,
    info_hash: String,
    category: Option<String>,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.set_torrent_category(&info_hash, category))
}

// ========== Tags ==========

#[tauri::command]
async fn add_torrent_tag(
    state: State<'_, AppState>,
    info_hash: String,
    tag: String,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.add_torrent_tag(&info_hash, tag))
}

#[tauri::command]
async fn remove_torrent_tag(
    state: State<'_, AppState>,
    info_hash: String,
    tag: String,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.remove_torrent_tag(&info_hash, &tag))
}

#[tauri::command]
async fn get_torrent_tags(
    state: State<'_, AppState>,
    info_hash: String,
) -> Result<Vec<String>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .get_torrent_tags(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())
}

#[tauri::command]
async fn set_torrent_tags(
    state: State<'_, AppState>,
    info_hash: String,
    tags: Vec<String>,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.set_torrent_tags(&info_hash, tags))
}

// ========== Share Limits ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareLimitsInfo {
    pub max_ratio: Option<f64>,
    pub max_seeding_time: Option<u64>,
    pub limit_action: String,
}

#[tauri::command]
async fn set_torrent_share_limits(
    state: State<'_, AppState>,
    info_hash: String,
    max_ratio: Option<f64>,
    max_seeding_time: Option<u64>,
    limit_action: String,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let action = match limit_action.as_str() {
        "remove" => engine::LimitAction::Remove,
        "remove_with_files" => engine::LimitAction::RemoveWithFiles,
        _ => engine::LimitAction::Pause,
    };

    Ok(engine.set_torrent_share_limits(&info_hash, max_ratio, max_seeding_time, action))
}

#[tauri::command]
async fn get_torrent_share_limits(
    state: State<'_, AppState>,
    info_hash: String,
) -> Result<ShareLimitsInfo, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let limits = engine
        .get_torrent_share_limits(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())?;

    Ok(ShareLimitsInfo {
        max_ratio: limits.max_ratio,
        max_seeding_time: limits.max_seeding_time,
        limit_action: match limits.limit_action {
            engine::LimitAction::Pause => "pause".to_string(),
            engine::LimitAction::Remove => "remove".to_string(),
            engine::LimitAction::RemoveWithFiles => "remove_with_files".to_string(),
        },
    })
}

#[tauri::command]
async fn get_torrent_ratio(state: State<'_, AppState>, info_hash: String) -> Result<f64, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .get_torrent_ratio(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())
}

#[tauri::command]
async fn get_torrent_seeding_time(
    state: State<'_, AppState>,
    info_hash: String,
) -> Result<u64, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine
        .get_torrent_seeding_time(&info_hash)
        .ok_or_else(|| "Torrent not found".to_string())
}

// ========== Auto-Add Trackers ==========

#[tauri::command]
async fn set_auto_tracker_settings(
    state: State<'_, AppState>,
    enabled: bool,
    trackers: Vec<String>,
) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.set_auto_tracker_settings(enabled, trackers);
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTrackerSettingsInfo {
    pub enabled: bool,
    pub trackers: Vec<String>,
}

#[tauri::command]
async fn get_auto_tracker_settings(
    state: State<'_, AppState>,
) -> Result<AutoTrackerSettingsInfo, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let settings = engine.get_auto_tracker_settings();
    Ok(AutoTrackerSettingsInfo {
        enabled: settings.enabled,
        trackers: settings.trackers,
    })
}

#[tauri::command]
async fn add_torrent_trackers(
    state: State<'_, AppState>,
    info_hash: String,
    trackers: Vec<String>,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.add_torrent_trackers(&info_hash, trackers))
}

#[tauri::command]
async fn remove_torrent_tracker(
    state: State<'_, AppState>,
    info_hash: String,
    tracker_url: String,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.remove_torrent_tracker(&info_hash, &tracker_url))
}

// ========== Move on Completion ==========

#[tauri::command]
async fn set_move_on_complete_settings(
    state: State<'_, AppState>,
    enabled: bool,
    target_path: Option<String>,
    use_category_path: bool,
) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.set_move_on_complete_settings(
        enabled,
        target_path.map(PathBuf::from),
        use_category_path,
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveOnCompleteSettingsInfo {
    pub enabled: bool,
    pub target_path: Option<String>,
    pub use_category_path: bool,
}

#[tauri::command]
async fn get_move_on_complete_settings(
    state: State<'_, AppState>,
) -> Result<MoveOnCompleteSettingsInfo, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let settings = engine.get_move_on_complete_settings();
    Ok(MoveOnCompleteSettingsInfo {
        enabled: settings.enabled,
        target_path: settings
            .target_path
            .map(|p| p.to_string_lossy().to_string()),
        use_category_path: settings.use_category_path,
    })
}

// ========== External Program ==========

#[tauri::command]
async fn set_external_program_settings(
    state: State<'_, AppState>,
    on_completion_enabled: bool,
    command: Option<String>,
) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.set_external_program_settings(on_completion_enabled, command);
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalProgramSettingsInfo {
    pub on_completion_enabled: bool,
    pub on_completion_command: Option<String>,
}

#[tauri::command]
async fn get_external_program_settings(
    state: State<'_, AppState>,
) -> Result<ExternalProgramSettingsInfo, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let settings = engine.get_external_program_settings();
    Ok(ExternalProgramSettingsInfo {
        on_completion_enabled: settings.on_completion_enabled,
        on_completion_command: settings.on_completion_command,
    })
}

// ========== Watch Folders ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchFolderInfo {
    pub id: String,
    pub path: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub process_existing: bool,
    pub enabled: bool,
}

#[tauri::command]
async fn add_watch_folder(
    state: State<'_, AppState>,
    path: String,
    category: Option<String>,
    tags: Vec<String>,
    process_existing: bool,
    enabled: bool,
) -> Result<String, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let id = uuid::Uuid::new_v4().to_string();
    let folder = engine::WatchFolder {
        id: id.clone(),
        path: PathBuf::from(path),
        category,
        tags,
        process_existing,
        enabled,
    };

    engine.add_watch_folder(folder).await;
    Ok(id)
}

#[tauri::command]
async fn remove_watch_folder(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.remove_watch_folder(&id).await)
}

#[tauri::command]
async fn get_watch_folders(state: State<'_, AppState>) -> Result<Vec<WatchFolderInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine
        .get_watch_folders()
        .await
        .into_iter()
        .map(|f| WatchFolderInfo {
            id: f.id,
            path: f.path.to_string_lossy().to_string(),
            category: f.category,
            tags: f.tags,
            process_existing: f.process_existing,
            enabled: f.enabled,
        })
        .collect())
}

// ========== RSS ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssFeedInfo {
    pub id: String,
    pub url: String,
    pub name: String,
    pub enabled: bool,
    pub refresh_interval: u64,
    pub last_refresh: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssRuleInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub must_contain: String,
    pub must_not_contain: String,
    pub use_regex: bool,
    pub episode_filter: Option<String>,
    pub affected_feeds: Vec<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub save_path: Option<String>,
    pub add_paused: bool,
    pub last_match: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssItemInfo {
    pub title: String,
    pub torrent_url: String,
    pub link: Option<String>,
    pub pub_date: Option<u64>,
    pub description: Option<String>,
    pub is_downloaded: bool,
}

#[tauri::command]
async fn add_rss_feed(
    state: State<'_, AppState>,
    url: String,
    name: String,
    refresh_interval: u64,
) -> Result<String, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let id = uuid::Uuid::new_v4().to_string();
    let feed = engine::RssFeed {
        id: id.clone(),
        url,
        name,
        enabled: true,
        refresh_interval,
        last_refresh: None,
        last_error: None,
    };

    engine.add_rss_feed(feed).await;
    Ok(id)
}

#[tauri::command]
async fn remove_rss_feed(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.remove_rss_feed(&id).await)
}

#[tauri::command]
async fn get_rss_feeds(state: State<'_, AppState>) -> Result<Vec<RssFeedInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine
        .get_rss_feeds()
        .await
        .into_iter()
        .map(|f| RssFeedInfo {
            id: f.id,
            url: f.url,
            name: f.name,
            enabled: f.enabled,
            refresh_interval: f.refresh_interval,
            last_refresh: f.last_refresh,
            last_error: f.last_error,
        })
        .collect())
}

#[tauri::command]
async fn refresh_rss_feed(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.refresh_rss_feed(&id).await
}

#[tauri::command]
async fn get_rss_feed_items(
    state: State<'_, AppState>,
    feed_id: String,
) -> Result<Vec<RssItemInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine
        .get_rss_feed_items(&feed_id)
        .await
        .into_iter()
        .map(|i| RssItemInfo {
            title: i.title,
            torrent_url: i.torrent_url,
            link: i.link,
            pub_date: i.pub_date,
            description: i.description,
            is_downloaded: i.is_downloaded,
        })
        .collect())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn add_rss_rule(
    state: State<'_, AppState>,
    name: String,
    must_contain: String,
    must_not_contain: String,
    use_regex: bool,
    episode_filter: Option<String>,
    affected_feeds: Vec<String>,
    category: Option<String>,
    tags: Vec<String>,
    save_path: Option<String>,
    add_paused: bool,
) -> Result<String, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let id = uuid::Uuid::new_v4().to_string();
    let rule = engine::RssDownloadRule {
        id: id.clone(),
        name,
        enabled: true,
        must_contain,
        must_not_contain,
        use_regex,
        episode_filter,
        affected_feeds,
        category,
        tags,
        save_path,
        add_paused,
        last_match: None,
    };

    engine.add_rss_rule(rule).await;
    Ok(id)
}

#[tauri::command]
async fn remove_rss_rule(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.remove_rss_rule(&id).await)
}

#[tauri::command]
async fn get_rss_rules(state: State<'_, AppState>) -> Result<Vec<RssRuleInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine
        .get_rss_rules()
        .await
        .into_iter()
        .map(|r| RssRuleInfo {
            id: r.id,
            name: r.name,
            enabled: r.enabled,
            must_contain: r.must_contain,
            must_not_contain: r.must_not_contain,
            use_regex: r.use_regex,
            episode_filter: r.episode_filter,
            affected_feeds: r.affected_feeds,
            category: r.category,
            tags: r.tags,
            save_path: r.save_path,
            add_paused: r.add_paused,
            last_match: r.last_match,
        })
        .collect())
}

// ========== Search Engine ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPluginInfo {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub enabled: bool,
    pub categories: Vec<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultInfo {
    pub name: String,
    pub download_link: String,
    pub size: u64,
    pub seeders: i32,
    pub leechers: i32,
    pub plugin: String,
    pub description_link: Option<String>,
    pub pub_date: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchJobInfo {
    pub id: String,
    pub query: String,
    pub plugins: Vec<String>,
    pub category: Option<String>,
    pub status: String,
    pub results_count: usize,
    pub error: Option<String>,
}

#[tauri::command]
async fn load_search_plugins(state: State<'_, AppState>) -> Result<usize, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.load_search_plugins().await
}

#[tauri::command]
async fn get_search_plugins(state: State<'_, AppState>) -> Result<Vec<SearchPluginInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine
        .get_search_plugins()
        .await
        .into_iter()
        .map(|p| SearchPluginInfo {
            name: p.name,
            display_name: p.display_name,
            version: p.version,
            enabled: p.enabled,
            categories: p.categories,
            url: p.url,
        })
        .collect())
}

#[tauri::command]
async fn install_search_plugin(
    state: State<'_, AppState>,
    url: String,
) -> Result<SearchPluginInfo, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let p = engine.install_search_plugin(&url).await?;
    Ok(SearchPluginInfo {
        name: p.name,
        display_name: p.display_name,
        version: p.version,
        enabled: p.enabled,
        categories: p.categories,
        url: p.url,
    })
}

#[tauri::command]
async fn remove_search_plugin(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    engine.remove_search_plugin(&name).await
}

#[tauri::command]
async fn set_search_plugin_enabled(
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.set_search_plugin_enabled(&name, enabled).await)
}

#[tauri::command]
async fn start_search(
    state: State<'_, AppState>,
    query: String,
    plugins: Vec<String>,
    category: Option<String>,
) -> Result<String, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.start_search(&query, plugins, category).await)
}

#[tauri::command]
async fn stop_search(state: State<'_, AppState>, search_id: String) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.stop_search(&search_id).await)
}

#[tauri::command]
async fn get_search_status(
    state: State<'_, AppState>,
    search_id: String,
) -> Result<SearchJobInfo, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    let job = engine
        .get_search(&search_id)
        .await
        .ok_or_else(|| "Search not found".to_string())?;

    Ok(SearchJobInfo {
        id: job.id,
        query: job.query,
        plugins: job.plugins,
        category: job.category,
        status: match job.status {
            engine::SearchStatus::Running => "running".to_string(),
            engine::SearchStatus::Completed => "completed".to_string(),
            engine::SearchStatus::Stopped => "stopped".to_string(),
            engine::SearchStatus::Failed => "failed".to_string(),
        },
        results_count: job.results.len(),
        error: job.error,
    })
}

#[tauri::command]
async fn get_search_results(
    state: State<'_, AppState>,
    search_id: String,
) -> Result<Vec<SearchResultInfo>, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine
        .get_search_results(&search_id)
        .await
        .into_iter()
        .map(|r| SearchResultInfo {
            name: r.name,
            download_link: r.download_link,
            size: r.size,
            seeders: r.seeders,
            leechers: r.leechers,
            plugin: r.plugin,
            description_link: r.description_link,
            pub_date: r.pub_date,
        })
        .collect())
}

#[tauri::command]
async fn delete_search(state: State<'_, AppState>, search_id: String) -> Result<bool, String> {
    let guard = state.engine.read().await;
    let engine = guard
        .as_ref()
        .ok_or_else(|| "Engine not initialized".to_string())?;

    Ok(engine.delete_search(&search_id).await)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rbitt=debug".parse().unwrap())
                .add_directive("rbitt_lib=debug".parse().unwrap())
                .add_directive("oxidebt_tracker=debug".parse().unwrap())
                .add_directive("oxidebt_peer=debug".parse().unwrap())
                .add_directive("oxidebt_dht=debug".parse().unwrap()),
        )
        .with_target(true)
        .init();

    tracing::info!("RBitt starting up...");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Core
            parse_torrent,
            parse_torrent_bytes,
            parse_magnet,
            init_engine,
            add_torrent,
            add_torrent_bytes,
            add_magnet,
            get_torrents,
            get_torrent_trackers,
            get_torrent_peers,
            get_torrent_files,
            get_global_stats,
            pause_torrent,
            resume_torrent,
            remove_torrent,
            set_bandwidth_limits,
            set_queue_settings,
            get_queue_settings,
            set_no_seed_mode,
            get_no_seed_mode,
            set_disconnect_on_complete,
            get_disconnect_on_complete,
            get_default_download_dir,
            // Sequential download
            set_sequential_download,
            get_sequential_download,
            // File priorities
            set_file_priority,
            set_all_file_priorities,
            get_file_priorities,
            // Categories
            add_category,
            remove_category,
            get_categories,
            set_torrent_category,
            // Tags
            add_torrent_tag,
            remove_torrent_tag,
            get_torrent_tags,
            set_torrent_tags,
            // Share limits
            set_torrent_share_limits,
            get_torrent_share_limits,
            get_torrent_ratio,
            get_torrent_seeding_time,
            // Auto-add trackers
            set_auto_tracker_settings,
            get_auto_tracker_settings,
            add_torrent_trackers,
            remove_torrent_tracker,
            // Move on completion
            set_move_on_complete_settings,
            get_move_on_complete_settings,
            // External program
            set_external_program_settings,
            get_external_program_settings,
            // Watch folders
            add_watch_folder,
            remove_watch_folder,
            get_watch_folders,
            // RSS
            add_rss_feed,
            remove_rss_feed,
            get_rss_feeds,
            refresh_rss_feed,
            get_rss_feed_items,
            add_rss_rule,
            remove_rss_rule,
            get_rss_rules,
            // Search
            load_search_plugins,
            get_search_plugins,
            install_search_plugin,
            remove_search_plugin,
            set_search_plugin_enabled,
            start_search,
            stop_search,
            get_search_status,
            get_search_results,
            delete_search,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
