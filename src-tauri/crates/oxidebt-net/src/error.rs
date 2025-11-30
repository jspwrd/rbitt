use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("upnp error: {0}")]
    Upnp(String),

    #[error("lsd error: {0}")]
    Lsd(String),

    #[error("timeout")]
    Timeout,

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("no mapping available")]
    NoMappingAvailable,
}
