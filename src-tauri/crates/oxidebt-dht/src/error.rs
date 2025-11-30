
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DhtError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bencode error: {0}")]
    Bencode(#[from] oxidebt_bencode::DecodeError),

    #[error("invalid message: {0}")]
    InvalidMessage(String),

    #[error("timeout")]
    Timeout,

    #[error("invalid node id length: {0}")]
    InvalidNodeIdLength(usize),

    #[error("node not found")]
    NodeNotFound,

    #[error("query failed: {0}")]
    QueryFailed(String),

    #[error("rate limited")]
    RateLimited,
}
