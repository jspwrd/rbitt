use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("torrent error: {0}")]
    Torrent(#[from] oxidebt_torrent::TorrentError),

    #[error("dht error: {0}")]
    Dht(#[from] oxidebt_dht::DhtError),

    #[error("tracker error: {0}")]
    Tracker(#[from] oxidebt_tracker::TrackerError),

    #[error("disk error: {0}")]
    Disk(#[from] oxidebt_disk::DiskError),

    #[error("peer error: {0}")]
    Peer(#[from] oxidebt_peer::PeerError),

    #[error("torrent not found: {0}")]
    NotFound(String),

    #[error("torrent already added: {0}")]
    Duplicate(String),

    #[error("BitTorrent v2-only torrents are not supported yet (no v1 info hash)")]
    UnsupportedV2Only,

    #[allow(dead_code)]
    #[error("global connection limit reached (limit: {limit})")]
    TooManyConnections { limit: usize },
}
