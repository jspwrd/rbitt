mod error;
mod http;
mod response;
mod udp;

pub use error::TrackerError;
pub use http::HttpTracker;
pub use response::{AnnounceResponse, Peer, ScrapeResponse, TrackerEvent};
pub use udp::UdpTracker;

use oxidebt_torrent::InfoHashV1;

pub struct AnnounceParams<'a> {
    pub url: &'a str,
    pub info_hash: &'a InfoHashV1,
    pub peer_id: &'a [u8; 20],
    pub port: u16,
    pub uploaded: u64,
    pub downloaded: u64,
    pub left: u64,
    pub event: TrackerEvent,
}

pub struct TrackerClient {
    http: HttpTracker,
    udp: UdpTracker,
}

impl TrackerClient {
    pub fn new() -> Self {
        Self {
            http: HttpTracker::new(),
            udp: UdpTracker::new(),
        }
    }

    pub async fn announce(
        &self,
        params: AnnounceParams<'_>,
    ) -> Result<AnnounceResponse, TrackerError> {
        if params.url.starts_with("http://") || params.url.starts_with("https://") {
            self.http.announce(&params).await
        } else if params.url.starts_with("udp://") {
            self.udp.announce(&params).await
        } else {
            Err(TrackerError::UnsupportedProtocol(params.url.to_string()))
        }
    }

    pub async fn scrape(
        &self,
        url: &str,
        info_hashes: &[InfoHashV1],
    ) -> Result<ScrapeResponse, TrackerError> {
        if url.starts_with("http://") || url.starts_with("https://") {
            self.http.scrape(url, info_hashes).await
        } else if url.starts_with("udp://") {
            self.udp.scrape(url, info_hashes).await
        } else {
            Err(TrackerError::UnsupportedProtocol(url.to_string()))
        }
    }
}

impl Default for TrackerClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
