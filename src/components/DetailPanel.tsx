import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes, formatSpeed, formatState, getStateColor } from "../utils";
import type {
  TorrentStatus,
  TrackerStatusInfo,
  PeerStatusInfo,
  TorrentFileInfo,
  DetailTab,
  ShareLimitsInfo,
  CategoryInfo,
} from "../types";

// Priority constants matching backend FilePriority enum
const FILE_PRIORITIES = [
  { value: 0, label: "Skip", color: "var(--text-muted)" },
  { value: 1, label: "Low", color: "var(--text-secondary)" },
  { value: 4, label: "Normal", color: "var(--text-primary)" },
  { value: 6, label: "High", color: "var(--accent-hover)" },
  { value: 7, label: "Maximum", color: "var(--state-seeding)" },
];

interface DetailPanelProps {
  torrent: TorrentStatus;
  activeTab: DetailTab;
  onTabChange: (tab: DetailTab) => void;
  trackerInfo: TrackerStatusInfo[];
  peerInfo: PeerStatusInfo[];
  fileInfo: TorrentFileInfo[];
  onError?: (error: string) => void;
}

export function DetailPanel({
  torrent,
  activeTab,
  onTabChange,
  trackerInfo,
  peerInfo,
  fileInfo,
  onError,
}: DetailPanelProps) {
  const [filePriorities, setFilePriorities] = useState<number[]>([]);
  const [sequentialDownload, setSequentialDownload] = useState(false);

  // Tags state
  const [tags, setTags] = useState<string[]>([]);
  const [newTag, setNewTag] = useState("");

  // Share limits state
  const [shareLimits, setShareLimits] = useState<ShareLimitsInfo>({
    max_ratio: null,
    max_seeding_time: null,
    limit_action: "pause",
  });
  const [ratio, setRatio] = useState<number>(0);
  const [seedingTime, setSeedingTime] = useState<number>(0);

  // Category state
  const [categories, setCategories] = useState<CategoryInfo[]>([]);
  const [currentCategory, setCurrentCategory] = useState<string | null>(null);

  // Load file priorities when torrent changes
  useEffect(() => {
    async function loadPriorities() {
      try {
        const priorities: number[] = await invoke("get_file_priorities", {
          infoHash: torrent.info_hash,
        });
        setFilePriorities(priorities);
      } catch (e) {
        console.error("Failed to load file priorities:", e);
      }
    }
    loadPriorities();
  }, [torrent.info_hash, fileInfo.length]);

  // Load sequential download setting
  useEffect(() => {
    async function loadSequential() {
      try {
        const enabled: boolean = await invoke("get_sequential_download", {
          infoHash: torrent.info_hash,
        });
        setSequentialDownload(enabled);
      } catch (e) {
        console.error("Failed to load sequential download:", e);
      }
    }
    loadSequential();
  }, [torrent.info_hash]);

  // Load tags
  useEffect(() => {
    async function loadTags() {
      try {
        const torrentTags: string[] = await invoke("get_torrent_tags", {
          infoHash: torrent.info_hash,
        });
        setTags(torrentTags);
      } catch (e) {
        console.error("Failed to load tags:", e);
      }
    }
    loadTags();
  }, [torrent.info_hash]);

  // Load share limits, ratio, and seeding time
  useEffect(() => {
    async function loadShareInfo() {
      try {
        const limits: ShareLimitsInfo = await invoke("get_torrent_share_limits", {
          infoHash: torrent.info_hash,
        });
        setShareLimits(limits);

        const currentRatio: number = await invoke("get_torrent_ratio", {
          infoHash: torrent.info_hash,
        });
        setRatio(currentRatio);

        const currentSeedingTime: number = await invoke("get_torrent_seeding_time", {
          infoHash: torrent.info_hash,
        });
        setSeedingTime(currentSeedingTime);
      } catch (e) {
        console.error("Failed to load share info:", e);
      }
    }
    loadShareInfo();

    // Refresh ratio and seeding time periodically
    const interval = setInterval(loadShareInfo, 5000);
    return () => clearInterval(interval);
  }, [torrent.info_hash]);

  // Load categories
  useEffect(() => {
    async function loadCategories() {
      try {
        const cats: CategoryInfo[] = await invoke("get_categories");
        setCategories(cats);
      } catch (e) {
        console.error("Failed to load categories:", e);
      }
    }
    loadCategories();
  }, []);

  // Tag handlers
  async function handleAddTag() {
    if (!newTag.trim()) return;
    try {
      await invoke("add_torrent_tag", {
        infoHash: torrent.info_hash,
        tag: newTag.trim(),
      });
      setTags((prev) => [...prev, newTag.trim()]);
      setNewTag("");
    } catch (e) {
      console.error("Failed to add tag:", e);
      onError?.(String(e));
    }
  }

  async function handleRemoveTag(tag: string) {
    try {
      await invoke("remove_torrent_tag", {
        infoHash: torrent.info_hash,
        tag,
      });
      setTags((prev) => prev.filter((t) => t !== tag));
    } catch (e) {
      console.error("Failed to remove tag:", e);
      onError?.(String(e));
    }
  }

  // Share limits handlers
  async function handleSaveShareLimits() {
    try {
      await invoke("set_torrent_share_limits", {
        infoHash: torrent.info_hash,
        maxRatio: shareLimits.max_ratio,
        maxSeedingTime: shareLimits.max_seeding_time,
        limitAction: shareLimits.limit_action,
      });
    } catch (e) {
      console.error("Failed to save share limits:", e);
      onError?.(String(e));
    }
  }

  // Category handler
  async function handleCategoryChange(category: string | null) {
    try {
      await invoke("set_torrent_category", {
        infoHash: torrent.info_hash,
        category,
      });
      setCurrentCategory(category);
    } catch (e) {
      console.error("Failed to set category:", e);
      onError?.(String(e));
    }
  }

  function formatSeedingTime(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
    return `${Math.floor(seconds / 86400)}d ${Math.floor((seconds % 86400) / 3600)}h`;
  }

  async function handlePriorityChange(fileIndex: number, priority: number) {
    try {
      await invoke("set_file_priority", {
        infoHash: torrent.info_hash,
        fileIndex,
        priority,
      });
      // Update local state
      setFilePriorities((prev) => {
        const next = [...prev];
        next[fileIndex] = priority;
        return next;
      });
    } catch (e) {
      console.error("Failed to set file priority:", e);
      onError?.(String(e));
    }
  }

  async function handleSequentialToggle() {
    try {
      const newValue = !sequentialDownload;
      await invoke("set_sequential_download", {
        infoHash: torrent.info_hash,
        enabled: newValue,
      });
      setSequentialDownload(newValue);
    } catch (e) {
      console.error("Failed to toggle sequential download:", e);
      onError?.(String(e));
    }
  }

  function getPriorityLabel(priority: number): string {
    const p = FILE_PRIORITIES.find((fp) => fp.value === priority);
    return p?.label || "Normal";
  }

  function getPriorityColor(priority: number): string {
    const p = FILE_PRIORITIES.find((fp) => fp.value === priority);
    return p?.color || "inherit";
  }

  return (
    <div className="detail-panel">
      <div className="detail-tabs">
        <button
          className={activeTab === "general" ? "active" : ""}
          onClick={() => onTabChange("general")}
        >
          General
        </button>
        <button
          className={activeTab === "trackers" ? "active" : ""}
          onClick={() => onTabChange("trackers")}
        >
          Trackers
        </button>
        <button
          className={activeTab === "peers" ? "active" : ""}
          onClick={() => onTabChange("peers")}
        >
          Peers
        </button>
        <button
          className={activeTab === "files" ? "active" : ""}
          onClick={() => onTabChange("files")}
        >
          Files
        </button>
      </div>
      <div className="detail-content">
        {activeTab === "general" && (
          <div className="detail-general">
            <div className="detail-grid">
              <div className="detail-item">
                <span className="detail-label">Name</span>
                <span className="detail-value">{torrent.name}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Info Hash</span>
                <span className="detail-value monospace">{torrent.info_hash}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Status</span>
                <span className="detail-value" style={{ color: getStateColor(torrent.state) }}>
                  {formatState(torrent.state)}
                </span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Total Size</span>
                <span className="detail-value">{formatBytes(torrent.total_size)}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Progress</span>
                <span className="detail-value">
                  {torrent.progress.toFixed(2)}% ({formatBytes(torrent.downloaded)} /{" "}
                  {formatBytes(torrent.total_size)})
                </span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Uploaded</span>
                <span className="detail-value">{formatBytes(torrent.uploaded)}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Download Speed</span>
                <span className="detail-value">{formatSpeed(torrent.download_rate)}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Upload Speed</span>
                <span className="detail-value">{formatSpeed(torrent.upload_rate)}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Seeds</span>
                <span className="detail-value">{torrent.seeds}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Peers</span>
                <span className="detail-value">{torrent.peers}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Ratio</span>
                <span className="detail-value">{ratio.toFixed(2)}</span>
              </div>
              <div className="detail-item">
                <span className="detail-label">Seeding Time</span>
                <span className="detail-value">{formatSeedingTime(seedingTime)}</span>
              </div>
            </div>

            {/* Category */}
            <div className="detail-section">
              <h4>Category</h4>
              <select
                className="detail-select"
                value={currentCategory || ""}
                onChange={(e) => handleCategoryChange(e.target.value || null)}
              >
                <option value="">No Category</option>
                {categories.map((cat) => (
                  <option key={cat.name} value={cat.name}>
                    {cat.name}
                  </option>
                ))}
              </select>
            </div>

            {/* Tags */}
            <div className="detail-section">
              <h4>Tags</h4>
              <div className="tags-container">
                {tags.map((tag) => (
                  <span key={tag} className="tag">
                    {tag}
                    <button className="tag-remove" onClick={() => handleRemoveTag(tag)}>
                      ×
                    </button>
                  </span>
                ))}
              </div>
              <div className="tag-add">
                <input
                  type="text"
                  value={newTag}
                  onChange={(e) => setNewTag(e.target.value)}
                  placeholder="Add tag..."
                  onKeyDown={(e) => e.key === "Enter" && handleAddTag()}
                />
                <button className="btn-small" onClick={handleAddTag} disabled={!newTag.trim()}>
                  Add
                </button>
              </div>
            </div>

            {/* Share Limits */}
            <div className="detail-section">
              <h4>Share Limits</h4>
              <div className="share-limits-form">
                <div className="share-limit-row">
                  <label>Max Ratio</label>
                  <input
                    type="number"
                    step="0.1"
                    min="0"
                    value={shareLimits.max_ratio ?? ""}
                    onChange={(e) =>
                      setShareLimits({
                        ...shareLimits,
                        max_ratio: e.target.value ? parseFloat(e.target.value) : null,
                      })
                    }
                    placeholder="No limit"
                  />
                </div>
                <div className="share-limit-row">
                  <label>Max Seeding Time (minutes)</label>
                  <input
                    type="number"
                    min="0"
                    value={shareLimits.max_seeding_time ?? ""}
                    onChange={(e) =>
                      setShareLimits({
                        ...shareLimits,
                        max_seeding_time: e.target.value ? parseInt(e.target.value) : null,
                      })
                    }
                    placeholder="No limit"
                  />
                </div>
                <div className="share-limit-row">
                  <label>Action when limit reached</label>
                  <select
                    value={shareLimits.limit_action}
                    onChange={(e) =>
                      setShareLimits({ ...shareLimits, limit_action: e.target.value })
                    }
                  >
                    <option value="pause">Pause</option>
                    <option value="remove">Remove</option>
                    <option value="remove_with_files">Remove with Files</option>
                  </select>
                </div>
                <button className="btn-small btn-primary" onClick={handleSaveShareLimits}>
                  Save Limits
                </button>
              </div>
            </div>
          </div>
        )}
        {activeTab === "trackers" && (
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
                      <td
                        className={`tracker-status status-${tracker.status
                          .toLowerCase()
                          .replace(/[^a-z]/g, "-")}`}
                      >
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
        {activeTab === "peers" && (
          <div className="detail-peers">
            <div className="peers-summary">
              <span>
                Connected to {peerInfo.length} peers ({torrent.seeds} seeds)
              </span>
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
                                backgroundColor:
                                  peer.progress >= 100
                                    ? "var(--state-seeding)"
                                    : "var(--state-downloading)",
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
        {activeTab === "files" && (
          <div className="detail-files">
            <div className="files-toolbar">
              <label className="sequential-download-toggle">
                <input
                  type="checkbox"
                  checked={sequentialDownload}
                  onChange={handleSequentialToggle}
                />
                <span>Sequential Download</span>
              </label>
            </div>
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
                    <th>Priority</th>
                  </tr>
                </thead>
                <tbody>
                  {fileInfo.map((file, idx) => (
                    <tr key={idx} className={filePriorities[idx] === 0 ? "file-skipped" : ""}>
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
                          <span className="file-progress-text">{file.progress.toFixed(1)}%</span>
                        </div>
                      </td>
                      <td>{formatBytes(file.downloaded)}</td>
                      <td className="file-priority">
                        <select
                          value={filePriorities[idx] ?? 4}
                          onChange={(e) => handlePriorityChange(idx, Number(e.target.value))}
                          style={{ color: getPriorityColor(filePriorities[idx] ?? 4) }}
                        >
                          {FILE_PRIORITIES.map((p) => (
                            <option key={p.value} value={p.value}>
                              {p.label}
                            </option>
                          ))}
                        </select>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export default DetailPanel;
