// Shared TypeScript interfaces for the application

export interface FileInfo {
  path: string;
  size: number;
}

export interface TorrentInfo {
  name: string;
  info_hash: string;
  version: string;
  total_size: number;
  piece_count: number;
  piece_length: number;
  file_count: number;
  files: FileInfo[];
  trackers: string[];
  is_private: boolean;
  comment: string | null;
  created_by: string | null;
  creation_date: number | null;
}

export interface MagnetInfo {
  info_hash: string;
  display_name: string | null;
  trackers: string[];
}

export interface PendingTorrent {
  info: TorrentInfo;
  data: number[];
}

// All possible torrent states (qBittorrent compatible)
export type TorrentState =
  | 'downloading'
  | 'forcedDL'
  | 'uploading'
  | 'forcedUP'
  | 'stalledDL'
  | 'stalledUP'
  | 'completed'
  | 'pausedDL'
  | 'pausedUP'
  | 'stoppedDL'
  | 'stoppedUP'
  | 'checkingDL'
  | 'checkingUP'
  | 'checkingResumeData'
  | 'queuedDL'
  | 'queuedUP'
  | 'metaDL'
  | 'forcedMetaDL'
  | 'allocating'
  | 'moving'
  | 'missingFiles'
  | 'error'
  | 'unknown';

export interface TorrentStatus {
  info_hash: string;
  name: string;
  state: TorrentState;
  progress: number;
  download_rate: number;
  upload_rate: number;
  downloaded: number;
  uploaded: number;
  total_size: number;
  peers: number;
  seeds: number;
}

export interface TrackerStatusInfo {
  url: string;
  status: string;
  peers: number;
  seeds: number;
  leechers: number;
  last_announce: number | null;
  next_announce: number | null;
  message: string | null;
}

export interface PeerStatusInfo {
  address: string;
  download_bytes: number;
  upload_bytes: number;
  is_choking_us: boolean;
  is_interested: boolean;
  progress: number;
}

export interface TorrentFileInfo {
  path: string;
  size: number;
  progress: number;
  downloaded: number;
}

export interface GlobalStats {
  download_rate: number;
  upload_rate: number;
  total_downloaded: number;
  total_uploaded: number;
  active_torrents: number;
  total_peers: number;
  global_connections: number;
}

export type FilterType =
  | "all"
  | "downloading"
  | "seeding"
  | "completed"
  | "paused"
  | "queued"
  | "checking"
  | "stalled"
  | "stalledDL"
  | "stalledUP"
  | "active";

export type DetailTab = "general" | "trackers" | "peers" | "files";

// RSS Types
export interface RssFeedInfo {
  id: string;
  url: string;
  name: string;
  enabled: boolean;
  refresh_interval: number;
  last_refresh: number | null;
  last_error: string | null;
}

export interface RssRuleInfo {
  id: string;
  name: string;
  enabled: boolean;
  must_contain: string;
  must_not_contain: string;
  use_regex: boolean;
  episode_filter: string | null;
  affected_feeds: string[];
  category: string | null;
  tags: string[];
  save_path: string | null;
  add_paused: boolean;
  last_match: number | null;
}

export interface RssItemInfo {
  title: string;
  torrent_url: string;
  link: string | null;
  pub_date: number | null;
  description: string | null;
  is_downloaded: boolean;
}

// Search Types
export interface SearchPluginInfo {
  name: string;
  display_name: string;
  version: string;
  enabled: boolean;
  categories: string[];
  url: string | null;
}

export interface SearchResultInfo {
  name: string;
  download_link: string;
  size: number;
  seeders: number;
  leechers: number;
  plugin: string;
  description_link: string | null;
  pub_date: number | null;
}

export interface SearchJobInfo {
  id: string;
  query: string;
  plugins: string[];
  category: string | null;
  status: string;
  results_count: number;
  error: string | null;
}

// Watch Folder Types
export interface WatchFolderInfo {
  id: string;
  path: string;
  category: string | null;
  tags: string[];
  process_existing: boolean;
  enabled: boolean;
}

// Category Types
export interface CategoryInfo {
  name: string;
  save_path: string;
}

// Share Limits Types
export interface ShareLimitsInfo {
  max_ratio: number | null;
  max_seeding_time: number | null;
  limit_action: string;
}

// Auto Tracker Settings
export interface AutoTrackerSettingsInfo {
  enabled: boolean;
  trackers: string[];
}

// Move on Complete Settings
export interface MoveOnCompleteSettingsInfo {
  enabled: boolean;
  target_path: string | null;
  use_category_path: boolean;
}

// External Program Settings
export interface ExternalProgramSettingsInfo {
  on_completion_enabled: boolean;
  on_completion_command: string | null;
}

// Theme Settings
export type ThemeMode = "light" | "dark" | "system";
