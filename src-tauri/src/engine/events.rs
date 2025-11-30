use oxidebt_peer::Bitfield;
use std::net::SocketAddr;

/// Events sent from peer connection tasks to the main engine.
#[derive(Debug)]
pub enum PeerEvent {
    Connected {
        torrent_hash: String,
        peer_addr: SocketAddr,
    },
    Disconnected {
        torrent_hash: String,
        peer_addr: SocketAddr,
    },
    PieceCompleted {
        torrent_hash: String,
        piece_index: u32,
    },
    BlockReceived {
        torrent_hash: String,
        size: u64,
    },
    BlockSent {
        torrent_hash: String,
        size: u64,
    },
    PeerBitfield {
        torrent_hash: String,
        peer_addr: SocketAddr,
        bitfield: Bitfield,
    },
    PeerState {
        torrent_hash: String,
        peer_addr: SocketAddr,
        is_choking_us: bool,
        is_interested: bool,
    },
    NewPeers {
        torrent_hash: String,
        peers: Vec<SocketAddr>,
    },
    PeerHave {
        torrent_hash: String,
        peer_addr: SocketAddr,
        piece_index: u32,
    },
}
