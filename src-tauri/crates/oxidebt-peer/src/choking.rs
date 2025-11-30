use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

pub const UNCHOKE_INTERVAL: Duration = Duration::from_secs(10);

pub const OPTIMISTIC_UNCHOKE_INTERVAL: Duration = Duration::from_secs(30);

pub const MAX_UNCHOKED: usize = 8;

#[derive(Debug, Clone)]
pub struct PeerStats {
    pub addr: SocketAddr,
    pub download_rate: f64,
    pub upload_rate: f64,
    pub is_interested: bool,
    pub is_choking_us: bool,
    pub is_choked_by_us: bool,
    pub last_received: Instant,
    pub is_seed: bool,
}

impl PeerStats {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            download_rate: 0.0,
            upload_rate: 0.0,
            is_interested: false,
            is_choking_us: true,
            is_choked_by_us: true,
            last_received: Instant::now(),
            is_seed: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChokingDecision {
    Choke,
    Unchoke,
    NoChange,
}

pub struct ChokingAlgorithm {
    last_unchoke: Instant,
    last_optimistic: Instant,
    optimistic_peer: Option<SocketAddr>,
    we_are_seeding: bool,
}

impl ChokingAlgorithm {
    pub fn new(
        _max_unchoked: usize,
        _unchoke_interval: Duration,
        _optimistic_interval: Duration,
    ) -> Self {
        Self {
            last_unchoke: Instant::now(),
            last_optimistic: Instant::now(),
            optimistic_peer: None,
            we_are_seeding: false,
        }
    }

    pub fn set_seeding(&mut self, seeding: bool) {
        self.we_are_seeding = seeding;
    }

    pub fn run(
        &mut self,
        peers: &HashMap<SocketAddr, PeerStats>,
    ) -> HashMap<SocketAddr, ChokingDecision> {
        let now = Instant::now();
        let mut decisions = HashMap::new();

        for addr in peers.keys() {
            decisions.insert(*addr, ChokingDecision::NoChange);
        }

        if now.duration_since(self.last_unchoke) < UNCHOKE_INTERVAL {
            return decisions;
        }
        self.last_unchoke = now;

        let mut interested_peers: Vec<_> = peers
            .iter()
            .filter(|(_, stats)| stats.is_interested)
            .collect();

        if interested_peers.is_empty() {
            return decisions;
        }

        if self.we_are_seeding {
            interested_peers.sort_by(|a, b| {
                b.1.upload_rate
                    .partial_cmp(&a.1.upload_rate)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            interested_peers.sort_by(|a, b| {
                b.1.download_rate
                    .partial_cmp(&a.1.download_rate)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        let regular_unchoke_count = MAX_UNCHOKED.saturating_sub(1);

        for (i, (addr, stats)) in interested_peers.iter().enumerate() {
            if i < regular_unchoke_count {
                if stats.is_choked_by_us {
                    decisions.insert(**addr, ChokingDecision::Unchoke);
                }
            } else if Some(**addr) != self.optimistic_peer && !stats.is_choked_by_us {
                decisions.insert(**addr, ChokingDecision::Choke);
            }
        }

        if now.duration_since(self.last_optimistic) >= OPTIMISTIC_UNCHOKE_INTERVAL {
            self.last_optimistic = now;
            self.rotate_optimistic(peers, &mut decisions);
        } else if let Some(opt_addr) = self.optimistic_peer {
            if peers.contains_key(&opt_addr) {
                if peers[&opt_addr].is_choked_by_us {
                    decisions.insert(opt_addr, ChokingDecision::Unchoke);
                }
            } else {
                self.rotate_optimistic(peers, &mut decisions);
            }
        }

        decisions
    }

    fn rotate_optimistic(
        &mut self,
        peers: &HashMap<SocketAddr, PeerStats>,
        decisions: &mut HashMap<SocketAddr, ChokingDecision>,
    ) {
        if let Some(old) = self.optimistic_peer {
            if let Some(stats) = peers.get(&old) {
                if !stats.is_choked_by_us {
                    let dominated = decisions.get(&old) != Some(&ChokingDecision::Unchoke);
                    if dominated {
                        decisions.insert(old, ChokingDecision::Choke);
                    }
                }
            }
        }

        let candidates: Vec<_> = peers
            .iter()
            .filter(|(_, stats)| stats.is_interested && stats.is_choked_by_us)
            .map(|(addr, _)| *addr)
            .collect();

        if candidates.is_empty() {
            self.optimistic_peer = None;
            return;
        }

        let idx = rand::random::<usize>() % candidates.len();
        let new_opt = candidates[idx];

        self.optimistic_peer = Some(new_opt);
        decisions.insert(new_opt, ChokingDecision::Unchoke);
    }

    pub fn optimistic_peer(&self) -> Option<SocketAddr> {
        self.optimistic_peer
    }
}

impl Default for ChokingAlgorithm {
    fn default() -> Self {
        Self::new(MAX_UNCHOKED, UNCHOKE_INTERVAL, OPTIMISTIC_UNCHOKE_INTERVAL)
    }
}
