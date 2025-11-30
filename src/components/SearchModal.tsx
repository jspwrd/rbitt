import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Icons } from "./Icons";
import { formatBytes } from "../utils";
import type { SearchPluginInfo, SearchResultInfo, SearchJobInfo } from "../types";

interface SearchModalProps {
  onClose: () => void;
  onError: (error: string) => void;
  onAddMagnet: (uri: string) => void;
}

type SearchTab = "search" | "plugins";

export function SearchModal({ onClose, onError, onAddMagnet }: SearchModalProps) {
  const [activeTab, setActiveTab] = useState<SearchTab>("search");
  const [plugins, setPlugins] = useState<SearchPluginInfo[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchCategory, setSearchCategory] = useState("");
  const [selectedPlugins] = useState<string[]>([]);
  const [currentSearch, setCurrentSearch] = useState<SearchJobInfo | null>(null);
  const [searchResults, setSearchResults] = useState<SearchResultInfo[]>([]);
  const [loading, setLoading] = useState(false);

  // Plugin installation
  const [newPluginUrl, setNewPluginUrl] = useState("");

  useEffect(() => {
    loadPlugins();
  }, []);

  useEffect(() => {
    let interval: number | undefined;
    if (currentSearch && currentSearch.status === "running") {
      interval = window.setInterval(() => {
        refreshSearchStatus(currentSearch.id);
      }, 1000);
    }
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [currentSearch?.id, currentSearch?.status]);

  async function loadPlugins() {
    try {
      await invoke("load_search_plugins");
      const result: SearchPluginInfo[] = await invoke("get_search_plugins");
      setPlugins(result);
    } catch (e) {
      console.error("Failed to load search plugins:", e);
    }
  }

  async function startSearch() {
    if (!searchQuery) return;
    setLoading(true);
    try {
      const searchId: string = await invoke("start_search", {
        query: searchQuery,
        plugins: selectedPlugins.length > 0 ? selectedPlugins : ["all"],
        category: searchCategory || null,
      });
      await refreshSearchStatus(searchId);
    } catch (e) {
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function refreshSearchStatus(searchId: string) {
    try {
      const status: SearchJobInfo = await invoke("get_search_status", { searchId });
      setCurrentSearch(status);

      const results: SearchResultInfo[] = await invoke("get_search_results", { searchId });
      setSearchResults(results);
    } catch (e) {
      console.error("Failed to refresh search status:", e);
    }
  }

  async function stopSearch() {
    if (!currentSearch) return;
    try {
      await invoke("stop_search", { searchId: currentSearch.id });
      await refreshSearchStatus(currentSearch.id);
    } catch (e) {
      onError(String(e));
    }
  }

  async function deleteSearch() {
    if (!currentSearch) return;
    try {
      await invoke("delete_search", { searchId: currentSearch.id });
      setCurrentSearch(null);
      setSearchResults([]);
    } catch (e) {
      onError(String(e));
    }
  }

  async function installPlugin() {
    if (!newPluginUrl) return;
    setLoading(true);
    try {
      await invoke("install_search_plugin", { url: newPluginUrl });
      setNewPluginUrl("");
      loadPlugins();
    } catch (e) {
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function removePlugin(name: string) {
    try {
      await invoke("remove_search_plugin", { name });
      loadPlugins();
    } catch (e) {
      onError(String(e));
    }
  }

  async function togglePluginEnabled(name: string, enabled: boolean) {
    try {
      await invoke("set_search_plugin_enabled", { name, enabled });
      loadPlugins();
    } catch (e) {
      onError(String(e));
    }
  }

  function downloadResult(result: SearchResultInfo) {
    if (result.download_link.startsWith("magnet:")) {
      onAddMagnet(result.download_link);
    }
  }

  const enabledPlugins = plugins.filter((p) => p.enabled);
  const allCategories = Array.from(new Set(enabledPlugins.flatMap((p) => p.categories)));

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal modal-large" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>
            <Icons.Search /> Search
          </h2>
          <button className="modal-close" onClick={onClose}>
            &times;
          </button>
        </div>
        <div className="modal-content search-layout">
          <div className="search-tabs">
            <button
              className={activeTab === "search" ? "active" : ""}
              onClick={() => setActiveTab("search")}
            >
              Search
            </button>
            <button
              className={activeTab === "plugins" ? "active" : ""}
              onClick={() => setActiveTab("plugins")}
            >
              Plugins ({plugins.length})
            </button>
          </div>

          {activeTab === "search" && (
            <div className="search-content">
              <div className="search-form">
                <div className="search-input-row">
                  <input
                    type="text"
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    placeholder="Search query..."
                    onKeyDown={(e) => e.key === "Enter" && startSearch()}
                    className="search-query-input"
                  />
                  <select
                    value={searchCategory}
                    onChange={(e) => setSearchCategory(e.target.value)}
                    className="search-category-select"
                  >
                    <option value="">All Categories</option>
                    {allCategories.map((cat) => (
                      <option key={cat} value={cat}>
                        {cat}
                      </option>
                    ))}
                  </select>
                  {currentSearch?.status === "running" ? (
                    <button className="btn-secondary" onClick={stopSearch}>
                      Stop
                    </button>
                  ) : (
                    <button
                      className="btn-primary"
                      onClick={startSearch}
                      disabled={loading || !searchQuery || enabledPlugins.length === 0}
                    >
                      Search
                    </button>
                  )}
                </div>

                {enabledPlugins.length === 0 && (
                  <p className="search-warning">
                    No search plugins installed. Go to the Plugins tab to install plugins.
                  </p>
                )}
              </div>

              {currentSearch && (
                <div className="search-status">
                  <span>
                    Status: {currentSearch.status} - {searchResults.length} results
                    {currentSearch.error && ` - Error: ${currentSearch.error}`}
                  </span>
                  {currentSearch.status !== "running" && (
                    <button className="btn-small btn-secondary" onClick={deleteSearch}>
                      Clear
                    </button>
                  )}
                </div>
              )}

              <div className="search-results">
                {searchResults.length === 0 ? (
                  <p className="search-placeholder">
                    {currentSearch
                      ? currentSearch.status === "running"
                        ? "Searching..."
                        : "No results found"
                      : "Enter a search query to begin"}
                  </p>
                ) : (
                  <table className="search-results-table">
                    <thead>
                      <tr>
                        <th>Name</th>
                        <th>Size</th>
                        <th>Seeds</th>
                        <th>Leechers</th>
                        <th>Plugin</th>
                        <th></th>
                      </tr>
                    </thead>
                    <tbody>
                      {searchResults.map((result, idx) => (
                        <tr key={idx}>
                          <td className="search-result-name" title={result.name}>
                            {result.description_link ? (
                              <a
                                href={result.description_link}
                                target="_blank"
                                rel="noopener noreferrer"
                              >
                                {result.name}
                              </a>
                            ) : (
                              result.name
                            )}
                          </td>
                          <td>{formatBytes(result.size)}</td>
                          <td className="search-seeds">
                            {result.seeders >= 0 ? result.seeders : "-"}
                          </td>
                          <td className="search-leechers">
                            {result.leechers >= 0 ? result.leechers : "-"}
                          </td>
                          <td className="search-plugin">{result.plugin}</td>
                          <td>
                            <button
                              className="btn-small btn-primary"
                              onClick={() => downloadResult(result)}
                            >
                              Download
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            </div>
          )}

          {activeTab === "plugins" && (
            <div className="search-plugins-content">
              <div className="plugins-list">
                {plugins.length === 0 ? (
                  <p className="search-placeholder">No search plugins installed</p>
                ) : (
                  plugins.map((plugin) => (
                    <div key={plugin.name} className="plugin-item">
                      <div className="plugin-info">
                        <span className="plugin-name">
                          <Icons.Plugin /> {plugin.display_name}
                        </span>
                        <span className="plugin-meta">
                          v{plugin.version} - Categories: {plugin.categories.join(", ")}
                        </span>
                      </div>
                      <div className="plugin-actions">
                        <label className="checkbox-label">
                          <input
                            type="checkbox"
                            checked={plugin.enabled}
                            onChange={(e) => togglePluginEnabled(plugin.name, e.target.checked)}
                          />
                          <span>Enabled</span>
                        </label>
                        <button
                          className="btn-icon danger"
                          onClick={() => removePlugin(plugin.name)}
                          title="Remove"
                        >
                          <Icons.Delete />
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>

              <div className="plugin-install">
                <h4>Install Plugin</h4>
                <p className="text-secondary">
                  Install search plugins compatible with qBittorrent format (Python scripts).
                </p>
                <div className="plugin-install-row">
                  <input
                    type="text"
                    value={newPluginUrl}
                    onChange={(e) => setNewPluginUrl(e.target.value)}
                    placeholder="Plugin URL (.py file)"
                  />
                  <button
                    className="btn-primary"
                    onClick={installPlugin}
                    disabled={loading || !newPluginUrl}
                  >
                    Install
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default SearchModal;
