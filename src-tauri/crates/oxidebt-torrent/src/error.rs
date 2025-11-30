
use oxidebt_bencode::DecodeError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TorrentError {
    #[error("bencode decoding failed: {0}")]
    Bencode(#[from] DecodeError),

    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("invalid field type for '{field}': expected {expected}")]
    InvalidFieldType {
        field: &'static str,
        expected: &'static str,
    },

    #[error("invalid piece length: {0}")]
    InvalidPieceLength(i64),

    #[error("invalid pieces hash: length {0} is not a multiple of 20")]
    InvalidPiecesLength(usize),

    #[error("invalid v2 pieces root: length {0} is not 32")]
    InvalidPiecesRoot(usize),

    #[error("invalid info hash length: {0}")]
    InvalidInfoHashLength(usize),

    #[error("invalid file path: {0}")]
    InvalidFilePath(String),

    #[error("both v1 and v2 info required for hybrid torrent")]
    InvalidHybrid,

    #[error("invalid magnet link: {0}")]
    InvalidMagnetLink(String),

    #[error("merkle tree verification failed")]
    MerkleVerificationFailed,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
