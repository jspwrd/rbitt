use std::time::Instant;

/// Current state of a tracker.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackerState {
    NotContacted,
    Updating,
    Working,
    Error,
}

impl TrackerState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrackerState::NotContacted => "Not contacted",
            TrackerState::Updating => "Updating...",
            TrackerState::Working => "Working",
            TrackerState::Error => "Error",
        }
    }
}

/// Information about a tracker for a torrent.
#[derive(Debug, Clone)]
pub struct TrackerInfo {
    pub url: String,
    pub status: TrackerState,
    pub peers: u32,
    pub seeds: u32,
    pub leechers: u32,
    pub last_announce: Option<Instant>,
    pub next_announce: Option<Instant>,
    pub message: Option<String>,
}

impl TrackerInfo {
    pub fn new(url: String) -> Self {
        Self {
            url,
            status: TrackerState::NotContacted,
            peers: 0,
            seeds: 0,
            leechers: 0,
            last_announce: None,
            next_announce: None,
            message: None,
        }
    }
}
