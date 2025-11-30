import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Icons } from "./Icons";
import { formatDate } from "../utils";
import type { RssFeedInfo, RssRuleInfo, RssItemInfo } from "../types";

interface RssModalProps {
  onClose: () => void;
  onError: (error: string) => void;
  onAddMagnet: (uri: string) => void;
}

type RssTab = "feeds" | "rules";

export function RssModal({ onClose, onError, onAddMagnet }: RssModalProps) {
  const [activeTab, setActiveTab] = useState<RssTab>("feeds");
  const [feeds, setFeeds] = useState<RssFeedInfo[]>([]);
  const [rules, setRules] = useState<RssRuleInfo[]>([]);
  const [selectedFeed, setSelectedFeed] = useState<string | null>(null);
  const [feedItems, setFeedItems] = useState<RssItemInfo[]>([]);
  const [loading, setLoading] = useState(false);

  // New feed form
  const [newFeedUrl, setNewFeedUrl] = useState("");
  const [newFeedName, setNewFeedName] = useState("");
  const [newFeedInterval, setNewFeedInterval] = useState(900);

  // New rule form
  const [showRuleForm, setShowRuleForm] = useState(false);
  const [newRuleName, setNewRuleName] = useState("");
  const [newRuleMustContain, setNewRuleMustContain] = useState("");
  const [newRuleMustNotContain, setNewRuleMustNotContain] = useState("");
  const [newRuleUseRegex, setNewRuleUseRegex] = useState(false);
  const [newRuleEpisodeFilter, setNewRuleEpisodeFilter] = useState("");
  const [newRuleCategory, setNewRuleCategory] = useState("");
  const [newRuleSavePath, setNewRuleSavePath] = useState("");
  const [newRuleAddPaused, setNewRuleAddPaused] = useState(false);

  useEffect(() => {
    loadFeeds();
    loadRules();
  }, []);

  useEffect(() => {
    if (selectedFeed) {
      loadFeedItems(selectedFeed);
    } else {
      setFeedItems([]);
    }
  }, [selectedFeed]);

  async function loadFeeds() {
    try {
      const result: RssFeedInfo[] = await invoke("get_rss_feeds");
      setFeeds(result);
    } catch (e) {
      console.error("Failed to load RSS feeds:", e);
    }
  }

  async function loadRules() {
    try {
      const result: RssRuleInfo[] = await invoke("get_rss_rules");
      setRules(result);
    } catch (e) {
      console.error("Failed to load RSS rules:", e);
    }
  }

  async function loadFeedItems(feedId: string) {
    try {
      const result: RssItemInfo[] = await invoke("get_rss_feed_items", { feedId });
      setFeedItems(result);
    } catch (e) {
      console.error("Failed to load feed items:", e);
    }
  }

  async function addFeed() {
    if (!newFeedUrl || !newFeedName) return;
    setLoading(true);
    try {
      await invoke("add_rss_feed", {
        url: newFeedUrl,
        name: newFeedName,
        refreshInterval: newFeedInterval,
      });
      setNewFeedUrl("");
      setNewFeedName("");
      loadFeeds();
    } catch (e) {
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function removeFeed(id: string) {
    try {
      await invoke("remove_rss_feed", { id });
      if (selectedFeed === id) {
        setSelectedFeed(null);
      }
      loadFeeds();
    } catch (e) {
      onError(String(e));
    }
  }

  async function refreshFeed(id: string) {
    setLoading(true);
    try {
      await invoke("refresh_rss_feed", { id });
      loadFeeds();
      if (selectedFeed === id) {
        loadFeedItems(id);
      }
    } catch (e) {
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function addRule() {
    if (!newRuleName || !newRuleMustContain) return;
    setLoading(true);
    try {
      await invoke("add_rss_rule", {
        name: newRuleName,
        mustContain: newRuleMustContain,
        mustNotContain: newRuleMustNotContain,
        useRegex: newRuleUseRegex,
        episodeFilter: newRuleEpisodeFilter || null,
        affectedFeeds: [],
        category: newRuleCategory || null,
        tags: [],
        savePath: newRuleSavePath || null,
        addPaused: newRuleAddPaused,
      });
      resetRuleForm();
      loadRules();
    } catch (e) {
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function removeRule(id: string) {
    try {
      await invoke("remove_rss_rule", { id });
      loadRules();
    } catch (e) {
      onError(String(e));
    }
  }

  function resetRuleForm() {
    setShowRuleForm(false);
    setNewRuleName("");
    setNewRuleMustContain("");
    setNewRuleMustNotContain("");
    setNewRuleUseRegex(false);
    setNewRuleEpisodeFilter("");
    setNewRuleCategory("");
    setNewRuleSavePath("");
    setNewRuleAddPaused(false);
  }

  function downloadItem(item: RssItemInfo) {
    if (item.torrent_url.startsWith("magnet:")) {
      onAddMagnet(item.torrent_url);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal modal-large" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>
            <Icons.Rss /> RSS
          </h2>
          <button className="modal-close" onClick={onClose}>
            &times;
          </button>
        </div>
        <div className="modal-content rss-layout">
          <div className="rss-tabs">
            <button
              className={activeTab === "feeds" ? "active" : ""}
              onClick={() => setActiveTab("feeds")}
            >
              Feeds
            </button>
            <button
              className={activeTab === "rules" ? "active" : ""}
              onClick={() => setActiveTab("rules")}
            >
              Download Rules
            </button>
          </div>

          {activeTab === "feeds" && (
            <div className="rss-feeds-layout">
              <div className="rss-feeds-list">
                <div className="rss-list-header">
                  <span>Feeds ({feeds.length})</span>
                </div>
                {feeds.map((feed) => (
                  <div
                    key={feed.id}
                    className={`rss-feed-item ${selectedFeed === feed.id ? "selected" : ""}`}
                    onClick={() => setSelectedFeed(feed.id)}
                  >
                    <div className="rss-feed-info">
                      <span className="rss-feed-name">{feed.name}</span>
                      <span className="rss-feed-url">{feed.url}</span>
                      {feed.last_error && (
                        <span className="rss-feed-error">{feed.last_error}</span>
                      )}
                    </div>
                    <div className="rss-feed-actions">
                      <button
                        className="btn-icon"
                        onClick={(e) => {
                          e.stopPropagation();
                          refreshFeed(feed.id);
                        }}
                        title="Refresh"
                      >
                        <Icons.Refresh />
                      </button>
                      <button
                        className="btn-icon danger"
                        onClick={(e) => {
                          e.stopPropagation();
                          removeFeed(feed.id);
                        }}
                        title="Remove"
                      >
                        <Icons.Delete />
                      </button>
                    </div>
                  </div>
                ))}

                <div className="rss-add-feed">
                  <h4>Add Feed</h4>
                  <input
                    type="text"
                    value={newFeedName}
                    onChange={(e) => setNewFeedName(e.target.value)}
                    placeholder="Feed name"
                  />
                  <input
                    type="text"
                    value={newFeedUrl}
                    onChange={(e) => setNewFeedUrl(e.target.value)}
                    placeholder="Feed URL"
                  />
                  <div className="rss-add-row">
                    <input
                      type="number"
                      value={newFeedInterval}
                      onChange={(e) => setNewFeedInterval(Number(e.target.value))}
                      min={60}
                      placeholder="Refresh interval (seconds)"
                    />
                    <button
                      className="btn-primary"
                      onClick={addFeed}
                      disabled={loading || !newFeedUrl || !newFeedName}
                    >
                      Add
                    </button>
                  </div>
                </div>
              </div>

              <div className="rss-feed-items">
                <div className="rss-list-header">
                  <span>Items {selectedFeed ? `(${feedItems.length})` : ""}</span>
                </div>
                {!selectedFeed ? (
                  <p className="rss-placeholder">Select a feed to view items</p>
                ) : feedItems.length === 0 ? (
                  <p className="rss-placeholder">No items in this feed</p>
                ) : (
                  <div className="rss-items-list">
                    {feedItems.map((item, idx) => (
                      <div key={idx} className="rss-item">
                        <div className="rss-item-info">
                          <span className="rss-item-title">{item.title}</span>
                          {item.pub_date && (
                            <span className="rss-item-date">{formatDate(item.pub_date)}</span>
                          )}
                        </div>
                        <div className="rss-item-actions">
                          {item.is_downloaded ? (
                            <span className="rss-item-downloaded">Downloaded</span>
                          ) : (
                            <button
                              className="btn-small btn-primary"
                              onClick={() => downloadItem(item)}
                            >
                              Download
                            </button>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}

          {activeTab === "rules" && (
            <div className="rss-rules-content">
              <div className="rss-rules-list">
                {rules.length === 0 ? (
                  <p className="rss-placeholder">No download rules configured</p>
                ) : (
                  rules.map((rule) => (
                    <div key={rule.id} className="rss-rule-item">
                      <div className="rss-rule-info">
                        <span className="rss-rule-name">
                          <Icons.Rule /> {rule.name}
                        </span>
                        <span className="rss-rule-filter">
                          Must contain: {rule.must_contain}
                          {rule.use_regex && " (regex)"}
                        </span>
                        {rule.must_not_contain && (
                          <span className="rss-rule-filter">
                            Must not contain: {rule.must_not_contain}
                          </span>
                        )}
                        {rule.last_match && (
                          <span className="rss-rule-meta">
                            Last match: {formatDate(rule.last_match)}
                          </span>
                        )}
                      </div>
                      <button
                        className="btn-icon danger"
                        onClick={() => removeRule(rule.id)}
                        title="Remove"
                      >
                        <Icons.Delete />
                      </button>
                    </div>
                  ))
                )}
              </div>

              {!showRuleForm ? (
                <button className="btn-primary" onClick={() => setShowRuleForm(true)}>
                  Add Rule
                </button>
              ) : (
                <div className="rss-rule-form">
                  <h4>Add Download Rule</h4>
                  <div className="setting-row">
                    <label>Rule Name</label>
                    <input
                      type="text"
                      value={newRuleName}
                      onChange={(e) => setNewRuleName(e.target.value)}
                      placeholder="Rule name"
                    />
                  </div>
                  <div className="setting-row">
                    <label>Must Contain</label>
                    <input
                      type="text"
                      value={newRuleMustContain}
                      onChange={(e) => setNewRuleMustContain(e.target.value)}
                      placeholder="Filter text or regex"
                    />
                  </div>
                  <div className="setting-row">
                    <label>Must Not Contain</label>
                    <input
                      type="text"
                      value={newRuleMustNotContain}
                      onChange={(e) => setNewRuleMustNotContain(e.target.value)}
                      placeholder="Exclude filter (optional)"
                    />
                  </div>
                  <div className="setting-row checkbox-row">
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={newRuleUseRegex}
                        onChange={(e) => setNewRuleUseRegex(e.target.checked)}
                      />
                      <span>Use Regular Expression</span>
                    </label>
                  </div>
                  <div className="setting-row">
                    <label>Episode Filter</label>
                    <input
                      type="text"
                      value={newRuleEpisodeFilter}
                      onChange={(e) => setNewRuleEpisodeFilter(e.target.value)}
                      placeholder="e.g., 1-10 or 1,3,5 (optional)"
                    />
                  </div>
                  <div className="setting-row">
                    <label>Category</label>
                    <input
                      type="text"
                      value={newRuleCategory}
                      onChange={(e) => setNewRuleCategory(e.target.value)}
                      placeholder="Category (optional)"
                    />
                  </div>
                  <div className="setting-row">
                    <label>Save Path</label>
                    <div className="dir-select">
                      <input
                        type="text"
                        value={newRuleSavePath}
                        onChange={(e) => setNewRuleSavePath(e.target.value)}
                        placeholder="Override save path (optional)"
                      />
                      <button
                        className="btn-secondary btn-small"
                        onClick={async () => {
                          try {
                            const selected = await open({ directory: true });
                            if (selected) setNewRuleSavePath(selected);
                          } catch (e) {
                            onError(String(e));
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
                        checked={newRuleAddPaused}
                        onChange={(e) => setNewRuleAddPaused(e.target.checked)}
                      />
                      <span>Add as Paused</span>
                    </label>
                  </div>
                  <div className="rss-rule-actions">
                    <button className="btn-secondary" onClick={resetRuleForm}>
                      Cancel
                    </button>
                    <button
                      className="btn-primary"
                      onClick={addRule}
                      disabled={loading || !newRuleName || !newRuleMustContain}
                    >
                      Add Rule
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default RssModal;
