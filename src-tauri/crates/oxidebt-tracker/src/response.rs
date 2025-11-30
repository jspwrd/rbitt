use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerEvent {
    None,
    Started,
    Stopped,
    Completed,
}

impl TrackerEvent {
    pub fn as_str(&self) -> Option<&'static str> {
        match self {
            TrackerEvent::None => None,
            TrackerEvent::Started => Some("started"),
            TrackerEvent::Stopped => Some("stopped"),
            TrackerEvent::Completed => Some("completed"),
        }
    }

    pub fn as_u32(&self) -> u32 {
        match self {
            TrackerEvent::None => 0,
            TrackerEvent::Completed => 1,
            TrackerEvent::Started => 2,
            TrackerEvent::Stopped => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Peer {
    pub addr: SocketAddr,
    pub peer_id: Option<[u8; 20]>,
}

impl Peer {
    pub fn from_compact_v4(data: &[u8]) -> Vec<Self> {
        let mut peers = Vec::new();

        for chunk in data.chunks_exact(6) {
            let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            peers.push(Peer {
                addr: SocketAddr::new(IpAddr::V4(ip), port),
                peer_id: None,
            });
        }

        peers
    }

    pub fn from_compact_v6(data: &[u8]) -> Vec<Self> {
        let mut peers = Vec::new();

        for chunk in data.chunks_exact(18) {
            let ip_bytes: [u8; 16] = chunk[..16].try_into().unwrap();
            let ip = Ipv6Addr::from(ip_bytes);
            let port = u16::from_be_bytes([chunk[16], chunk[17]]);
            peers.push(Peer {
                addr: SocketAddr::new(IpAddr::V6(ip), port),
                peer_id: None,
            });
        }

        peers
    }
}

#[derive(Debug, Clone)]
pub struct AnnounceResponse {
    pub interval: u32,
    pub min_interval: Option<u32>,
    pub complete: Option<u32>,
    pub incomplete: Option<u32>,
    pub peers: Vec<Peer>,
    pub peers6: Vec<Peer>,
    pub warning_message: Option<String>,
    pub tracker_id: Option<String>,
}

impl AnnounceResponse {
    pub fn all_peers(&self) -> Vec<Peer> {
        let mut all = self.peers.clone();
        all.extend(self.peers6.clone());
        all
    }
}

#[derive(Debug, Clone)]
pub struct ScrapeStats {
    pub complete: u32,
    pub incomplete: u32,
    pub downloaded: u32,
}

#[derive(Debug, Clone)]
pub struct ScrapeResponse {
    pub files: Vec<([u8; 20], ScrapeStats)>,
}
