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
async fn set_disconnect_on_complete(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
