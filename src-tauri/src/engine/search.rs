//! Search engine with plugin support.
//!
//! Plugins are Python scripts that implement a simple interface to search
//! torrent sites. This is compatible with qBittorrent's search plugin format.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

/// Validates that a filename is safe for use in plugin paths.
/// Returns an error if the filename contains path traversal or unsafe characters.
fn validate_plugin_filename(filename: &str) -> Result<(), String> {
    // Check for empty filename
    if filename.is_empty() {
        return Err("Empty filename".to_string());
    }

    // Check for path separators (both Unix and Windows)
    if filename.contains('/') || filename.contains('\\') {
        return Err(format!("Filename contains path separators: {}", filename));
    }

    // Check for parent directory references
    if filename == ".." || filename.starts_with("..") {
        return Err(format!("Filename contains path traversal: {}", filename));
    }

    // Check for hidden files (starting with .)
    if filename.starts_with('.') {
        return Err(format!("Hidden files not allowed: {}", filename));
    }

    // Must have .py extension
    if !filename.ends_with(".py") {
        return Err(format!("Plugin must have .py extension: {}", filename));
    }

    // Only allow alphanumeric, underscore, hyphen, and .py extension
    let name_without_ext = &filename[..filename.len() - 3];
    if !name_without_ext
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "Filename contains invalid characters: {}",
            filename
        ));
    }

    Ok(())
}

/// A search plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPlugin {
    /// Plugin name (e.g., "piratebay")
    pub name: String,
    /// Display name
    pub display_name: String,
    /// Plugin version
    pub version: String,
    /// Plugin file path
    pub path: PathBuf,
    /// Whether this plugin is enabled
    pub enabled: bool,
    /// Supported categories
    pub categories: Vec<String>,
    /// Plugin URL (for updates)
    pub url: Option<String>,
}

/// A search result from a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Result name/title
    pub name: String,
    /// Torrent URL or magnet link
    pub download_link: String,
    /// Size in bytes
    pub size: u64,
    /// Number of seeders
    pub seeders: i32,
    /// Number of leechers
    pub leechers: i32,
    /// Plugin that returned this result
    pub plugin: String,
    /// Info page URL
    pub description_link: Option<String>,
    /// Publication date (unix epoch)
    pub pub_date: Option<u64>,
}

/// Search status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchStatus {
    /// Search is running
    Running,
    /// Search completed successfully
    Completed,
    /// Search was stopped
    Stopped,
    /// Search failed with error
    Failed,
}

/// An active search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchJob {
    /// Search ID
    pub id: String,
    /// Search query
    pub query: String,
    /// Plugins being searched
    pub plugins: Vec<String>,
    /// Category filter
    pub category: Option<String>,
    /// Current status
    pub status: SearchStatus,
    /// Results found so far
    pub results: Vec<SearchResult>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Search engine manager
pub struct SearchEngine {
    /// Installed plugins
    plugins: Arc<RwLock<HashMap<String, SearchPlugin>>>,
    /// Active searches
    searches: Arc<RwLock<HashMap<String, SearchJob>>>,
    /// Plugin directory
    plugin_dir: PathBuf,
    /// Python executable path
    python_path: String,
}

impl SearchEngine {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            searches: Arc::new(RwLock::new(HashMap::new())),
            plugin_dir,
            python_path: "python3".to_string(),
        }
    }

    /// Set the Python executable path
    pub fn set_python_path(&mut self, path: String) {
        self.python_path = path;
    }

    /// Scan plugin directory and load plugins
    pub async fn load_plugins(&self) -> Result<usize, String> {
        if !self.plugin_dir.exists() {
            tokio::fs::create_dir_all(&self.plugin_dir)
                .await
                .map_err(|e| format!("Failed to create plugin directory: {}", e))?;
            return Ok(0);
        }

        let mut entries = tokio::fs::read_dir(&self.plugin_dir)
            .await
            .map_err(|e| format!("Failed to read plugin directory: {}", e))?;

        let mut count = 0;
        let mut plugins = self.plugins.write().await;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("py") {
                if let Some(plugin) = self.parse_plugin_file(&path).await {
                    plugins.insert(plugin.name.clone(), plugin);
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    async fn parse_plugin_file(&self, path: &PathBuf) -> Option<SearchPlugin> {
        let content = tokio::fs::read_to_string(path).await.ok()?;

        // Parse plugin metadata from comments/docstrings
        // qBittorrent plugin format uses specific variable names
        let name = Self::extract_plugin_var(&content, "name")?;
        let version =
            Self::extract_plugin_var(&content, "version").unwrap_or_else(|| "1.0".to_string());
        let url = Self::extract_plugin_var(&content, "url");
        let display_name =
            Self::extract_plugin_var(&content, "display_name").unwrap_or_else(|| name.clone());

        // Parse supported categories
        let categories = Self::extract_plugin_var(&content, "supported_categories")
            .map(|s| {
                s.split(',')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| vec!["all".to_string()]);

        Some(SearchPlugin {
            name,
            display_name,
            version,
            path: path.clone(),
            enabled: true,
            categories,
            url,
        })
    }

    fn extract_plugin_var(content: &str, var_name: &str) -> Option<String> {
        // Look for patterns like: name = "value" or name = 'value'
        let patterns = [
            format!(r#"{}\s*=\s*["']([^"']+)["']"#, var_name),
            format!(r#"#\s*{}\s*:\s*(.+)"#, var_name), // Comment style: # name: value
        ];

        for pattern in patterns {
            if let Ok(re) = regex::Regex::new(&pattern) {
                if let Some(cap) = re.captures(content) {
                    if let Some(m) = cap.get(1) {
                        return Some(m.as_str().trim().to_string());
                    }
                }
            }
        }

        None
    }

    /// Install a plugin from a URL
    pub async fn install_plugin(&self, url: &str) -> Result<SearchPlugin, String> {
        let response = reqwest::get(url)
            .await
            .map_err(|e| format!("Failed to download plugin: {}", e))?;

        let content = response
            .text()
            .await
            .map_err(|e| format!("Failed to read plugin content: {}", e))?;

        // Extract filename from URL or generate one
        let filename = url
            .split('/')
            .last()
            .filter(|s| s.ends_with(".py"))
            .unwrap_or("plugin.py");

        // Validate filename to prevent path traversal
        validate_plugin_filename(filename)?;

        let path = self.plugin_dir.join(filename);

        tokio::fs::write(&path, &content)
            .await
            .map_err(|e| format!("Failed to save plugin: {}", e))?;

        let plugin = self
            .parse_plugin_file(&path)
            .await
            .ok_or_else(|| "Failed to parse plugin file".to_string())?;

        self.plugins
            .write()
            .await
            .insert(plugin.name.clone(), plugin.clone());

        Ok(plugin)
    }

    /// Install a plugin from a local file
    pub async fn install_plugin_file(&self, source_path: &PathBuf) -> Result<SearchPlugin, String> {
        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| "Invalid file path".to_string())?;

        // Validate filename to prevent path traversal
        validate_plugin_filename(filename)?;

        let dest_path = self.plugin_dir.join(filename);

        tokio::fs::copy(source_path, &dest_path)
            .await
            .map_err(|e| format!("Failed to copy plugin: {}", e))?;

        let plugin = self
            .parse_plugin_file(&dest_path)
            .await
            .ok_or_else(|| "Failed to parse plugin file".to_string())?;

        self.plugins
            .write()
            .await
            .insert(plugin.name.clone(), plugin.clone());

        Ok(plugin)
    }

    /// Remove a plugin
    pub async fn remove_plugin(&self, name: &str) -> Result<(), String> {
        let plugin = {
            let mut guard = self.plugins.write().await;
            guard.remove(name)
        };

        if let Some(plugin) = plugin {
            tokio::fs::remove_file(&plugin.path)
                .await
                .map_err(|e| format!("Failed to delete plugin file: {}", e))?;
            Ok(())
        } else {
            Err("Plugin not found".to_string())
        }
    }

    /// Enable or disable a plugin
    pub async fn set_plugin_enabled(&self, name: &str, enabled: bool) -> bool {
        let mut guard = self.plugins.write().await;
        if let Some(plugin) = guard.get_mut(name) {
            plugin.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Get all plugins
    pub async fn get_plugins(&self) -> Vec<SearchPlugin> {
        self.plugins.read().await.values().cloned().collect()
    }

    /// Start a search
    pub async fn start_search(
        &self,
        query: &str,
        plugins: Vec<String>,
        category: Option<String>,
    ) -> String {
        let search_id = uuid::Uuid::new_v4().to_string();

        // Get enabled plugins
        let plugin_list: Vec<SearchPlugin> = {
            let guard = self.plugins.read().await;
            if plugins.is_empty() || plugins.contains(&"all".to_string()) {
                guard.values().filter(|p| p.enabled).cloned().collect()
            } else {
                plugins
                    .iter()
                    .filter_map(|name| guard.get(name))
                    .filter(|p| p.enabled)
                    .cloned()
                    .collect()
            }
        };

        let plugin_names: Vec<String> = plugin_list.iter().map(|p| p.name.clone()).collect();

        let job = SearchJob {
            id: search_id.clone(),
            query: query.to_string(),
            plugins: plugin_names,
            category: category.clone(),
            status: SearchStatus::Running,
            results: Vec::new(),
            error: None,
        };

        self.searches.write().await.insert(search_id.clone(), job);

        // Run search in background
        let searches = self.searches.clone();
        let search_id_clone = search_id.clone();
        let query = query.to_string();
        let python_path = self.python_path.clone();

        tokio::spawn(async move {
            let mut all_results = Vec::new();
            let mut has_error = false;
            let mut error_msg = None;

            for plugin in plugin_list {
                match Self::run_plugin_search(&python_path, &plugin, &query, category.as_deref())
                    .await
                {
                    Ok(results) => {
                        all_results.extend(results);
                    }
                    Err(e) => {
                        tracing::warn!("Search plugin {} failed: {}", plugin.name, e);
                        has_error = true;
                        if error_msg.is_none() {
                            error_msg = Some(e);
                        }
                    }
                }

                // Update results incrementally
                {
                    let mut guard = searches.write().await;
                    if let Some(job) = guard.get_mut(&search_id_clone) {
                        job.results = all_results.clone();
                    }
                }
            }

            // Update final status
            {
                let mut guard = searches.write().await;
                if let Some(job) = guard.get_mut(&search_id_clone) {
                    job.results = all_results;
                    job.status = if has_error && job.results.is_empty() {
                        job.error = error_msg;
                        SearchStatus::Failed
                    } else {
                        SearchStatus::Completed
                    };
                }
            }
        });

        search_id
    }

    async fn run_plugin_search(
        python_path: &str,
        plugin: &SearchPlugin,
        query: &str,
        category: Option<&str>,
    ) -> Result<Vec<SearchResult>, String> {
        // qBittorrent plugin interface:
        // python plugin.py "query" [category]
        // Output format: link|name|size|seeds|leech|engine_url|desc_link|pub_date

        let mut cmd = Command::new(python_path);
        cmd.arg(&plugin.path);
        cmd.arg(query);
        if let Some(cat) = category {
            cmd.arg(cat);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn plugin process: {}", e))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to get stdout".to_string())?;

        let mut reader = BufReader::new(stdout).lines();
        let mut results = Vec::new();

        while let Ok(Some(line)) = reader.next_line().await {
            if let Some(result) = Self::parse_plugin_output(&line, &plugin.name) {
                results.push(result);
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Failed to wait for plugin: {}", e))?;

        if !status.success() && results.is_empty() {
            return Err(format!("Plugin exited with status: {}", status));
        }

        Ok(results)
    }

    fn parse_plugin_output(line: &str, plugin_name: &str) -> Option<SearchResult> {
        // Format: link|name|size|seeds|leech|engine_url|desc_link|pub_date
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 5 {
            return None;
        }

        let download_link = parts[0].to_string();
        let name = parts[1].to_string();
        let size = Self::parse_size(parts[2]);
        let seeders = parts[3].parse::<i32>().unwrap_or(-1);
        let leechers = parts[4].parse::<i32>().unwrap_or(-1);
        let description_link = parts
            .get(6)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let pub_date = parts.get(7).and_then(|s| s.parse::<u64>().ok());

        Some(SearchResult {
            name,
            download_link,
            size,
            seeders,
            leechers,
            plugin: plugin_name.to_string(),
            description_link,
            pub_date,
        })
    }

    fn parse_size(s: &str) -> u64 {
        // Parse size like "1.5 GB", "500 MB", "1024" (bytes)
        let s = s.trim().to_uppercase();

        if let Ok(bytes) = s.parse::<u64>() {
            return bytes;
        }

        let multipliers = [
            ("TB", 1024u64 * 1024 * 1024 * 1024),
            ("GB", 1024 * 1024 * 1024),
            ("MB", 1024 * 1024),
            ("KB", 1024),
            ("B", 1),
        ];

        for (suffix, mult) in multipliers {
            if s.ends_with(suffix) {
                let num_part = s.trim_end_matches(suffix).trim();
                if let Ok(num) = num_part.parse::<f64>() {
                    return (num * mult as f64) as u64;
                }
            }
        }

        0
    }

    /// Stop a search
    pub async fn stop_search(&self, search_id: &str) -> bool {
        let mut guard = self.searches.write().await;
        if let Some(job) = guard.get_mut(search_id) {
            if job.status == SearchStatus::Running {
                job.status = SearchStatus::Stopped;
                return true;
            }
        }
        false
    }

    /// Get search status
    pub async fn get_search(&self, search_id: &str) -> Option<SearchJob> {
        self.searches.read().await.get(search_id).cloned()
    }

    /// Get search results
    pub async fn get_search_results(&self, search_id: &str) -> Vec<SearchResult> {
        self.searches
            .read()
            .await
            .get(search_id)
            .map(|j| j.results.clone())
            .unwrap_or_default()
    }

    /// Delete a search
    pub async fn delete_search(&self, search_id: &str) -> bool {
        self.searches.write().await.remove(search_id).is_some()
    }

    /// Get all active searches
    pub async fn get_searches(&self) -> Vec<SearchJob> {
        self.searches.read().await.values().cloned().collect()
    }
}
