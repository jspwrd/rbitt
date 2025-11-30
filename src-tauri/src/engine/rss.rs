//! RSS feed support with download rules.

#![allow(dead_code)]

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};

/// An RSS feed configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssFeed {
    /// Unique identifier
    pub id: String,
    /// Feed URL
    pub url: String,
    /// Display name
    pub name: String,
    /// Whether this feed is enabled
    pub enabled: bool,
    /// Refresh interval in seconds
    pub refresh_interval: u64,
    /// Last refresh timestamp (unix epoch)
    pub last_refresh: Option<u64>,
    /// Last error message if any
    pub last_error: Option<String>,
}

/// An RSS download rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssDownloadRule {
    /// Unique identifier
    pub id: String,
    /// Rule name
    pub name: String,
    /// Whether this rule is enabled
    pub enabled: bool,
    /// Must contain filter (regex if use_regex is true)
    pub must_contain: String,
    /// Must not contain filter (regex if use_regex is true)
    pub must_not_contain: String,
    /// Whether to use regex for filters
    pub use_regex: bool,
    /// Episode filter (e.g., "1-10" or "1,3,5")
    pub episode_filter: Option<String>,
    /// Feeds this rule applies to (empty = all feeds)
    pub affected_feeds: Vec<String>,
    /// Category to assign
    pub category: Option<String>,
    /// Tags to assign
    pub tags: Vec<String>,
    /// Save path override
    pub save_path: Option<String>,
    /// Whether to add as paused
    pub add_paused: bool,
    /// Last match timestamp
    pub last_match: Option<u64>,
}

/// An RSS item from a feed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssItem {
    /// Item title
    pub title: String,
    /// Torrent URL or magnet link
    pub torrent_url: String,
    /// Item link (info page)
    pub link: Option<String>,
    /// Publication date (unix epoch)
    pub pub_date: Option<u64>,
    /// Description
    pub description: Option<String>,
    /// Whether this item has been downloaded
    pub is_downloaded: bool,
}

/// Events from RSS processing
#[derive(Debug, Clone)]
pub struct RssMatchEvent {
    /// Feed ID that matched
    pub feed_id: String,
    /// Rule ID that matched
    pub rule_id: String,
    /// Item that matched
    pub item: RssItem,
    /// Category to assign
    pub category: Option<String>,
    /// Tags to assign
    pub tags: Vec<String>,
    /// Save path override
    pub save_path: Option<String>,
    /// Whether to add as paused
    pub add_paused: bool,
}

/// RSS feed manager
pub struct RssManager {
    /// Configured feeds
    feeds: Arc<RwLock<HashMap<String, RssFeed>>>,
    /// Download rules
    rules: Arc<RwLock<HashMap<String, RssDownloadRule>>>,
    /// Cached feed items
    feed_items: Arc<RwLock<HashMap<String, Vec<RssItem>>>>,
    /// Downloaded item hashes (to avoid re-downloading)
    downloaded: Arc<RwLock<std::collections::HashSet<String>>>,
    /// Event channel
    event_tx: mpsc::UnboundedSender<RssMatchEvent>,
    /// HTTP client
    client: reqwest::Client,
    /// Shutdown signal
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl RssManager {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<RssMatchEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        (
            Self {
                feeds: Arc::new(RwLock::new(HashMap::new())),
                rules: Arc::new(RwLock::new(HashMap::new())),
                feed_items: Arc::new(RwLock::new(HashMap::new())),
                downloaded: Arc::new(RwLock::new(std::collections::HashSet::new())),
                event_tx,
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .unwrap_or_default(),
                shutdown_tx,
            },
            event_rx,
        )
    }

    /// Start the RSS refresh loop
    pub fn start(&self) {
        let feeds = self.feeds.clone();
        let rules = self.rules.clone();
        let feed_items = self.feed_items.clone();
        let downloaded = self.downloaded.clone();
        let event_tx = self.event_tx.clone();
        let client = self.client.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let check_interval = Duration::from_secs(60);

            loop {
                tokio::select! {
                    _ = tokio::time::sleep(check_interval) => {
                        Self::refresh_feeds(&feeds, &rules, &feed_items, &downloaded, &event_tx, &client).await;
                    }
                    result = shutdown_rx.recv() => {
                        // Only shutdown on successful receive, ignore errors (like Lagged)
                        if result.is_ok() {
                            tracing::info!("RSS manager shutting down");
                            break;
                        }
                    }
                }
            }
        });
    }

    async fn refresh_feeds(
        feeds: &Arc<RwLock<HashMap<String, RssFeed>>>,
        rules: &Arc<RwLock<HashMap<String, RssDownloadRule>>>,
        feed_items: &Arc<RwLock<HashMap<String, Vec<RssItem>>>>,
        downloaded: &Arc<RwLock<std::collections::HashSet<String>>>,
        event_tx: &mpsc::UnboundedSender<RssMatchEvent>,
        client: &reqwest::Client,
    ) {
        let feeds_to_check: Vec<RssFeed> = {
            let guard = feeds.read().await;
            guard
                .values()
                .filter(|f| f.enabled)
                .filter(|f| {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    f.last_refresh
                        .map(|last| now - last >= f.refresh_interval)
                        .unwrap_or(true)
                })
                .cloned()
                .collect()
        };

        for feed in feeds_to_check {
            match Self::fetch_feed(&feed.url, client).await {
                Ok(items) => {
                    // Update last refresh
                    {
                        let mut guard = feeds.write().await;
                        if let Some(f) = guard.get_mut(&feed.id) {
                            f.last_refresh = Some(
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            );
                            f.last_error = None;
                        }
                    }

                    // Store items
                    {
                        let mut guard = feed_items.write().await;
                        guard.insert(feed.id.clone(), items.clone());
                    }

                    // Check rules
                    let rules_snapshot: Vec<RssDownloadRule> = {
                        let guard = rules.read().await;
                        guard
                            .values()
                            .filter(|r| r.enabled)
                            .filter(|r| {
                                r.affected_feeds.is_empty() || r.affected_feeds.contains(&feed.id)
                            })
                            .cloned()
                            .collect()
                    };

                    for item in items {
                        // Check if already downloaded
                        let item_hash = format!("{}:{}", feed.id, item.torrent_url);
                        {
                            let guard = downloaded.read().await;
                            if guard.contains(&item_hash) {
                                continue;
                            }
                        }

                        // Check each rule
                        for rule in &rules_snapshot {
                            if Self::item_matches_rule(&item, rule) {
                                // Mark as downloaded
                                {
                                    let mut guard = downloaded.write().await;
                                    guard.insert(item_hash.clone());
                                }

                                // Send event
                                let event = RssMatchEvent {
                                    feed_id: feed.id.clone(),
                                    rule_id: rule.id.clone(),
                                    item: item.clone(),
                                    category: rule.category.clone(),
                                    tags: rule.tags.clone(),
                                    save_path: rule.save_path.clone(),
                                    add_paused: rule.add_paused,
                                };

                                if event_tx.send(event).is_err() {
                                    tracing::warn!("RSS event receiver dropped");
                                    return;
                                }

                                // Update rule last match
                                {
                                    let mut guard = rules.write().await;
                                    if let Some(r) = guard.get_mut(&rule.id) {
                                        r.last_match = Some(
                                            SystemTime::now()
                                                .duration_since(UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_secs(),
                                        );
                                    }
                                }

                                break; // Only match first rule
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch RSS feed {}: {}", feed.url, e);
                    let mut guard = feeds.write().await;
                    if let Some(f) = guard.get_mut(&feed.id) {
                        f.last_error = Some(e.to_string());
                        f.last_refresh = Some(
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        );
                    }
                }
            }
        }
    }

    async fn fetch_feed(url: &str, client: &reqwest::Client) -> Result<Vec<RssItem>, String> {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        Self::parse_rss(&text)
    }

    fn parse_rss(xml: &str) -> Result<Vec<RssItem>, String> {
        // Simple RSS/Atom parser - in production, use a proper crate like `feed-rs`
        let mut items = Vec::new();

        // Try to find items (RSS) or entries (Atom)
        let item_regex = Regex::new(r"<item[^>]*>([\s\S]*?)</item>|<entry[^>]*>([\s\S]*?)</entry>")
            .map_err(|e| e.to_string())?;

        for cap in item_regex.captures_iter(xml) {
            let content = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str())
                .unwrap_or("");

            let title = Self::extract_tag(content, "title").unwrap_or_default();
            let link = Self::extract_tag(content, "link");
            let description = Self::extract_tag(content, "description")
                .or_else(|| Self::extract_tag(content, "summary"));

            // Find torrent URL - check enclosure, link, or magnetURI
            let torrent_url = Self::extract_enclosure_url(content)
                .or_else(|| Self::extract_tag(content, "magnetURI"))
                .or_else(|| {
                    // Check if link is a torrent/magnet
                    link.as_ref().and_then(|l| {
                        if l.ends_with(".torrent") || l.starts_with("magnet:") {
                            Some(l.clone())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default();

            if !torrent_url.is_empty() {
                items.push(RssItem {
                    title,
                    torrent_url,
                    link,
                    pub_date: None, // Would need proper date parsing
                    description,
                    is_downloaded: false,
                });
            }
        }

        Ok(items)
    }

    fn extract_tag(content: &str, tag: &str) -> Option<String> {
        let pattern = format!(r"<{tag}[^>]*>(?:<!\[CDATA\[)?([\s\S]*?)(?:\]\]>)?</{tag}>");
        Regex::new(&pattern)
            .ok()
            .and_then(|re| re.captures(content))
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().trim().to_string())
    }

    fn extract_enclosure_url(content: &str) -> Option<String> {
        let pattern = r#"<enclosure[^>]*url=["']([^"']+)["'][^>]*/?\s*>"#;
        Regex::new(pattern)
            .ok()
            .and_then(|re| re.captures(content))
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
    }

    fn item_matches_rule(item: &RssItem, rule: &RssDownloadRule) -> bool {
        let title = &item.title;

        // Check must contain
        if !rule.must_contain.is_empty() {
            let matches = if rule.use_regex {
                Regex::new(&rule.must_contain)
                    .map(|re| re.is_match(title))
                    .unwrap_or(false)
            } else {
                title
                    .to_lowercase()
                    .contains(&rule.must_contain.to_lowercase())
            };
            if !matches {
                return false;
            }
        }

        // Check must not contain
        if !rule.must_not_contain.is_empty() {
            let matches = if rule.use_regex {
                Regex::new(&rule.must_not_contain)
                    .map(|re| re.is_match(title))
                    .unwrap_or(false)
            } else {
                title
                    .to_lowercase()
                    .contains(&rule.must_not_contain.to_lowercase())
            };
            if matches {
                return false;
            }
        }

        // Check episode filter (simplified - would need full implementation)
        if let Some(ref ep_filter) = rule.episode_filter {
            if !Self::matches_episode_filter(title, ep_filter) {
                return false;
            }
        }

        true
    }

    fn matches_episode_filter(title: &str, filter: &str) -> bool {
        // Extract episode number from title (simplified)
        let ep_regex = Regex::new(r"[Ss](\d+)[Ee](\d+)|(\d+)x(\d+)|[Ee](\d+)").ok();
        let episode = ep_regex.and_then(|re| {
            re.captures(title).and_then(|cap| {
                cap.get(2)
                    .or_else(|| cap.get(4))
                    .or_else(|| cap.get(5))
                    .and_then(|m| m.as_str().parse::<u32>().ok())
            })
        });

        let Some(ep) = episode else {
            return true; // No episode number found, allow
        };

        // Parse filter (e.g., "1-10" or "1,3,5")
        for part in filter.split(',') {
            let part = part.trim();
            if part.contains('-') {
                let bounds: Vec<&str> = part.split('-').collect();
                if bounds.len() == 2 {
                    if let (Ok(start), Ok(end)) = (
                        bounds[0].trim().parse::<u32>(),
                        bounds[1].trim().parse::<u32>(),
                    ) {
                        if ep >= start && ep <= end {
                            return true;
                        }
                    }
                }
            } else if let Ok(single) = part.parse::<u32>() {
                if ep == single {
                    return true;
                }
            }
        }

        false
    }

    // CRUD operations for feeds
    pub async fn add_feed(&self, feed: RssFeed) {
        self.feeds.write().await.insert(feed.id.clone(), feed);
    }

    pub async fn remove_feed(&self, id: &str) -> Option<RssFeed> {
        self.feeds.write().await.remove(id)
    }

    pub async fn get_feeds(&self) -> Vec<RssFeed> {
        self.feeds.read().await.values().cloned().collect()
    }

    pub async fn get_feed(&self, id: &str) -> Option<RssFeed> {
        self.feeds.read().await.get(id).cloned()
    }

    pub async fn update_feed(&self, feed: RssFeed) -> bool {
        let mut guard = self.feeds.write().await;
        if guard.contains_key(&feed.id) {
            guard.insert(feed.id.clone(), feed);
            true
        } else {
            false
        }
    }

    // CRUD operations for rules
    pub async fn add_rule(&self, rule: RssDownloadRule) {
        self.rules.write().await.insert(rule.id.clone(), rule);
    }

    pub async fn remove_rule(&self, id: &str) -> Option<RssDownloadRule> {
        self.rules.write().await.remove(id)
    }

    pub async fn get_rules(&self) -> Vec<RssDownloadRule> {
        self.rules.read().await.values().cloned().collect()
    }

    pub async fn get_rule(&self, id: &str) -> Option<RssDownloadRule> {
        self.rules.read().await.get(id).cloned()
    }

    pub async fn update_rule(&self, rule: RssDownloadRule) -> bool {
        let mut guard = self.rules.write().await;
        if guard.contains_key(&rule.id) {
            guard.insert(rule.id.clone(), rule);
            true
        } else {
            false
        }
    }

    /// Get items for a specific feed
    pub async fn get_feed_items(&self, feed_id: &str) -> Vec<RssItem> {
        self.feed_items
            .read()
            .await
            .get(feed_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Force refresh a specific feed
    pub async fn refresh_feed(&self, feed_id: &str) -> Result<(), String> {
        let feed = {
            let guard = self.feeds.read().await;
            guard.get(feed_id).cloned()
        };

        if let Some(feed) = feed {
            match Self::fetch_feed(&feed.url, &self.client).await {
                Ok(items) => {
                    {
                        let mut guard = self.feeds.write().await;
                        if let Some(f) = guard.get_mut(feed_id) {
                            f.last_refresh = Some(
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            );
                            f.last_error = None;
                        }
                    }
                    {
                        let mut guard = self.feed_items.write().await;
                        guard.insert(feed_id.to_string(), items);
                    }
                    Ok(())
                }
                Err(e) => {
                    {
                        let mut guard = self.feeds.write().await;
                        if let Some(f) = guard.get_mut(feed_id) {
                            f.last_error = Some(e.clone());
                        }
                    }
                    Err(e)
                }
            }
        } else {
            Err("Feed not found".to_string())
        }
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}
