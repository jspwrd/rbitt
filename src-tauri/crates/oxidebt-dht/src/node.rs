use crate::error::DhtError;
use rand::RngExt;
use sha1::{Digest, Sha1};
use std::fmt;
use std::net::SocketAddr;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub [u8; 20]);

impl NodeId {
    pub fn generate() -> Self {
        let mut id = [0u8; 20];
        rand::rng().fill(&mut id);
        Self(id)
    }

    pub fn generate_secure(ip: std::net::IpAddr) -> Self {
        let mut hasher = Sha1::new();

        match ip {
            std::net::IpAddr::V4(v4) => {
                let octets = v4.octets();
                let masked = [
                    octets[0] & 0x03,
                    octets[1] & 0x0F,
                    octets[2] & 0x3F,
                    octets[3],
                ];
                hasher.update(masked);
            }
            std::net::IpAddr::V6(v6) => {
                let octets = v6.octets();
                hasher.update(&octets[..8]);
            }
        }

        let r: u8 = rand::random();
        hasher.update([r]);

        let result = hasher.finalize();
        let mut id = [0u8; 20];
        id.copy_from_slice(&result);
        id[0] = (id[0] & 0x07) | (r << 5);
        Self(id)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DhtError> {
        if bytes.len() != 20 {
            return Err(DhtError::InvalidNodeIdLength(bytes.len()));
        }
        let mut id = [0u8; 20];
        id.copy_from_slice(bytes);
        Ok(Self(id))
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    pub fn distance(&self, other: &NodeId) -> [u8; 20] {
        let mut result = [0u8; 20];
        for (i, byte) in result.iter_mut().enumerate() {
            *byte = self.0[i] ^ other.0[i];
        }
        result
    }

    pub fn bucket_index(&self, other: &NodeId) -> usize {
        let dist = self.distance(other);

        for (i, &byte) in dist.iter().enumerate() {
            if byte != 0 {
                return i * 8 + byte.leading_zeros() as usize;
            }
        }

        159
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", hex::encode(&self.0[..4]))
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub last_seen: Instant,
    pub last_query: Option<Instant>,
    pub failed_queries: u32,
}

impl Node {
    pub fn new(id: NodeId, addr: SocketAddr) -> Self {
        Self {
            id,
            addr,
            last_seen: Instant::now(),
            last_query: None,
            failed_queries: 0,
        }
    }

    pub fn touch(&mut self) {
        self.last_seen = Instant::now();
        self.failed_queries = 0;
    }

    pub fn fail(&mut self) {
        self.failed_queries += 1;
    }

    pub fn is_good(&self) -> bool {
        self.failed_queries == 0 && self.last_seen.elapsed().as_secs() < 15 * 60
    }

    pub fn is_bad(&self) -> bool {
        self.failed_queries >= 3 || self.last_seen.elapsed().as_secs() > 60 * 60
    }

    pub fn to_compact(&self) -> Option<[u8; 26]> {
        if let SocketAddr::V4(addr) = self.addr {
            let mut buf = [0u8; 26];
            buf[..20].copy_from_slice(&self.id.0);
            buf[20..24].copy_from_slice(&addr.ip().octets());
            buf[24..26].copy_from_slice(&addr.port().to_be_bytes());
            Some(buf)
        } else {
            None
        }
    }

    pub fn from_compact(data: &[u8]) -> Option<Self> {
        if data.len() != 26 {
            return None;
        }

        let id = NodeId::from_bytes(&data[..20]).ok()?;
        let ip = std::net::Ipv4Addr::new(data[20], data[21], data[22], data[23]);
        let port = u16::from_be_bytes([data[24], data[25]]);
        let addr = SocketAddr::new(std::net::IpAddr::V4(ip), port);

        Some(Node::new(id, addr))
    }
}
