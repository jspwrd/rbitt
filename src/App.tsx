import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import "./App.css";

const Icons = {
  Add: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
      <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z" />
    </svg>
  ),
  Pause: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
      <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
    </svg>
  ),
  Play: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
      <path d="M8 5v14l11-7z" />
    </svg>
  ),
  Delete: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
      <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" />
    </svg>
  ),
  Settings: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
      <path d="M19.14 12.94c.04-.31.06-.63.06-.94 0-.31-.02-.63-.06-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.04.31-.06.63-.06.94s.02.63.06.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z" />
    </svg>
  ),
  Download: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z" />
    </svg>
  ),
  Upload: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M9 16h6v-6h4l-7-7-7 7h4v6zm-4 2h14v2H5v-2z" />
    </svg>
  ),
  Folder: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
      <path d="M10 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z" />
    </svg>
  ),
  Link: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="18" height="18">
      <path d="M3.9 12c0-1.71 1.39-3.1 3.1-3.1h4V7H7c-2.76 0-5 2.24-5 5s2.24 5 5 5h4v-1.9H7c-1.71 0-3.1-1.39-3.1-3.1zM8 13h8v-2H8v2zm9-6h-4v1.9h4c1.71 0 3.1 1.39 3.1 3.1s-1.39 3.1-3.1 3.1h-4V17h4c2.76 0 5-2.24 5-5s-2.24-5-5-5z" />
    </svg>
  ),
  All: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M4 8h4V4H4v4zm6 12h4v-4h-4v4zm-6 0h4v-4H4v4zm0-6h4v-4H4v4zm6 0h4v-4h-4v4zm6-10v4h4V4h-4zm-6 4h4V4h-4v4zm6 6h4v-4h-4v4zm0 6h4v-4h-4v4z" />
    </svg>
  ),
  Downloading: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z" />
    </svg>
  ),
  Seeding: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M9 16h6v-6h4l-7-7-7 7h4v6zm-4 2h14v2H5v-2z" />
    </svg>
  ),
  Paused: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
    </svg>
  ),
  Checking: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M12 4V1L8 5l4 4V6c3.31 0 6 2.69 6 6 0 1.01-.25 1.97-.7 2.8l1.46 1.46C19.54 15.03 20 13.57 20 12c0-4.42-3.58-8-8-8zm0 14c-3.31 0-6-2.69-6-6 0-1.01.25-1.97.7-2.8L5.24 7.74C4.46 8.97 4 10.43 4 12c0 4.42 3.58 8 8 8v3l4-4-4-4v3z" />
    </svg>
  ),
  Completed: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" />
    </svg>
  ),
  Peers: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5c-1.66 0-3 1.34-3 3s1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5C6.34 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z" />
    </svg>
  ),
  Stalled: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z" />
    </svg>
  ),
  Error: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z" />
    </svg>
  ),
  Queued: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M4 6H2v14c0 1.1.9 2 2 2h14v-2H4V6zm16-4H8c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm-1 9h-4v4h-2v-4H9V9h4V5h2v4h4v2z" />
    </svg>
  ),
  Stopped: () => (
    <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
      <path d="M6 6h12v12H6z" />
    </svg>
  ),
};

interface FileInfo {
  path: string;
  size: number;
}

interface TorrentInfo {
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

interface MagnetInfo {
  info_hash: string;
  display_name: string | null;
  trackers: string[];
}

interface PendingTorrent {
  info: TorrentInfo;
  data: number[];
}

interface TorrentStatus {
  info_hash: string;
  name: string;
  state: string;  // qBittorrent-compatible: downloading, uploading, stalledDL, stalledUP, pausedDL, pausedUP, etc.
  progress: number;
  download_rate: number;
  upload_rate: number;
  downloaded: number;
  uploaded: number;
  total_size: number;
  peers: number;
  seeds: number;
}

function isDownloading(state: string): boolean {
  return [
    "downloading",
    "forcedDL",
    "metaDL",
    "forcedMetaDL",
    "checkingDL",
    "pausedDL",
    "stoppedDL",
    "allocating",
    "checkingResumeData",
  ].includes(state);
}

function isUploading(state: string): boolean {
  return [
    "uploading",
    "forcedUP",
    "checkingUP",
    "pausedUP",
    "stoppedUP",
  ].includes(state);
}

function isStalledDownloading(state: string): boolean {
  return state === "stalledDL";
}

function isStalledUploading(state: string): boolean {
  return state === "stalledUP";
}

function isStalled(state: string): boolean {
  return state === "stalledDL" || state === "stalledUP";
}

function isPaused(state: string): boolean {
  return ["pausedDL", "pausedUP", "stoppedDL", "stoppedUP"].includes(state);
}

function isChecking(state: string): boolean {
  return ["checkingDL", "checkingUP", "checkingResumeData"].includes(state);
}

function isActive(state: string): boolean {
  return ["downloading", "uploading", "forcedDL", "forcedUP", "metaDL", "forcedMetaDL"].includes(state);
}

function isError(state: string): boolean {
  return state === "error" || state === "missingFiles";
}

function isCompleted(state: string): boolean {
  return state === "completed";
}

function isQueued(state: string): boolean {
  return state === "queuedDL" || state === "queuedUP";
}

interface TrackerStatusInfo {
  url: string;
  status: string;
  peers: number;
  seeds: number;
  leechers: number;
  last_announce: number | null;
  next_announce: number | null;
  message: string | null;
}

interface PeerStatusInfo {
  address: string;
  download_bytes: number;
  upload_bytes: number;
  is_choking_us: boolean;
  is_interested: boolean;
  progress: number;
}

interface TorrentFileInfo {
  path: string;
  size: number;
  progress: number;
  downloaded: number;
}

interface GlobalStats {
  download_rate: number;
  upload_rate: number;
  total_downloaded: number;
  total_uploaded: number;
  active_torrents: number;
  total_peers: number;
  global_connections: number;
}

type FilterType = "all" | "downloading" | "seeding" | "paused" | "checking" | "stalled" | "stalledDL" | "stalledUP" | "active";
type DetailTab = "general" | "trackers" | "peers" | "files";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KiB", "MiB", "GiB", "TiB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

function formatSpeed(bytesPerSecond: number): string {
  return formatBytes(bytesPerSecond) + "/s";
}

function formatDate(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString();
}


function formatEta(downloaded: number, total: number, rate: number): string {
  if (rate === 0) return "∞";
  const remaining = total - downloaded;
  const seconds = remaining / rate;
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)}h`;
  return `${Math.round(seconds / 86400)}d`;
}

function App() {
  const [engineInitialized, setEngineInitialized] = useState(false);
  const [downloadDir, setDownloadDir] = useState("");
  const [torrents, setTorrents] = useState<TorrentStatus[]>([]);
  const [selectedTorrent, setSelectedTorrent] = useState<string | null>(null);
  const [filter, setFilter] = useState<FilterType>("all");
  const [detailTab, setDetailTab] = useState<DetailTab>("general");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [initializing, setInitializing] = useState(true);

  const [showAddModal, setShowAddModal] = useState(false);
  const [pendingTorrents, setPendingTorrents] = useState<PendingTorrent[]>([]);
  const [selectedPendingIndex, setSelectedPendingIndex] = useState<number>(0);
  const [magnetUri, setMagnetUri] = useState("");
  const [magnetInfo, setMagnetInfo] = useState<MagnetInfo | null>(null);

  const [showSettings, setShowSettings] = useState(false);
  const [maxDownloadSpeed, setMaxDownloadSpeed] = useState(0);
  const [maxUploadSpeed, setMaxUploadSpeed] = useState(0);
  const [maxActiveDownloads, setMaxActiveDownloads] = useState(5);
  const [maxActiveUploads, setMaxActiveUploads] = useState(5);
  const [askDownloadLocation, setAskDownloadLocation] = useState(false);
  const [tempDownloadDir, setTempDownloadDir] = useState("");
  const [noSeedMode, setNoSeedMode] = useState(false);
  const [disconnectOnComplete, setDisconnectOnComplete] = useState(false);

  const [trackerInfo, setTrackerInfo] = useState<TrackerStatusInfo[]>([]);
  const [peerInfo, setPeerInfo] = useState<PeerStatusInfo[]>([]);
  const [fileInfo, setFileInfo] = useState<TorrentFileInfo[]>([]);
  const [globalStatsData, setGlobalStatsData] = useState<GlobalStats | null>(null);

  useEffect(() => {
    async function autoInit() {
      try {
        const defaultDir: string = await invoke("get_default_download_dir");
        setDownloadDir(defaultDir);
        setTempDownloadDir(defaultDir);

        await invoke("init_engine", { downloadDir: defaultDir });
        setEngineInitialized(true);
      } catch (e) {
        console.error("Failed to auto-initialize:", e);
      } finally {
        setInitializing(false);
      }
    }
    autoInit();
  }, []);

  const refreshTorrents = useCallback(async () => {
    if (!engineInitialized) return;
    try {
      const status: TorrentStatus[] = await invoke("get_torrents");
      setTorrents(status);
    } catch (e) {
      console.error("Failed to get torrents:", e);
    }
  }, [engineInitialized]);

  useEffect(() => {
    if (engineInitialized) {
      refreshTorrents();
      const interval = setInterval(refreshTorrents, 1000);
      return () => clearInterval(interval);
    }
  }, [engineInitialized, refreshTorrents]);

  useEffect(() => {
    async function fetchTrackerInfo() {
      if (!selectedTorrent || !engineInitialized) {
        setTrackerInfo([]);
        return;
      }
      try {
        const info: TrackerStatusInfo[] = await invoke("get_torrent_trackers", {
          infoHash: selectedTorrent,
        });
        setTrackerInfo(info);
      } catch (e) {
        console.error("Failed to get tracker info:", e);
        setTrackerInfo([]);
      }
    }
    fetchTrackerInfo();
    const interval = setInterval(fetchTrackerInfo, 5000);
    return () => clearInterval(interval);
  }, [selectedTorrent, engineInitialized]);

  useEffect(() => {
    async function fetchPeerInfo() {
      if (!selectedTorrent || !engineInitialized) {
        setPeerInfo([]);
        return;
      }
      try {
        const info: PeerStatusInfo[] = await invoke("get_torrent_peers", {
          infoHash: selectedTorrent,
        });
        setPeerInfo(info);
      } catch (e) {
        console.error("Failed to get peer info:", e);
        setPeerInfo([]);
      }
    }
    fetchPeerInfo();
    const interval = setInterval(fetchPeerInfo, 2000);
    return () => clearInterval(interval);
  }, [selectedTorrent, engineInitialized]);

  useEffect(() => {
    async function fetchFileInfo() {
      if (!selectedTorrent || !engineInitialized) {
        setFileInfo([]);
        return;
      }
      try {
        const info: TorrentFileInfo[] = await invoke("get_torrent_files", {
          infoHash: selectedTorrent,
        });
        setFileInfo(info);
      } catch (e) {
        console.error("Failed to get file info:", e);
        setFileInfo([]);
      }
    }
    fetchFileInfo();
    const interval = setInterval(fetchFileInfo, 2000);
    return () => clearInterval(interval);
  }, [selectedTorrent, engineInitialized]);

  useEffect(() => {
    async function fetchGlobalStats() {
      if (!engineInitialized) return;
      try {
        const stats: GlobalStats = await invoke("get_global_stats");
        setGlobalStatsData(stats);
      } catch (e) {
        console.error("Failed to get global stats:", e);
      }
    }
    fetchGlobalStats();
    const interval = setInterval(fetchGlobalStats, 1000);
    return () => clearInterval(interval);
  }, [engineInitialized]);

  useEffect(() => {
    if (engineInitialized) {
      loadQueueSettings();
      loadNoSeedMode();
      loadDisconnectOnComplete();
    }
  }, [engineInitialized]);

  const filteredTorrents = useMemo(() => {
    if (filter === "all") return torrents;
    switch (filter) {
      case "downloading":
        return torrents.filter((t) => isDownloading(t.state));
      case "seeding":
        return torrents.filter((t) => isUploading(t.state) && !isCompleted(t.state));
      case "completed":
        return torrents.filter((t) => isCompleted(t.state));
      case "paused":
        return torrents.filter((t) => isPaused(t.state) && !isCompleted(t.state));
      case "queued":
        return torrents.filter((t) => isQueued(t.state));
      case "checking":
        return torrents.filter((t) => isChecking(t.state));
      case "stalled":
        return torrents.filter((t) => isStalled(t.state));
      case "stalledDL":
        return torrents.filter((t) => isStalledDownloading(t.state));
      case "stalledUP":
        return torrents.filter((t) => isStalledUploading(t.state));
      case "active":
        return torrents.filter((t) => isActive(t.state));
      default:
        return torrents;
    }
  }, [torrents, filter]);

  const selectedTorrentData = useMemo(() => {
    return torrents.find((t) => t.info_hash === selectedTorrent);
  }, [torrents, selectedTorrent]);

  const globalStats = useMemo(() => {
    return {
      totalDownload: torrents.reduce((sum, t) => sum + t.download_rate, 0),
      totalUpload: torrents.reduce((sum, t) => sum + t.upload_rate, 0),
      downloading: torrents.filter((t) => isDownloading(t.state)).length,
      seeding: torrents.filter((t) => isUploading(t.state)).length,
      active: torrents.filter((t) => isActive(t.state)).length,
      stalled: torrents.filter((t) => isStalled(t.state)).length,
      total: torrents.length,
    };
  }, [torrents]);

  async function selectDownloadDir() {
    try {
      const selected = await open({
        directory: true,
        title: "Select Download Directory",
      });
      if (selected) {
        setDownloadDir(selected);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function initEngine() {
    if (!downloadDir) {
      setError("Please select a download directory");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      await invoke("init_engine", { downloadDir });
      setEngineInitialized(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function openTorrentFile() {
    try {
      const selected = await open({
        multiple: true,
        filters: [{ name: "Torrent Files", extensions: ["torrent"] }],
        title: "Select Torrent Files",
      });

      if (selected) {
        setLoading(true);
        setError(null);
        try {
          const files = Array.isArray(selected) ? selected : [selected];
          const newPendingTorrents: PendingTorrent[] = [];

          for (const filePath of files) {
            try {
              const fileData = await readFile(filePath);
              const data: number[] = Array.from(fileData);
              const info: TorrentInfo = await invoke("parse_torrent_bytes", { data });
              newPendingTorrents.push({ info, data });
            } catch (err) {
              console.error(`Failed to parse ${filePath}:`, err);
            }
          }

          if (newPendingTorrents.length > 0) {
            setPendingTorrents(newPendingTorrents);
            setSelectedPendingIndex(0);
            setShowAddModal(true);
          } else {
            setError("Failed to parse any of the selected torrent files");
          }
        } catch (err) {
          setError(String(err));
        } finally {
          setLoading(false);
        }
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function parseMagnet() {
    if (!magnetUri) return;
    setLoading(true);
    setError(null);
    try {
      const info: MagnetInfo = await invoke("parse_magnet", { uri: magnetUri });
      setMagnetInfo(info);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  async function addTorrentFromFile() {
    if (pendingTorrents.length === 0) return;
    setLoading(true);
    setError(null);
    try {
      for (const torrent of pendingTorrents) {
        await invoke("add_torrent_bytes", { data: torrent.data });
      }
      closeAddModal();
      refreshTorrents();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  async function addTorrentFromMagnet() {
    if (!magnetUri) return;
    setLoading(true);
    setError(null);
    try {
      await invoke("add_magnet", { uri: magnetUri });
      closeAddModal();
      refreshTorrents();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  function closeAddModal() {
    setShowAddModal(false);
    setPendingTorrents([]);
    setSelectedPendingIndex(0);
    setMagnetUri("");
    setMagnetInfo(null);
  }

  async function pauseTorrent(infoHash: string) {
    try {
      await invoke("pause_torrent", { infoHash });
      refreshTorrents();
    } catch (err) {
      setError(String(err));
    }
  }

  async function resumeTorrent(infoHash: string) {
    try {
      await invoke("resume_torrent", { infoHash });
      refreshTorrents();
    } catch (err) {
      setError(String(err));
    }
  }

  async function removeTorrent(infoHash: string, deleteFiles: boolean) {
    try {
      await invoke("remove_torrent", { infoHash, deleteFiles });
      if (selectedTorrent === infoHash) {
        setSelectedTorrent(null);
      }
      refreshTorrents();
    } catch (err) {
      setError(String(err));
    }
  }

  async function applyBandwidthLimits() {
    try {
      const downloadLimit = maxDownloadSpeed > 0 ? maxDownloadSpeed * 1024 : 0;
      const uploadLimit = maxUploadSpeed > 0 ? maxUploadSpeed * 1024 : 0;
      await invoke("set_bandwidth_limits", { downloadLimit, uploadLimit });
    } catch (err) {
      setError(String(err));
    }
  }

  async function applyQueueSettings() {
    try {
      await invoke("set_queue_settings", {
        maxDownloads: maxActiveDownloads,
        maxUploads: maxActiveUploads,
      });
    } catch (err) {
      setError(String(err));
    }
  }

  async function loadQueueSettings() {
    try {
      const settings: [number, number] = await invoke("get_queue_settings");
      setMaxActiveDownloads(settings[0]);
      setMaxActiveUploads(settings[1]);
    } catch (err) {
      console.error("Failed to load queue settings:", err);
    }
  }

  async function loadNoSeedMode() {
    try {
      const enabled: boolean = await invoke("get_no_seed_mode");
      setNoSeedMode(enabled);
    } catch (err) {
      console.error("Failed to load no-seed mode:", err);
    }
  }

  async function toggleNoSeedMode(enabled: boolean) {
    try {
      await invoke("set_no_seed_mode", { enabled });
      setNoSeedMode(enabled);
    } catch (err) {
      setError(String(err));
    }
  }

  async function loadDisconnectOnComplete() {
    try {
      const enabled: boolean = await invoke("get_disconnect_on_complete");
      setDisconnectOnComplete(enabled);
    } catch (err) {
      console.error("Failed to load disconnect on complete:", err);
    }
  }

  async function toggleDisconnectOnComplete(enabled: boolean) {
    try {
      await invoke("set_disconnect_on_complete", { enabled });
      setDisconnectOnComplete(enabled);
    } catch (err) {
      setError(String(err));
    }
  }

  function getStateColor(state: string): string {
    switch (state) {
      case "downloading":
      case "forcedDL":
        return "var(--state-downloading)";
      case "uploading":
      case "forcedUP":
        return "var(--state-seeding)";
      case "stalledDL":
        return "var(--state-stalled)";
      case "stalledUP":
        return "var(--state-stalled)";
      case "completed":
        return "var(--state-completed)";
      case "pausedDL":
      case "pausedUP":
      case "stoppedDL":
      case "stoppedUP":
        return "var(--state-paused)";
      case "checkingDL":
      case "checkingUP":
      case "checkingResumeData":
        return "var(--state-checking)";
      case "queuedDL":
      case "queuedUP":
        return "var(--state-queued)";
      case "metaDL":
      case "forcedMetaDL":
        return "var(--state-metadata)";
      case "allocating":
        return "var(--state-checking)";
      case "moving":
        return "var(--state-moving)";
      case "missingFiles":
      case "error":
        return "var(--state-error)";
      default:
        return "var(--text-secondary)";
    }
  }

  function getStateIcon(state: string) {
    if (state === "error" || state === "missingFiles") {
      return <Icons.Error />;
    }
    if (isChecking(state)) {
      return <Icons.Checking />;
    }
    if (isQueued(state)) {
      return <Icons.Queued />;
    }
    if (isCompleted(state)) {
      return <Icons.Completed />;
    }
    if (isPaused(state)) {
      return <Icons.Stopped />;
    }
    if (state === "stalledDL") {
      return <Icons.Downloading />;
    }
    if (state === "stalledUP") {
      return <Icons.Seeding />;
    }
    if (state === "metaDL" || state === "forcedMetaDL") {
      return <Icons.Downloading />;
    }
    if (state === "uploading" || state === "forcedUP") {
      return <Icons.Seeding />;
    }
    if (state === "downloading" || state === "forcedDL") {
      return <Icons.Downloading />;
    }
    return <Icons.Downloading />;
  }

  function formatState(state: string): string {
    const stateLabels: Record<string, string> = {
      downloading: "Downloading",
      uploading: "Seeding",
      stalledDL: "Stalled",
      stalledUP: "Stalled",
      completed: "Completed",
      pausedDL: "Paused",
      pausedUP: "Paused",
      stoppedDL: "Paused",
      stoppedUP: "Paused",
      checkingDL: "Checking",
      checkingUP: "Checking",
      checkingResumeData: "Checking",
      queuedDL: "Queued",
      queuedUP: "Queued",
      metaDL: "Fetching metadata",
      forcedDL: "[F] Downloading",
      forcedUP: "[F] Seeding",
      forcedMetaDL: "[F] Fetching metadata",
      moving: "Moving",
      missingFiles: "Missing files",
      error: "Error",
      allocating: "Allocating",
      unknown: "Unknown",
    };
    return stateLabels[state] || state;
  }

  if (initializing) {
    return (
      <div className="setup-screen">
        <div className="setup-content">
          <h1 className="setup-title">RBitt</h1>
          <p className="setup-subtitle">A Modern BitTorrent Client</p>
          <p className="setup-loading">Starting...</p>
        </div>
      </div>
    );
  }

  if (!engineInitialized) {
    return (
      <div className="setup-screen">
        <div className="setup-content">
          <h1 className="setup-title">RBitt</h1>
          <p className="setup-subtitle">A Modern BitTorrent Client</p>

          <div className="setup-form">
            <label>Download Directory</label>
            <div className="dir-select">
              <input
                type="text"
                value={downloadDir}
                onChange={(e) => setDownloadDir(e.target.value)}
                placeholder="Select a download directory..."
                readOnly
              />
              <button onClick={selectDownloadDir} className="btn-secondary">
                <Icons.Folder /> Browse
              </button>
            </div>

            <button onClick={initEngine} disabled={loading || !downloadDir} className="btn-primary btn-large">
              {loading ? "Initializing..." : "Start RBitt"}
            </button>
          </div>

          {error && <div className="setup-error">{error}</div>}
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      {}
      <div className="toolbar">
        <div className="toolbar-group">
          <button className="toolbar-btn" onClick={() => setShowAddModal(true)} title="Add Torrent">
            <Icons.Add />
            <span>Add</span>
          </button>
          <div className="toolbar-separator" />
          <button
            className="toolbar-btn"
            onClick={() => selectedTorrent && resumeTorrent(selectedTorrent)}
            disabled={!selectedTorrent || !selectedTorrentData || !isPaused(selectedTorrentData.state)}
            title="Resume"
          >
            <Icons.Play />
          </button>
          <button
            className="toolbar-btn"
            onClick={() => selectedTorrent && pauseTorrent(selectedTorrent)}
            disabled={!selectedTorrent || !selectedTorrentData || isPaused(selectedTorrentData.state)}
            title="Pause"
          >
            <Icons.Pause />
          </button>
          <button
            className="toolbar-btn danger"
            onClick={() => selectedTorrent && removeTorrent(selectedTorrent, false)}
            disabled={!selectedTorrent}
            title="Remove"
          >
            <Icons.Delete />
          </button>
        </div>
        <div className="toolbar-group">
          <button className="toolbar-btn" onClick={() => setShowSettings(true)} title="Settings">
            <Icons.Settings />
          </button>
        </div>
      </div>

      {}
      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      {}
      <div className="main-content">
        {}
        <div className="sidebar">
          <div className="sidebar-section">
            <div className="sidebar-header">Status</div>
            <button
              className={`sidebar-item ${filter === "all" ? "active" : ""}`}
              onClick={() => setFilter("all")}
            >
              <Icons.All />
              <span>All</span>
              <span className="sidebar-count">{torrents.length}</span>
            </button>
            <button
              className={`sidebar-item ${filter === "downloading" ? "active" : ""}`}
              onClick={() => setFilter("downloading")}
            >
              <Icons.Downloading />
              <span>Downloading</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isDownloading(t.state)).length}
              </span>
            </button>
            <button
              className={`sidebar-item ${filter === "seeding" ? "active" : ""}`}
              onClick={() => setFilter("seeding")}
            >
              <Icons.Seeding />
              <span>Seeding</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isUploading(t.state) && !isCompleted(t.state)).length}
              </span>
            </button>
            <button
              className={`sidebar-item ${filter === "completed" ? "active" : ""}`}
              onClick={() => setFilter("completed")}
            >
              <Icons.Completed />
              <span>Completed</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isCompleted(t.state)).length}
              </span>
            </button>
            <button
              className={`sidebar-item ${filter === "paused" ? "active" : ""}`}
              onClick={() => setFilter("paused")}
            >
              <Icons.Paused />
              <span>Paused</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isPaused(t.state) && !isCompleted(t.state)).length}
              </span>
            </button>
            <button
              className={`sidebar-item ${filter === "queued" ? "active" : ""}`}
              onClick={() => setFilter("queued")}
            >
              <Icons.Queued />
              <span>Queued</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isQueued(t.state)).length}
              </span>
            </button>
            <button
              className={`sidebar-item ${filter === "checking" ? "active" : ""}`}
              onClick={() => setFilter("checking")}
            >
              <Icons.Checking />
              <span>Checking</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isChecking(t.state)).length}
              </span>
            </button>
            <button
              className={`sidebar-item ${filter === "stalledDL" ? "active" : ""}`}
              onClick={() => setFilter("stalledDL")}
            >
              <Icons.Downloading />
              <span>Stalled Downloading</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isStalledDownloading(t.state)).length}
              </span>
            </button>
            <button
              className={`sidebar-item ${filter === "stalledUP" ? "active" : ""}`}
              onClick={() => setFilter("stalledUP")}
            >
              <Icons.Seeding />
              <span>Stalled Uploading</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isStalledUploading(t.state)).length}
              </span>
            </button>
            <button
              className={`sidebar-item ${filter === "active" ? "active" : ""}`}
              onClick={() => setFilter("active")}
            >
              <Icons.Downloading />
              <span>Active</span>
              <span className="sidebar-count">
                {torrents.filter((t) => isActive(t.state)).length}
              </span>
            </button>
          </div>
        </div>

        {}
        <div className="content-area">
          {}
          <div className="torrent-list-container">
            <table className="torrent-table">
              <thead>
                <tr>
                  <th className="col-name">Name</th>
                  <th className="col-size">Size</th>
                  <th className="col-progress">Progress</th>
                  <th className="col-status">Status</th>
                  <th className="col-seeds">Seeds</th>
                  <th className="col-peers">Peers</th>
                  <th className="col-down">Down Speed</th>
                  <th className="col-up">Up Speed</th>
                  <th className="col-eta">ETA</th>
                </tr>
              </thead>
              <tbody>
                {filteredTorrents.length === 0 ? (
                  <tr className="empty-row">
                    <td colSpan={9}>
                      <div className="empty-state">
                        <p>No torrents</p>
                        <button className="btn-primary" onClick={() => setShowAddModal(true)}>
                          Add your first torrent
                        </button>
                      </div>
                    </td>
                  </tr>
                ) : (
                  filteredTorrents.map((torrent) => (
                    <tr
                      key={torrent.info_hash}
                      className={selectedTorrent === torrent.info_hash ? "selected" : ""}
                      onClick={() => setSelectedTorrent(torrent.info_hash)}
                      onDoubleClick={() => {
                        if (isPaused(torrent.state)) {
                          resumeTorrent(torrent.info_hash);
                        } else {
                          pauseTorrent(torrent.info_hash);
                        }
                      }}
                    >
                      <td className="col-name">
                        <div className="torrent-name-cell">
                          <span
                            className={`state-icon ${isChecking(torrent.state) ? "spinning" : ""}`}
                            style={{ color: getStateColor(torrent.state) }}
                          >
                            {getStateIcon(torrent.state)}
                          </span>
                          <span className="torrent-name" title={torrent.name}>
                            {torrent.name}
                          </span>
                        </div>
                      </td>
                      <td className="col-size">{formatBytes(torrent.total_size)}</td>
                      <td className="col-progress">
                        <div className="progress-cell">
                          <div className="progress-bar-container">
                            <div
                              className="progress-bar"
                              style={{
                                width: `${torrent.progress}%`,
                                backgroundColor: getStateColor(torrent.state),
                              }}
                            />
                          </div>
                          <span className="progress-text">{torrent.progress.toFixed(1)}%</span>
                        </div>
                      </td>
                      <td className="col-status" style={{ color: getStateColor(torrent.state) }}>
                        {formatState(torrent.state)}
                      </td>
                      <td className="col-seeds">{torrent.seeds}</td>
                      <td className="col-peers">{torrent.peers}</td>
                      <td className="col-down">
                        {torrent.download_rate > 0 ? formatSpeed(torrent.download_rate) : "-"}
                      </td>
                      <td className="col-up">
                        {torrent.upload_rate > 0 ? formatSpeed(torrent.upload_rate) : "-"}
                      </td>
                      <td className="col-eta">
                        {isDownloading(torrent.state) && torrent.download_rate > 0 && torrent.total_size > 0
                          ? formatEta(torrent.downloaded, torrent.total_size, torrent.download_rate)
                          : "-"}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>

          {}
          {selectedTorrentData && (
            <div className="detail-panel">
              <div className="detail-tabs">
                <button
                  className={detailTab === "general" ? "active" : ""}
                  onClick={() => setDetailTab("general")}
                >
                  General
                </button>
                <button
                  className={detailTab === "trackers" ? "active" : ""}
                  onClick={() => setDetailTab("trackers")}
                >
                  Trackers
                </button>
                <button
                  className={detailTab === "peers" ? "active" : ""}
                  onClick={() => setDetailTab("peers")}
                >
                  Peers
                </button>
                <button
                  className={detailTab === "files" ? "active" : ""}
                  onClick={() => setDetailTab("files")}
                >
                  Files
                </button>
              </div>
              <div className="detail-content">
                {detailTab === "general" && (
                  <div className="detail-general">
                    <div className="detail-grid">
                      <div className="detail-item">
                        <span className="detail-label">Name</span>
                        <span className="detail-value">{selectedTorrentData.name}</span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Info Hash</span>
                        <span className="detail-value monospace">{selectedTorrentData.info_hash}</span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Status</span>
                        <span className="detail-value" style={{ color: getStateColor(selectedTorrentData.state) }}>
                          {formatState(selectedTorrentData.state)}
                        </span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Total Size</span>
                        <span className="detail-value">{formatBytes(selectedTorrentData.total_size)}</span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Progress</span>
                        <span className="detail-value">
                          {selectedTorrentData.progress.toFixed(2)}% ({formatBytes(selectedTorrentData.downloaded)} / {formatBytes(selectedTorrentData.total_size)})
                        </span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Uploaded</span>
                        <span className="detail-value">{formatBytes(selectedTorrentData.uploaded)}</span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Download Speed</span>
                        <span className="detail-value">{formatSpeed(selectedTorrentData.download_rate)}</span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Upload Speed</span>
                        <span className="detail-value">{formatSpeed(selectedTorrentData.upload_rate)}</span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Seeds</span>
                        <span className="detail-value">{selectedTorrentData.seeds}</span>
                      </div>
                      <div className="detail-item">
                        <span className="detail-label">Peers</span>
                        <span className="detail-value">{selectedTorrentData.peers}</span>
                      </div>
                    </div>
                  </div>
                )}
                {detailTab === "trackers" && (
                  <div className="detail-trackers">
                    {trackerInfo.length === 0 ? (
                      <p className="detail-placeholder">No trackers available.</p>
                    ) : (
                      <table className="tracker-table">
                        <thead>
                          <tr>
                            <th>URL</th>
                            <th>Status</th>
                            <th>Seeds</th>
                            <th>Leechers</th>
                            <th>Peers</th>
                            <th>Last Announce</th>
                            <th>Next Announce</th>
                            <th>Message</th>
                          </tr>
                        </thead>
                        <tbody>
                          {trackerInfo.map((tracker, idx) => (
                            <tr key={idx}>
                              <td className="tracker-url" title={tracker.url}>
                                {tracker.url}
                              </td>
                              <td className={`tracker-status status-${tracker.status.toLowerCase().replace(/[^a-z]/g, '-')}`}>
                                {tracker.status}
                              </td>
                              <td>{tracker.seeds}</td>
                              <td>{tracker.leechers}</td>
                              <td>{tracker.peers}</td>
                              <td>
                                {tracker.last_announce !== null
                                  ? `${tracker.last_announce}s ago`
                                  : "-"}
                              </td>
                              <td>
                                {tracker.next_announce !== null
                                  ? `in ${tracker.next_announce}s`
                                  : "-"}
                              </td>
                              <td className="tracker-message" title={tracker.message || undefined}>
                                {tracker.message || "-"}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    )}
                  </div>
                )}
                {detailTab === "peers" && (
                  <div className="detail-peers">
                    <div className="peers-summary">
                      <span>Connected to {peerInfo.length} peers ({selectedTorrentData.seeds} seeds)</span>
                    </div>
                    {peerInfo.length === 0 ? (
                      <p className="detail-placeholder">No peers connected.</p>
                    ) : (
                      <table className="peer-table">
                        <thead>
                          <tr>
                            <th>Address</th>
                            <th>Progress</th>
                            <th>Downloaded</th>
                            <th>Uploaded</th>
                            <th>Status</th>
                          </tr>
                        </thead>
                        <tbody>
                          {peerInfo.map((peer, idx) => (
                            <tr key={idx}>
                              <td className="peer-address monospace">{peer.address}</td>
                              <td>
                                <div className="progress-cell">
                                  <div className="progress-bar-container small">
                                    <div
                                      className="progress-bar"
                                      style={{
                                        width: `${peer.progress}%`,
                                        backgroundColor: peer.progress >= 100 ? "var(--state-seeding)" : "var(--state-downloading)",
                                      }}
                                    />
                                  </div>
                                  <span className="progress-text">{peer.progress.toFixed(1)}%</span>
                                </div>
                              </td>
                              <td>{formatBytes(peer.download_bytes)}</td>
                              <td>{formatBytes(peer.upload_bytes)}</td>
                              <td className="peer-status">
                                {peer.is_choking_us ? "Choked" : "Unchoked"}
                                {peer.is_interested ? " / Interested" : ""}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    )}
                  </div>
                )}
                {detailTab === "files" && (
                  <div className="detail-files">
                    {fileInfo.length === 0 ? (
                      <p className="detail-placeholder">No files available.</p>
                    ) : (
                      <table className="file-table">
                        <thead>
                          <tr>
                            <th>Name</th>
                            <th>Size</th>
                            <th>Progress</th>
                            <th>Downloaded</th>
                          </tr>
                        </thead>
                        <tbody>
                          {fileInfo.map((file, idx) => (
                            <tr key={idx}>
                              <td className="file-path" title={file.path}>
                                {file.path}
                              </td>
                              <td>{formatBytes(file.size)}</td>
                              <td>
                                <div className="file-progress">
                                  <div
                                    className="file-progress-bar"
                                    style={{ width: `${file.progress}%` }}
                                  />
                                  <span className="file-progress-text">
                                    {file.progress.toFixed(1)}%
                                  </span>
                                </div>
                              </td>
                              <td>{formatBytes(file.downloaded)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    )}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      {}
      <div className="status-bar">
        <div className="status-section">
          <span className="status-item">
            <Icons.Download />
            <span>{formatSpeed(globalStatsData?.download_rate ?? globalStats.totalDownload)}</span>
          </span>
          <span className="status-item">
            <Icons.Upload />
            <span>{formatSpeed(globalStatsData?.upload_rate ?? globalStats.totalUpload)}</span>
          </span>
        </div>
        <div className="status-section">
          <span className="status-item">
            Downloading: {globalStats.downloading}
          </span>
          <span className="status-item">
            Seeding: {globalStats.seeding}
          </span>
          <span className="status-item">
            Total: {globalStats.total}
          </span>
        </div>
        <div className="status-section">
          <span className="status-item">
            <Icons.Peers />
            <span>Peers: {globalStatsData?.total_peers ?? 0}</span>
          </span>
          <span className="status-item connection">
            <span className="connection-dot connected" />
            DHT: Ready
          </span>
        </div>
      </div>

      {}
      {showAddModal && (
        <div className="modal-overlay" onClick={closeAddModal}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>Add Torrent</h2>
              <button className="modal-close" onClick={closeAddModal}>
                &times;
              </button>
            </div>
            <div className="modal-content">
              <div className="add-section">
                <h3>
                  <Icons.Folder /> From File
                </h3>
                <button className="btn-secondary btn-full" onClick={openTorrentFile}>
                  Select .torrent files...
                </button>

                {pendingTorrents.length > 0 && (
                  <div className="torrent-preview-multi">
                    <div className="torrent-list-panel">
                      <div className="torrent-list-header">
                        Torrents to Add ({pendingTorrents.length})
                      </div>
                      <div className="torrent-list-items">
                        {pendingTorrents.map((torrent, index) => (
                          <div
                            key={index}
                            className={`torrent-list-item ${selectedPendingIndex === index ? "selected" : ""}`}
                            onClick={() => setSelectedPendingIndex(index)}
                          >
                            <span className="torrent-list-name" title={torrent.info.name}>
                              {torrent.info.name}
                            </span>
                            <span className="torrent-list-size">
                              {formatBytes(torrent.info.total_size)}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                    <div className="torrent-detail-panel">
                      {pendingTorrents[selectedPendingIndex] && (
                        <>
                          <h4>{pendingTorrents[selectedPendingIndex].info.name}</h4>
                          <div className="preview-grid">
                            <span>Info Hash:</span>
                            <span className="monospace">{pendingTorrents[selectedPendingIndex].info.info_hash}</span>
                            <span>Size:</span>
                            <span>{formatBytes(pendingTorrents[selectedPendingIndex].info.total_size)}</span>
                            <span>Files:</span>
                            <span>{pendingTorrents[selectedPendingIndex].info.file_count}</span>
                            <span>Pieces:</span>
                            <span>
                              {pendingTorrents[selectedPendingIndex].info.piece_count} x {formatBytes(pendingTorrents[selectedPendingIndex].info.piece_length)}
                            </span>
                            <span>Version:</span>
                            <span>{pendingTorrents[selectedPendingIndex].info.version}</span>
                            <span>Private:</span>
                            <span>{pendingTorrents[selectedPendingIndex].info.is_private ? "Yes" : "No"}</span>
                            {pendingTorrents[selectedPendingIndex].info.created_by && (
                              <>
                                <span>Created By:</span>
                                <span>{pendingTorrents[selectedPendingIndex].info.created_by}</span>
                              </>
                            )}
                            {pendingTorrents[selectedPendingIndex].info.creation_date && (
                              <>
                                <span>Created:</span>
                                <span>{formatDate(pendingTorrents[selectedPendingIndex].info.creation_date)}</span>
                              </>
                            )}
                            {pendingTorrents[selectedPendingIndex].info.comment && (
                              <>
                                <span>Comment:</span>
                                <span>{pendingTorrents[selectedPendingIndex].info.comment}</span>
                              </>
                            )}
                          </div>
                          {pendingTorrents[selectedPendingIndex].info.trackers.length > 0 && (
                            <details className="preview-trackers">
                              <summary>Trackers ({pendingTorrents[selectedPendingIndex].info.trackers.length})</summary>
                              <ul>
                                {pendingTorrents[selectedPendingIndex].info.trackers.map((tracker, i) => (
                                  <li key={i} className="tracker-item">{tracker}</li>
                                ))}
                              </ul>
                            </details>
                          )}

                          {pendingTorrents[selectedPendingIndex].info.files.length > 0 && (
                            <details className="preview-files">
                              <summary>Files ({pendingTorrents[selectedPendingIndex].info.files.length})</summary>
                              <ul>
                                {pendingTorrents[selectedPendingIndex].info.files.slice(0, 10).map((file, i) => (
                                  <li key={i}>
                                    <span className="file-path">{file.path}</span>
                                    <span className="file-size">{formatBytes(file.size)}</span>
                                  </li>
                                ))}
                                {pendingTorrents[selectedPendingIndex].info.files.length > 10 && (
                                  <li className="more-files">
                                    ...and {pendingTorrents[selectedPendingIndex].info.files.length - 10} more files
                                  </li>
                                )}
                              </ul>
                            </details>
                          )}
                        </>
                      )}
                    </div>
                    <div className="torrent-add-actions">
                      <button className="btn-primary btn-full" onClick={addTorrentFromFile} disabled={loading}>
                        {loading ? "Adding..." : `Add ${pendingTorrents.length} Torrent${pendingTorrents.length > 1 ? "s" : ""}`}
                      </button>
                    </div>
                  </div>
                )}
              </div>

              <div className="add-divider">
                <span>OR</span>
              </div>

              <div className="add-section">
                <h3>
                  <Icons.Link /> From Magnet Link
                </h3>
                <div className="magnet-input">
                  <input
                    type="text"
                    value={magnetUri}
                    onChange={(e) => setMagnetUri(e.target.value)}
                    placeholder="magnet:?xt=urn:btih:..."
                  />
                  <button className="btn-secondary" onClick={parseMagnet} disabled={loading || !magnetUri}>
                    Parse
                  </button>
                </div>

                {magnetInfo && (
                  <div className="magnet-preview">
                    <h4>{magnetInfo.display_name || "Unknown"}</h4>
                    <div className="preview-grid">
                      <span>Info Hash:</span>
                      <span className="monospace">{magnetInfo.info_hash}</span>
                    </div>
                    {magnetInfo.trackers.length > 0 && (
                      <details>
                        <summary>Trackers ({magnetInfo.trackers.length})</summary>
                        <ul>
                          {magnetInfo.trackers.map((tracker, i) => (
                            <li key={i}>{tracker}</li>
                          ))}
                        </ul>
                      </details>
                    )}
                    <button className="btn-primary btn-full" onClick={addTorrentFromMagnet} disabled={loading}>
                      {loading ? "Adding..." : "Add Magnet"}
                    </button>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {}
      {showSettings && (
        <div className="modal-overlay" onClick={() => setShowSettings(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h2>Settings</h2>
              <button className="modal-close" onClick={() => setShowSettings(false)}>
                &times;
              </button>
            </div>
            <div className="modal-content">
              <div className="settings-section">
                <h3>Downloads</h3>
                <div className="setting-row">
                  <label>Default Download Location</label>
                  <div className="dir-select">
                    <input
                      type="text"
                      value={tempDownloadDir}
                      readOnly
                      className="dir-input"
                    />
                    <button
                      className="btn-secondary btn-small"
                      onClick={async () => {
                        try {
                          const selected = await open({
                            directory: true,
                            title: "Select Download Directory",
                            defaultPath: tempDownloadDir,
                          });
                          if (selected) {
                            setTempDownloadDir(selected);
                          }
                        } catch (e) {
                          setError(String(e));
                        }
                      }}
                    >
                      Browse
                    </button>
                  </div>
                </div>
                <div className="setting-row checkbox-row">
                  <label className="checkbox-label">
                    <input
                      type="checkbox"
                      checked={askDownloadLocation}
                      onChange={(e) => setAskDownloadLocation(e.target.checked)}
                    />
                    <span>Ask where to save each download</span>
                  </label>
                </div>
                {tempDownloadDir !== downloadDir && (
                  <p className="setting-note">
                    Note: Changing the download location will apply to new torrents only.
                    Restart the application to use the new location.
                  </p>
                )}
              </div>

              <div className="settings-section">
                <h3>Bandwidth Limits</h3>
                <div className="setting-row">
                  <label>Max Download Speed (KB/s)</label>
                  <input
                    type="number"
                    value={maxDownloadSpeed}
                    onChange={(e) => setMaxDownloadSpeed(Number(e.target.value))}
                    min={0}
                    placeholder="0 = unlimited"
                  />
                </div>
                <div className="setting-row">
                  <label>Max Upload Speed (KB/s)</label>
                  <input
                    type="number"
                    value={maxUploadSpeed}
                    onChange={(e) => setMaxUploadSpeed(Number(e.target.value))}
                    min={0}
                    placeholder="0 = unlimited"
                  />
                </div>
                <button className="btn-primary" onClick={applyBandwidthLimits}>
                  Apply Limits
                </button>
              </div>

              <div className="settings-section">
                <h3>Queueing</h3>
                <div className="setting-row">
                  <label>Max Active Downloads</label>
                  <input
                    type="number"
                    value={maxActiveDownloads}
                    onChange={(e) => setMaxActiveDownloads(Number(e.target.value))}
                    min={0}
                    placeholder="0 = unlimited"
                  />
                </div>
                <div className="setting-row">
                  <label>Max Active Uploads</label>
                  <input
                    type="number"
                    value={maxActiveUploads}
                    onChange={(e) => setMaxActiveUploads(Number(e.target.value))}
                    min={0}
                    placeholder="0 = unlimited"
                  />
                </div>
                <button className="btn-primary" onClick={applyQueueSettings}>
                  Apply Queue Settings
                </button>
              </div>

              <div className="settings-section">
                <h3>Seeding</h3>
                <div className="setting-row checkbox-row">
                  <label className="checkbox-label">
                    <input
                      type="checkbox"
                      checked={noSeedMode}
                      onChange={(e) => toggleNoSeedMode(e.target.checked)}
                    />
                    <span>No Seed Mode</span>
                  </label>
                </div>
                <p className="text-secondary setting-note">
                  When enabled, completed torrents will not upload to other peers.
                </p>
                <div className="setting-row checkbox-row">
                  <label className={`checkbox-label ${!noSeedMode ? "disabled" : ""}`}>
                    <input
                      type="checkbox"
                      checked={disconnectOnComplete}
                      onChange={(e) => toggleDisconnectOnComplete(e.target.checked)}
                      disabled={!noSeedMode}
                    />
                    <span>Disconnect on Complete</span>
                  </label>
                </div>
                <p className={`text-secondary setting-note ${!noSeedMode ? "disabled" : ""}`}>
                  When enabled, disconnect all peers when a torrent completes.
                </p>
              </div>

              <div className="settings-section">
                <h3>About</h3>
                <p>RBitt v0.1.0</p>
                <p className="text-secondary">A modern BitTorrent client built with Rust and Tauri.</p>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
