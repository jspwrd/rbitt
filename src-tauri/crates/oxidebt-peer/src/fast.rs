pub use oxidebt_constants::DEFAULT_ALLOWED_FAST_COUNT;
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::net::IpAddr;

/// Generate the Allowed Fast set per BEP-6.
/// Uses the canonical algorithm: SHA1(IP masked to /24 + info_hash), iterating
/// through 4-byte chunks mod piece_count.
pub fn generate_allowed_fast_set(
    peer_ip: IpAddr,
    info_hash: &[u8; 20],
    piece_count: usize,
    count: usize,
) -> HashSet<u32> {
    let mut allowed_set = HashSet::with_capacity(count);

    if piece_count == 0 {
        return allowed_set;
    }

    // BEP-6: Can't have more allowed fast pieces than total pieces
    let target_count = count.min(piece_count);

    // BEP-6: For IPv4, mask to /24 (set last octet to 0)
    let ip_bytes: Vec<u8> = match peer_ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            vec![octets[0], octets[1], octets[2], 0]
        }
        IpAddr::V6(v6) => {
            // For IPv6, use first 8 bytes (equivalent to /64 masking)
            v6.octets()[..8].to_vec()
        }
    };

    let mut x = Vec::with_capacity(ip_bytes.len() + 20);
    x.extend_from_slice(&ip_bytes);
    x.extend_from_slice(info_hash);

    // Limit iterations to prevent infinite loop when piece_count is small
    let max_iterations = target_count * 10;
    let mut iterations = 0;

    while allowed_set.len() < target_count && iterations < max_iterations {
        let mut hasher = Sha1::new();
        hasher.update(&x);
        let hash = hasher.finalize();
        x = hash.to_vec();

        for chunk in x.chunks_exact(4) {
            if allowed_set.len() >= target_count {
                break;
            }

            let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let index = value % (piece_count as u32);

            allowed_set.insert(index);
        }
        iterations += 1;
    }

    allowed_set
}

#[derive(Debug, Clone)]
pub struct FastExtensionState {
    pub peer_supports_fast: bool,
    pub allowed_fast_set_outgoing: HashSet<u32>,
    pub allowed_fast_set_incoming: HashSet<u32>,
    pub suggested_pieces: Vec<u32>,
}

impl FastExtensionState {
    pub fn new() -> Self {
        Self {
            peer_supports_fast: false,
            allowed_fast_set_outgoing: HashSet::new(),
            allowed_fast_set_incoming: HashSet::new(),
            suggested_pieces: Vec::new(),
        }
    }

    pub fn init_for_peer(&mut self, peer_ip: IpAddr, info_hash: &[u8; 20], piece_count: usize) {
        self.peer_supports_fast = true;
        self.allowed_fast_set_outgoing =
            generate_allowed_fast_set(peer_ip, info_hash, piece_count, DEFAULT_ALLOWED_FAST_COUNT);
    }

    pub fn add_incoming_allowed_fast(&mut self, piece_index: u32) {
        self.allowed_fast_set_incoming.insert(piece_index);
    }

    pub fn add_suggestion(&mut self, piece_index: u32) {
        if !self.suggested_pieces.contains(&piece_index) {
            self.suggested_pieces.push(piece_index);
        }
    }

    pub fn can_request_while_choked(&self, piece_index: u32) -> bool {
        self.allowed_fast_set_incoming.contains(&piece_index)
    }

    pub fn get_outgoing_allowed_fast(&self) -> &HashSet<u32> {
        &self.allowed_fast_set_outgoing
    }

    pub fn clear_suggestions(&mut self) {
        self.suggested_pieces.clear();
    }
}

impl Default for FastExtensionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_allowed_fast_set_generation() {
        let peer_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let info_hash = [0u8; 20];
        let piece_count = 1000;

        let set = generate_allowed_fast_set(peer_ip, &info_hash, piece_count, 10);

        assert_eq!(set.len(), 10);

        for &index in &set {
            assert!(index < piece_count as u32);
        }
    }

    #[test]
    fn test_allowed_fast_set_deterministic() {
        let peer_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let info_hash = [1u8; 20];
        let piece_count = 500;

        let set1 = generate_allowed_fast_set(peer_ip, &info_hash, piece_count, 10);
        let set2 = generate_allowed_fast_set(peer_ip, &info_hash, piece_count, 10);

        assert_eq!(set1, set2);
    }

    #[test]
    fn test_allowed_fast_set_different_ips() {
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let info_hash = [0u8; 20];
        let piece_count = 1000;

        let set1 = generate_allowed_fast_set(ip1, &info_hash, piece_count, 10);
        let set2 = generate_allowed_fast_set(ip2, &info_hash, piece_count, 10);

        assert_ne!(set1, set2);
    }

    #[test]
    fn test_ip_masking() {
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 254));
        let info_hash = [0u8; 20];
        let piece_count = 1000;

        let set1 = generate_allowed_fast_set(ip1, &info_hash, piece_count, 10);
        let set2 = generate_allowed_fast_set(ip2, &info_hash, piece_count, 10);

        assert_eq!(set1, set2);
    }

    #[test]
    fn test_small_piece_count() {
        let peer_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let info_hash = [0u8; 20];

        let set = generate_allowed_fast_set(peer_ip, &info_hash, 3, 10);

        assert!(set.len() <= 3);
    }
}
