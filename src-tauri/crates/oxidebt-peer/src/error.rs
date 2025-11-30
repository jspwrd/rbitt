use thiserror::Error;

#[derive(Debug, Error)]
pub enum PeerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid handshake: {0}")]
    InvalidHandshake(String),

    #[error("protocol mismatch")]
    ProtocolMismatch,

    #[error("info hash mismatch")]
    InfoHashMismatch,

    #[error("invalid message: {0}")]
    InvalidMessage(String),

    #[error("message too large: {0} bytes")]
    MessageTooLarge(u32),

    #[error("connection closed")]
    ConnectionClosed,

    #[error("timeout")]
    Timeout,

    #[error("peer choked us")]
    Choked,

    #[error("invalid piece index: {0}")]
    InvalidPieceIndex(u32),

    #[error("invalid block offset: {0}")]
    InvalidBlockOffset(u32),

    #[error("invalid block length: {0}")]
    InvalidBlockLength(u32),

    #[error("bitfield length mismatch: expected {expected}, got {actual}")]
    BitfieldLengthMismatch { expected: usize, actual: usize },

    #[error("unexpected message type")]
    UnexpectedMessage,

    #[error("extension not supported: {0}")]
    ExtensionNotSupported(String),
}
