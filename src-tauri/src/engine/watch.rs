//! Watch folder implementation for automatically adding torrents.

#![allow(dead_code)]

pub use super::settings::WatchFolder;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};

/// Events from watch folder processing
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// A new torrent file was found
    TorrentFound {
        path: PathBuf,
        folder_id: String,
        category: Option<String>,
        tags: Vec<String>,
    },
}

/// Manages watch folders for auto-adding torrents
pub struct WatchFolderManager {
    /// Active watch folders
    folders: Arc<RwLock<HashMap<String, WatchFolder>>>,
    /// Files we've already processed (to avoid duplicates)
    processed_files: Arc<RwLock<HashSet<PathBuf>>>,
    /// Channel to send events
    event_tx: mpsc::UnboundedSender<WatchEvent>,
    /// Shutdown signal
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl WatchFolderManager {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<WatchEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        (
            Self {
                folders: Arc::new(RwLock::new(HashMap::new())),
                processed_files: Arc::new(RwLock::new(HashSet::new())),
                event_tx,
                shutdown_tx,
            },
            event_rx,
        )
    }

    /// Start the watch folder scanning loop
    pub fn start(&self) {
        let folders = self.folders.clone();
        let processed = self.processed_files.clone();
        let event_tx = self.event_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let scan_interval = Duration::from_secs(5);

            loop {
                tokio::select! {
                    _ = tokio::time::sleep(scan_interval) => {
                        Self::scan_folders(&folders, &processed, &event_tx).await;
                    }
                    result = shutdown_rx.recv() => {
                        // Only shutdown on successful receive, ignore errors (like Lagged)
                        if result.is_ok() {
                            tracing::info!("Watch folder manager shutting down");
                            break;
                        }
                    }
                }
            }
        });
    }

    async fn scan_folders(
        folders: &Arc<RwLock<HashMap<String, WatchFolder>>>,
        processed: &Arc<RwLock<HashSet<PathBuf>>>,
        event_tx: &mpsc::UnboundedSender<WatchEvent>,
    ) {
        let folders_snapshot: Vec<WatchFolder> = {
            let guard = folders.read().await;
            guard.values().filter(|f| f.enabled).cloned().collect()
        };

        for folder in folders_snapshot {
            if let Err(e) = Self::scan_folder(&folder, processed, event_tx).await {
                tracing::warn!("Error scanning watch folder {:?}: {}", folder.path, e);
            }
        }
    }

    async fn scan_folder(
        folder: &WatchFolder,
        processed: &Arc<RwLock<HashSet<PathBuf>>>,
        event_tx: &mpsc::UnboundedSender<WatchEvent>,
    ) -> Result<(), std::io::Error> {
        let mut entries = tokio::fs::read_dir(&folder.path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Only process .torrent files
            if path.extension().and_then(|e| e.to_str()) != Some("torrent") {
                continue;
            }

            // Check if already processed
            {
                let guard = processed.read().await;
                if guard.contains(&path) {
                    continue;
                }
            }

            // Mark as processed
            {
                let mut guard = processed.write().await;
                guard.insert(path.clone());
            }

            // Send event
            let event = WatchEvent::TorrentFound {
                path,
                folder_id: folder.id.clone(),
                category: folder.category.clone(),
                tags: folder.tags.clone(),
            };

            if event_tx.send(event).is_err() {
                tracing::warn!("Watch event receiver dropped");
                break;
            }
        }

        Ok(())
    }

    /// Add a watch folder
    pub async fn add_folder(&self, folder: WatchFolder) {
        let id = folder.id.clone();
        self.folders.write().await.insert(id, folder);
    }

    /// Remove a watch folder
    pub async fn remove_folder(&self, id: &str) -> Option<WatchFolder> {
        self.folders.write().await.remove(id)
    }

    /// Get all watch folders
    pub async fn get_folders(&self) -> Vec<WatchFolder> {
        self.folders.read().await.values().cloned().collect()
    }

    /// Update a watch folder
    pub async fn update_folder(&self, folder: WatchFolder) -> bool {
        let mut guard = self.folders.write().await;
        if guard.contains_key(&folder.id) {
            guard.insert(folder.id.clone(), folder);
            true
        } else {
            false
        }
    }

    /// Clear processed files cache (useful when re-enabling a folder)
    pub async fn clear_processed(&self) {
        self.processed_files.write().await.clear();
    }

    /// Shutdown the manager
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

impl Default for WatchFolderManager {
    fn default() -> Self {
        Self::new().0
    }
}
