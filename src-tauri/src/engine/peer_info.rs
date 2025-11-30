use oxidebt_constants::{MAX_PEER_RETRY_ATTEMPTS, PEER_RETRY_BASE_DELAY};
use oxidebt_peer::Bitfield;
use std::time::Instant;

/// Information about a connected peer.
pub struct PeerInfo {
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub last_active: Instant,
    pub is_choking_us: bool,
    pub is_interested: bool,
    pub bitfield: Option<Bitfield>,
}

impl PeerInfo {
    pub fn new() -> Self {
        Self {
            download_bytes: 0,
            upload_bytes: 0,
            last_active: Instant::now(),
            is_choking_us: true,
            is_interested: false,
            bitfield: None,
        }
    }
}

impl Default for PeerInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks retry state for failed peer connections.
#[derive(Debug, Clone)]
pub struct FailedPeer {
    pub attempts: u32,
    pub next_retry: Instant,
}

impl FailedPeer {
    pub fn new() -> Self {
        Self {
            attempts: 1,
            next_retry: Instant::now() + PEER_RETRY_BASE_DELAY,
        }
    }

    pub fn increment_attempt(&mut self) {
        self.attempts += 1;
        let backoff_multiplier = 2u64.pow(self.attempts.saturating_sub(1).min(4));
        self.next_retry = Instant::now() + PEER_RETRY_BASE_DELAY * backoff_multiplier as u32;
    }

    pub fn is_ready_for_retry(&self) -> bool {
        Instant::now() >= self.next_retry
    }

    pub fn should_give_up(&self) -> bool {
        self.attempts >= MAX_PEER_RETRY_ATTEMPTS
    }
}

impl Default for FailedPeer {
    fn default() -> Self {
        Self::new()
    }
}
