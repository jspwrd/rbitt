mod bitfield;
mod choking;
mod connection;
mod error;
pub mod extension;
mod fast;
mod message;
mod peer_id;
mod piece;
mod transport;

pub use bitfield::Bitfield;
pub use choking::{ChokingAlgorithm, ChokingDecision};
pub use connection::{PeerConnection, PeerState};
pub use error::PeerError;
pub use extension::{
    ExtensionHandshake, MetadataMessage, PexMessage, PexPeer, EXTENSION_HANDSHAKE_ID,
    METADATA_PIECE_SIZE, PEX_FLAG_PREFERS_ENCRYPTION, PEX_FLAG_SUPPORTS_UTP, PEX_FLAG_UPLOAD_ONLY,
    UT_METADATA_ID, UT_PEX_ID,
};
pub use fast::{generate_allowed_fast_set, FastExtensionState, DEFAULT_ALLOWED_FAST_COUNT};
pub use message::{ExtensionMessage, Handshake, Message, PROTOCOL_STRING};
pub use peer_id::PeerId;
pub use piece::{Block, BlockRequest, PieceManager};
pub use transport::TransportStream;

#[cfg(test)]
mod tests;
