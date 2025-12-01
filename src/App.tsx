import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import "./App.css";
import { useTheme } from "./hooks";

import {
  Icons,
  Sidebar,
  Toolbar,
  TorrentList,
  DetailPanel,
  StatusBar,
  AddTorrentModal,
  SettingsModal,
  RssModal,
  SearchModal,
  ResizablePanel,
} from "./components";

import {
  isDownloading,
  isUploading,
  isCompleted,
  isPaused,
  isQueued,
  isChecking,
  isStalledDownloading,
  isStalledUploading,
  isStalled,
  isActive,
} from "./utils";

import type {
  TorrentInfo,
  MagnetInfo,
  PendingTorrent,
  TorrentStatus,
  TrackerStatusInfo,
  PeerStatusInfo,
  TorrentFileInfo,
  GlobalStats,
  FilterType,
  DetailTab,
} from "./types";

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

  // Add torrent modal state
  const [showAddModal, setShowAddModal] = useState(false);
  const [pendingTorrents, setPendingTorrents] = useState<PendingTorrent[]>([]);
  const [selectedPendingIndex, setSelectedPendingIndex] = useState<number>(0);
  const [magnetUri, setMagnetUri] = useState("");
  const [magnetInfo, setMagnetInfo] = useState<MagnetInfo | null>(null);

  // Settings modal state
  const [showSettings, setShowSettings] = useState(false);
  const [maxDownloadSpeed, setMaxDownloadSpeed] = useState(0);
  const [maxUploadSpeed, setMaxUploadSpeed] = useState(0);
  const [maxActiveDownloads, setMaxActiveDownloads] = useState(5);
  const [maxActiveUploads, setMaxActiveUploads] = useState(5);
  const [askDownloadLocation, setAskDownloadLocation] = useState(false);
  const [tempDownloadDir, setTempDownloadDir] = useState("");
  const [noSeedMode, setNoSeedMode] = useState(false);
  const [disconnectOnComplete, setDisconnectOnComplete] = useState(false);

  // Feature modals
  const [showRssModal, setShowRssModal] = useState(false);
  const [showSearchModal, setShowSearchModal] = useState(false);

  // UI preferences
  const [useStatusIndicators, setUseStatusIndicators] = useState(false);
  const { themeMode, setTheme } = useTheme();

  // Detail panel data
  const [trackerInfo, setTrackerInfo] = useState<TrackerStatusInfo[]>([]);
  const [peerInfo, setPeerInfo] = useState<PeerStatusInfo[]>([]);
  const [fileInfo, setFileInfo] = useState<TorrentFileInfo[]>([]);
  const [globalStatsData, setGlobalStatsData] = useState<GlobalStats | null>(null);

  // Auto-init on mount
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

  // Periodic torrent refresh
  useEffect(() => {
    if (engineInitialized) {
      refreshTorrents();
      const interval = setInterval(refreshTorrents, 1000);
      return () => clearInterval(interval);
    }
  }, [engineInitialized, refreshTorrents]);

  // Fetch tracker info
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

  // Fetch peer info
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

  // Fetch file info
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

  // Fetch global stats
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

  // Load settings on engine init
  useEffect(() => {
    if (engineInitialized) {
      loadQueueSettings();
      loadNoSeedMode();
      loadDisconnectOnComplete();
    }
  }, [engineInitialized]);

  // Filter torrents
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

  // Action handlers
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

  async function addMagnetDirectly(uri: string) {
    setLoading(true);
    setError(null);
    try {
      await invoke("add_magnet", { uri });
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

  // Loading screen
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

  // Setup screen
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

  // Main app
  return (
    <div className="app">
      <Toolbar
        selectedTorrentData={selectedTorrentData}
        onAdd={() => setShowAddModal(true)}
        onResume={() => selectedTorrent && resumeTorrent(selectedTorrent)}
        onPause={() => selectedTorrent && pauseTorrent(selectedTorrent)}
        onRemove={() => selectedTorrent && removeTorrent(selectedTorrent, false)}
        onSettings={() => setShowSettings(true)}
      />

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      <div className="main-content">
        <Sidebar
          torrents={torrents}
          filter={filter}
          onFilterChange={setFilter}
          onRssClick={() => setShowRssModal(true)}
          onSearchClick={() => setShowSearchModal(true)}
        />

        <div className="content-area">
          <TorrentList
            torrents={filteredTorrents}
            selectedTorrent={selectedTorrent}
            onSelect={setSelectedTorrent}
            onDoubleClick={(torrent) => {
              if (isPaused(torrent.state)) {
                resumeTorrent(torrent.info_hash);
              } else {
                pauseTorrent(torrent.info_hash);
              }
            }}
            onAddClick={() => setShowAddModal(true)}
            useStatusIndicators={useStatusIndicators}
          />

          {selectedTorrentData && (
            <ResizablePanel
              minHeight={100}
              maxHeight={600}
              defaultHeight={200}
              storageKey="detailPanelHeight"
            >
              <DetailPanel
                torrent={selectedTorrentData}
                activeTab={detailTab}
                onTabChange={setDetailTab}
                trackerInfo={trackerInfo}
                peerInfo={peerInfo}
                fileInfo={fileInfo}
                onError={setError}
              />
            </ResizablePanel>
          )}
        </div>
      </div>

      <StatusBar torrents={torrents} globalStats={globalStatsData} />

      {showAddModal && (
        <AddTorrentModal
          pendingTorrents={pendingTorrents}
          selectedPendingIndex={selectedPendingIndex}
          onSelectPending={setSelectedPendingIndex}
          magnetUri={magnetUri}
          onMagnetChange={setMagnetUri}
          magnetInfo={magnetInfo}
          loading={loading}
          onClose={closeAddModal}
          onOpenFile={openTorrentFile}
          onParseMagnet={parseMagnet}
          onAddFromFile={addTorrentFromFile}
          onAddFromMagnet={addTorrentFromMagnet}
        />
      )}

      {showSettings && (
        <SettingsModal
          downloadDir={downloadDir}
          tempDownloadDir={tempDownloadDir}
          onTempDownloadDirChange={setTempDownloadDir}
          askDownloadLocation={askDownloadLocation}
          onAskDownloadLocationChange={setAskDownloadLocation}
          maxDownloadSpeed={maxDownloadSpeed}
          onMaxDownloadSpeedChange={setMaxDownloadSpeed}
          maxUploadSpeed={maxUploadSpeed}
          onMaxUploadSpeedChange={setMaxUploadSpeed}
          maxActiveDownloads={maxActiveDownloads}
          onMaxActiveDownloadsChange={setMaxActiveDownloads}
          maxActiveUploads={maxActiveUploads}
          onMaxActiveUploadsChange={setMaxActiveUploads}
          noSeedMode={noSeedMode}
          onNoSeedModeChange={toggleNoSeedMode}
          disconnectOnComplete={disconnectOnComplete}
          onDisconnectOnCompleteChange={toggleDisconnectOnComplete}
          onApplyBandwidthLimits={applyBandwidthLimits}
          onApplyQueueSettings={applyQueueSettings}
          useStatusIndicators={useStatusIndicators}
          onUseStatusIndicatorsChange={setUseStatusIndicators}
          themeMode={themeMode}
          onThemeModeChange={setTheme}
          onClose={() => setShowSettings(false)}
          onError={setError}
        />
      )}

      {showRssModal && (
        <RssModal
          onClose={() => setShowRssModal(false)}
          onError={setError}
          onAddMagnet={addMagnetDirectly}
        />
      )}

      {showSearchModal && (
        <SearchModal
          onClose={() => setShowSearchModal(false)}
          onError={setError}
          onAddMagnet={addMagnetDirectly}
        />
      )}
    </div>
  );
}

export default App;
