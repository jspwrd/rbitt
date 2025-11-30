//! Engine settings and configuration types.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// File priority levels matching qBittorrent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FilePriority {
    /// Do not download this file
    Skip = 0,
    /// Low priority
    Low = 1,
    /// Normal priority (default)
    #[default]
    Normal = 4,
    /// High priority
    High = 6,
    /// Maximum priority
    Maximum = 7,
}

impl FilePriority {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => FilePriority::Skip,
            1..=3 => FilePriority::Low,
            4..=5 => FilePriority::Normal,
            6 => FilePriority::High,
            7.. => FilePriority::Maximum,
        }
    }
}

/// A category with an associated save path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub save_path: PathBuf,
}

/// Share ratio/time limit settings for a torrent
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShareLimits {
    /// Maximum share ratio (uploaded / downloaded). None = unlimited
    pub max_ratio: Option<f64>,
    /// Maximum seeding time in seconds. None = unlimited
    pub max_seeding_time: Option<u64>,
    /// Action to take when limits are reached
    pub limit_action: LimitAction,
}

/// Action to take when share limits are reached
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LimitAction {
    /// Pause the torrent
    #[default]
    Pause,
    /// Remove the torrent (keep files)
    Remove,
    /// Remove the torrent and delete files
    RemoveWithFiles,
}

/// Settings for automatic tracker addition
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoTrackerSettings {
    /// Whether to automatically add trackers to new torrents
    pub enabled: bool,
    /// List of tracker URLs to add
    pub trackers: Vec<String>,
}

/// Settings for move-on-completion feature
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoveOnCompleteSettings {
    /// Whether to move completed torrents
    pub enabled: bool,
    /// Target directory for completed torrents (if not using category path)
    pub target_path: Option<PathBuf>,
    /// Whether to use category save path instead of target_path
    pub use_category_path: bool,
}

/// Settings for running external programs on events
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExternalProgramSettings {
    /// Whether to run program on torrent completion
    pub on_completion_enabled: bool,
    /// Program/script to run on completion
    /// Supports placeholders: %N (name), %F (content path), %R (root path), %D (save path), %I (info hash)
    pub on_completion_command: Option<String>,
}

/// Watch folder configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchFolder {
    /// Unique identifier for this watch folder
    pub id: String,
    /// Path to watch for .torrent files
    pub path: PathBuf,
    /// Category to assign to added torrents (optional)
    pub category: Option<String>,
    /// Tags to assign to added torrents
    pub tags: Vec<String>,
    /// Whether to process existing files when watch is started
    pub process_existing: bool,
    /// Whether this watch folder is enabled
    pub enabled: bool,
}

/// All engine settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EngineSettings {
    /// Download directory
    pub download_dir: PathBuf,
    /// Categories (name -> category)
    pub categories: HashMap<String, Category>,
    /// Auto-tracker settings
    pub auto_trackers: AutoTrackerSettings,
    /// Move-on-completion settings
    pub move_on_complete: MoveOnCompleteSettings,
    /// External program settings
    pub external_program: ExternalProgramSettings,
    /// Watch folders
    pub watch_folders: Vec<WatchFolder>,
    /// Default share limits for new torrents
    pub default_share_limits: ShareLimits,
}
