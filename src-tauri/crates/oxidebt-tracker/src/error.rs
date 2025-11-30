use thiserror::Error;

#[derive(Debug, Error)]
pub enum TrackerError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bencode error: {0}")]
    Bencode(#[from] oxidebt_bencode::DecodeError),

    #[error("url parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("tracker returned failure: {0}")]
    TrackerFailure(String),

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("unsupported protocol: {0}")]
    UnsupportedProtocol(String),

    #[error("timeout")]
    Timeout,

    #[error("connection refused")]
    ConnectionRefused,

    #[error("invalid transaction id")]
    InvalidTransactionId,

    #[error("invalid action in response")]
    InvalidAction,

    #[error("no peers returned")]
    NoPeers,
}
