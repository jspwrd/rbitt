import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Icons } from "./Icons";
import type {
  WatchFolderInfo,
  CategoryInfo,
  AutoTrackerSettingsInfo,
  MoveOnCompleteSettingsInfo,
  ExternalProgramSettingsInfo,
} from "../types";

interface SettingsModalProps {
  downloadDir: string;
  tempDownloadDir: string;
  onTempDownloadDirChange: (dir: string) => void;
  askDownloadLocation: boolean;
  onAskDownloadLocationChange: (ask: boolean) => void;
  maxDownloadSpeed: number;
  onMaxDownloadSpeedChange: (speed: number) => void;
  maxUploadSpeed: number;
  onMaxUploadSpeedChange: (speed: number) => void;
  maxActiveDownloads: number;
  onMaxActiveDownloadsChange: (num: number) => void;
  maxActiveUploads: number;
  onMaxActiveUploadsChange: (num: number) => void;
  noSeedMode: boolean;
  onNoSeedModeChange: (enabled: boolean) => void;
  disconnectOnComplete: boolean;
  onDisconnectOnCompleteChange: (enabled: boolean) => void;
  onApplyBandwidthLimits: () => void;
  onApplyQueueSettings: () => void;
  onClose: () => void;
  onError: (error: string) => void;
}

type SettingsTab = "general" | "downloads" | "connection" | "advanced" | "categories" | "watchfolders";

export function SettingsModal({
  downloadDir,
  tempDownloadDir,
  onTempDownloadDirChange,
  askDownloadLocation,
  onAskDownloadLocationChange,
  maxDownloadSpeed,
  onMaxDownloadSpeedChange,
  maxUploadSpeed,
  onMaxUploadSpeedChange,
  maxActiveDownloads,
  onMaxActiveDownloadsChange,
  maxActiveUploads,
  onMaxActiveUploadsChange,
  noSeedMode,
  onNoSeedModeChange,
  disconnectOnComplete,
  onDisconnectOnCompleteChange,
  onApplyBandwidthLimits,
  onApplyQueueSettings,
  onClose,
  onError,
}: SettingsModalProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");

  // Watch folders state
  const [watchFolders, setWatchFolders] = useState<WatchFolderInfo[]>([]);
  const [newWatchPath, setNewWatchPath] = useState("");
  const [newWatchCategory, setNewWatchCategory] = useState("");
  const [newWatchEnabled, setNewWatchEnabled] = useState(true);

  // Categories state
  const [categories, setCategories] = useState<CategoryInfo[]>([]);
  const [newCategoryName, setNewCategoryName] = useState("");
  const [newCategorySavePath, setNewCategorySavePath] = useState("");

  // Auto-tracker state
  const [autoTrackerSettings, setAutoTrackerSettings] = useState<AutoTrackerSettingsInfo>({
    enabled: false,
    trackers: [],
  });
  const [autoTrackerText, setAutoTrackerText] = useState("");

  // Move on complete state
  const [moveOnComplete, setMoveOnComplete] = useState<MoveOnCompleteSettingsInfo>({
    enabled: false,
    target_path: null,
    use_category_path: false,
  });

  // External program state
  const [externalProgram, setExternalProgram] = useState<ExternalProgramSettingsInfo>({
    on_completion_enabled: false,
    on_completion_command: null,
  });

  // Load settings on mount
  useEffect(() => {
    loadWatchFolders();
    loadCategories();
    loadAutoTrackerSettings();
    loadMoveOnCompleteSettings();
    loadExternalProgramSettings();
  }, []);

  async function loadWatchFolders() {
    try {
      const folders: WatchFolderInfo[] = await invoke("get_watch_folders");
      setWatchFolders(folders);
    } catch (e) {
      console.error("Failed to load watch folders:", e);
    }
  }

  async function loadCategories() {
    try {
      const cats: CategoryInfo[] = await invoke("get_categories");
      setCategories(cats);
    } catch (e) {
      console.error("Failed to load categories:", e);
    }
  }

  async function loadAutoTrackerSettings() {
    try {
      const settings: AutoTrackerSettingsInfo = await invoke("get_auto_tracker_settings");
      setAutoTrackerSettings(settings);
      setAutoTrackerText(settings.trackers.join("\n"));
    } catch (e) {
      console.error("Failed to load auto-tracker settings:", e);
    }
  }

  async function loadMoveOnCompleteSettings() {
    try {
      const settings: MoveOnCompleteSettingsInfo = await invoke("get_move_on_complete_settings");
      setMoveOnComplete(settings);
    } catch (e) {
      console.error("Failed to load move-on-complete settings:", e);
    }
  }

  async function loadExternalProgramSettings() {
    try {
      const settings: ExternalProgramSettingsInfo = await invoke("get_external_program_settings");
      setExternalProgram(settings);
    } catch (e) {
      console.error("Failed to load external program settings:", e);
    }
  }

  async function addWatchFolder() {
    if (!newWatchPath) return;
    try {
      await invoke("add_watch_folder", {
        path: newWatchPath,
        category: newWatchCategory || null,
        tags: [],
        processExisting: false,
        enabled: newWatchEnabled,
      });
      setNewWatchPath("");
      setNewWatchCategory("");
      loadWatchFolders();
    } catch (e) {
      onError(String(e));
    }
  }

  async function removeWatchFolder(id: string) {
    try {
      await invoke("remove_watch_folder", { id });
      loadWatchFolders();
    } catch (e) {
      onError(String(e));
    }
  }

  async function addCategory() {
    if (!newCategoryName || !newCategorySavePath) return;
    try {
      await invoke("add_category", {
        name: newCategoryName,
        savePath: newCategorySavePath,
      });
      setNewCategoryName("");
      setNewCategorySavePath("");
      loadCategories();
    } catch (e) {
      onError(String(e));
    }
  }

  async function removeCategory(name: string) {
    try {
      await invoke("remove_category", { name });
      loadCategories();
    } catch (e) {
      onError(String(e));
    }
  }

  async function saveAutoTrackerSettings() {
    try {
      const trackers = autoTrackerText
        .split("\n")
        .map((t) => t.trim())
        .filter((t) => t.length > 0);
      await invoke("set_auto_tracker_settings", {
        enabled: autoTrackerSettings.enabled,
        trackers,
      });
      setAutoTrackerSettings({ ...autoTrackerSettings, trackers });
    } catch (e) {
      onError(String(e));
    }
  }

  async function saveMoveOnCompleteSettings() {
    try {
      await invoke("set_move_on_complete_settings", {
        enabled: moveOnComplete.enabled,
        targetPath: moveOnComplete.target_path,
        useCategoryPath: moveOnComplete.use_category_path,
      });
    } catch (e) {
      onError(String(e));
    }
  }

  async function saveExternalProgramSettings() {
    try {
      await invoke("set_external_program_settings", {
        onCompletionEnabled: externalProgram.on_completion_enabled,
        command: externalProgram.on_completion_command,
      });
    } catch (e) {
      onError(String(e));
    }
  }

  async function selectDirectory(setter: (path: string) => void, defaultPath?: string) {
    try {
      const selected = await open({
        directory: true,
        title: "Select Directory",
        defaultPath,
      });
      if (selected) {
        setter(selected);
      }
    } catch (e) {
      onError(String(e));
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal modal-large" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Settings</h2>
          <button className="modal-close" onClick={onClose}>
            &times;
          </button>
        </div>
        <div className="modal-content settings-layout">
          <div className="settings-tabs">
            <button
              className={activeTab === "general" ? "active" : ""}
              onClick={() => setActiveTab("general")}
            >
              General
            </button>
            <button
              className={activeTab === "downloads" ? "active" : ""}
              onClick={() => setActiveTab("downloads")}
            >
              Downloads
            </button>
            <button
              className={activeTab === "connection" ? "active" : ""}
              onClick={() => setActiveTab("connection")}
            >
              Connection
            </button>
            <button
              className={activeTab === "categories" ? "active" : ""}
              onClick={() => setActiveTab("categories")}
            >
              Categories
            </button>
            <button
              className={activeTab === "watchfolders" ? "active" : ""}
              onClick={() => setActiveTab("watchfolders")}
            >
              Watch Folders
            </button>
            <button
              className={activeTab === "advanced" ? "active" : ""}
              onClick={() => setActiveTab("advanced")}
            >
              Advanced
            </button>
          </div>

          <div className="settings-content">
            {activeTab === "general" && (
              <div className="settings-section">
                <h3>About</h3>
                <p>RBitt v0.1.0</p>
                <p className="text-secondary">A modern BitTorrent client built with Rust and Tauri.</p>
              </div>
            )}

            {activeTab === "downloads" && (
              <>
                <div className="settings-section">
                  <h3>Download Location</h3>
                  <div className="setting-row">
                    <label>Default Download Location</label>
                    <div className="dir-select">
                      <input type="text" value={tempDownloadDir} readOnly className="dir-input" />
                      <button
                        className="btn-secondary btn-small"
                        onClick={() => selectDirectory(onTempDownloadDirChange, tempDownloadDir)}
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
                        onChange={(e) => onAskDownloadLocationChange(e.target.checked)}
                      />
                      <span>Ask where to save each download</span>
                    </label>
                  </div>
                  {tempDownloadDir !== downloadDir && (
                    <p className="setting-note">
                      Note: Changing the download location will apply to new torrents only. Restart
                      the application to use the new location.
                    </p>
                  )}
                </div>

                <div className="settings-section">
                  <h3>Move Completed Downloads</h3>
                  <div className="setting-row checkbox-row">
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={moveOnComplete.enabled}
                        onChange={(e) =>
                          setMoveOnComplete({ ...moveOnComplete, enabled: e.target.checked })
                        }
                      />
                      <span>Move completed downloads to a different folder</span>
                    </label>
                  </div>
                  {moveOnComplete.enabled && (
                    <>
                      <div className="setting-row checkbox-row">
                        <label className="checkbox-label">
                          <input
                            type="checkbox"
                            checked={moveOnComplete.use_category_path}
                            onChange={(e) =>
                              setMoveOnComplete({
                                ...moveOnComplete,
                                use_category_path: e.target.checked,
                              })
                            }
                          />
                          <span>Use category save path</span>
                        </label>
                      </div>
                      {!moveOnComplete.use_category_path && (
                        <div className="setting-row">
                          <label>Target Directory</label>
                          <div className="dir-select">
                            <input
                              type="text"
                              value={moveOnComplete.target_path || ""}
                              readOnly
                              className="dir-input"
                            />
                            <button
                              className="btn-secondary btn-small"
                              onClick={() =>
                                selectDirectory(
                                  (p) => setMoveOnComplete({ ...moveOnComplete, target_path: p }),
                                  moveOnComplete.target_path || undefined
                                )
                              }
                            >
                              Browse
                            </button>
                          </div>
                        </div>
                      )}
                      <button className="btn-primary" onClick={saveMoveOnCompleteSettings}>
                        Save Move Settings
                      </button>
                    </>
                  )}
                </div>

                <div className="settings-section">
                  <h3>Seeding</h3>
                  <div className="setting-row checkbox-row">
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={noSeedMode}
                        onChange={(e) => onNoSeedModeChange(e.target.checked)}
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
                        onChange={(e) => onDisconnectOnCompleteChange(e.target.checked)}
                        disabled={!noSeedMode}
                      />
                      <span>Disconnect on Complete</span>
                    </label>
                  </div>
                  <p className={`text-secondary setting-note ${!noSeedMode ? "disabled" : ""}`}>
                    When enabled, disconnect all peers when a torrent completes.
                  </p>
                </div>
              </>
            )}

            {activeTab === "connection" && (
              <>
                <div className="settings-section">
                  <h3>Bandwidth Limits</h3>
                  <div className="setting-row">
                    <label>Max Download Speed (KB/s)</label>
                    <input
                      type="number"
                      value={maxDownloadSpeed}
                      onChange={(e) => onMaxDownloadSpeedChange(Number(e.target.value))}
                      min={0}
                      placeholder="0 = unlimited"
                    />
                  </div>
                  <div className="setting-row">
                    <label>Max Upload Speed (KB/s)</label>
                    <input
                      type="number"
                      value={maxUploadSpeed}
                      onChange={(e) => onMaxUploadSpeedChange(Number(e.target.value))}
                      min={0}
                      placeholder="0 = unlimited"
                    />
                  </div>
                  <button className="btn-primary" onClick={onApplyBandwidthLimits}>
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
                      onChange={(e) => onMaxActiveDownloadsChange(Number(e.target.value))}
                      min={0}
                      placeholder="0 = unlimited"
                    />
                  </div>
                  <div className="setting-row">
                    <label>Max Active Uploads</label>
                    <input
                      type="number"
                      value={maxActiveUploads}
                      onChange={(e) => onMaxActiveUploadsChange(Number(e.target.value))}
                      min={0}
                      placeholder="0 = unlimited"
                    />
                  </div>
                  <button className="btn-primary" onClick={onApplyQueueSettings}>
                    Apply Queue Settings
                  </button>
                </div>
              </>
            )}

            {activeTab === "categories" && (
              <div className="settings-section">
                <h3>Categories</h3>
                <p className="text-secondary">
                  Categories allow you to organize torrents and set custom save paths.
                </p>

                {categories.length > 0 && (
                  <div className="settings-list">
                    {categories.map((cat) => (
                      <div key={cat.name} className="settings-list-item">
                        <div className="settings-list-info">
                          <span className="settings-list-name">
                            <Icons.Category /> {cat.name}
                          </span>
                          <span className="settings-list-path">{cat.save_path}</span>
                        </div>
                        <button
                          className="btn-icon danger"
                          onClick={() => removeCategory(cat.name)}
                          title="Remove"
                        >
                          <Icons.Delete />
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                <div className="settings-add-form">
                  <h4>Add Category</h4>
                  <div className="setting-row">
                    <label>Name</label>
                    <input
                      type="text"
                      value={newCategoryName}
                      onChange={(e) => setNewCategoryName(e.target.value)}
                      placeholder="Category name"
                    />
                  </div>
                  <div className="setting-row">
                    <label>Save Path</label>
                    <div className="dir-select">
                      <input
                        type="text"
                        value={newCategorySavePath}
                        onChange={(e) => setNewCategorySavePath(e.target.value)}
                        placeholder="Save path"
                      />
                      <button
                        className="btn-secondary btn-small"
                        onClick={() => selectDirectory(setNewCategorySavePath)}
                      >
                        Browse
                      </button>
                    </div>
                  </div>
                  <button
                    className="btn-primary"
                    onClick={addCategory}
                    disabled={!newCategoryName || !newCategorySavePath}
                  >
                    Add Category
                  </button>
                </div>
              </div>
            )}

            {activeTab === "watchfolders" && (
              <div className="settings-section">
                <h3>Watch Folders</h3>
                <p className="text-secondary">
                  Automatically add torrents from specified folders.
                </p>

                {watchFolders.length > 0 && (
                  <div className="settings-list">
                    {watchFolders.map((folder) => (
                      <div key={folder.id} className="settings-list-item">
                        <div className="settings-list-info">
                          <span className="settings-list-name">
                            <Icons.FolderOpen /> {folder.path}
                          </span>
                          <span className="settings-list-meta">
                            {folder.enabled ? "Enabled" : "Disabled"}
                            {folder.category && ` - Category: ${folder.category}`}
                          </span>
                        </div>
                        <button
                          className="btn-icon danger"
                          onClick={() => removeWatchFolder(folder.id)}
                          title="Remove"
                        >
                          <Icons.Delete />
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                <div className="settings-add-form">
                  <h4>Add Watch Folder</h4>
                  <div className="setting-row">
                    <label>Folder Path</label>
                    <div className="dir-select">
                      <input
                        type="text"
                        value={newWatchPath}
                        onChange={(e) => setNewWatchPath(e.target.value)}
                        placeholder="Folder path"
                      />
                      <button
                        className="btn-secondary btn-small"
                        onClick={() => selectDirectory(setNewWatchPath)}
                      >
                        Browse
                      </button>
                    </div>
                  </div>
                  <div className="setting-row">
                    <label>Category (optional)</label>
                    <input
                      type="text"
                      value={newWatchCategory}
                      onChange={(e) => setNewWatchCategory(e.target.value)}
                      placeholder="Category name"
                    />
                  </div>
                  <div className="setting-row checkbox-row">
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={newWatchEnabled}
                        onChange={(e) => setNewWatchEnabled(e.target.checked)}
                      />
                      <span>Enabled</span>
                    </label>
                  </div>
                  <button className="btn-primary" onClick={addWatchFolder} disabled={!newWatchPath}>
                    Add Watch Folder
                  </button>
                </div>
              </div>
            )}

            {activeTab === "advanced" && (
              <>
                <div className="settings-section">
                  <h3>Auto-Add Trackers</h3>
                  <div className="setting-row checkbox-row">
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={autoTrackerSettings.enabled}
                        onChange={(e) =>
                          setAutoTrackerSettings({
                            ...autoTrackerSettings,
                            enabled: e.target.checked,
                          })
                        }
                      />
                      <span>Automatically add trackers to new torrents</span>
                    </label>
                  </div>
                  {autoTrackerSettings.enabled && (
                    <>
                      <div className="setting-row">
                        <label>Trackers (one per line)</label>
                        <textarea
                          className="settings-textarea"
                          value={autoTrackerText}
                          onChange={(e) => setAutoTrackerText(e.target.value)}
                          placeholder="udp://tracker.example.com:6969/announce"
                          rows={5}
                        />
                      </div>
                      <button className="btn-primary" onClick={saveAutoTrackerSettings}>
                        Save Tracker Settings
                      </button>
                    </>
                  )}
                </div>

                <div className="settings-section">
                  <h3>External Program</h3>
                  <div className="setting-row checkbox-row">
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={externalProgram.on_completion_enabled}
                        onChange={(e) =>
                          setExternalProgram({
                            ...externalProgram,
                            on_completion_enabled: e.target.checked,
                          })
                        }
                      />
                      <span>Run external program on torrent completion</span>
                    </label>
                  </div>
                  {externalProgram.on_completion_enabled && (
                    <>
                      <div className="setting-row">
                        <label>Command</label>
                        <input
                          type="text"
                          value={externalProgram.on_completion_command || ""}
                          onChange={(e) =>
                            setExternalProgram({
                              ...externalProgram,
                              on_completion_command: e.target.value || null,
                            })
                          }
                          placeholder="/path/to/script.sh %N %F"
                        />
                      </div>
                      <p className="text-secondary setting-note">
                        Placeholders: %N (name), %F (content path), %R (root path), %D (save path),
                        %I (info hash)
                      </p>
                      <button className="btn-primary" onClick={saveExternalProgramSettings}>
                        Save External Program Settings
                      </button>
                    </>
                  )}
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default SettingsModal;
