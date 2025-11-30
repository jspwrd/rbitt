
use crate::error::TrackerError;
use crate::response::{AnnounceResponse, Peer, ScrapeResponse, ScrapeStats};
use crate::AnnounceParams;
use bytes::{Buf, BufMut, BytesMut};
use oxidebt_constants::{
    UDP_ACTION_ANNOUNCE, UDP_ACTION_CONNECT, UDP_ACTION_ERROR, UDP_ACTION_SCRAPE,
    UDP_TRACKER_PROTOCOL_ID, UDP_TRACKER_REQUEST_TIMEOUT,
};
use oxidebt_torrent::InfoHashV1;
use parking_lot::RwLock;
use rand::Rng;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout;
use url::Url;

/// BEP-15: Maximum number of retries before giving up
const UDP_MAX_RETRIES: u32 = 8;

/// BEP-15: Connection IDs are valid for at least 1 minute
/// We use 55 seconds to be conservative
const CONNECTION_ID_TTL: Duration = Duration::from_secs(55);

/// Cached connection ID with expiry time
struct CachedConnection {
    connection_id: i64,
    expires_at: Instant,
}

impl CachedConnection {
    fn new(connection_id: i64) -> Self {
        Self {
            connection_id,
            expires_at: Instant::now() + CONNECTION_ID_TTL,
        }
    }

    fn is_valid(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

pub struct UdpTracker {
    /// Cache of connection IDs per tracker address
    connection_cache: RwLock<HashMap<SocketAddr, CachedConnection>>,
}

impl UdpTracker {
    pub fn new() -> Self {
        Self {
            connection_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Get a cached connection ID or establish a new connection
    async fn get_connection_id(
        &self,
        socket: &UdpSocket,
        addr: SocketAddr,
    ) -> Result<i64, TrackerError> {
        // Check cache first
        {
            let cache = self.connection_cache.read();
            if let Some(cached) = cache.get(&addr) {
                if cached.is_valid() {
                    return Ok(cached.connection_id);
                }
            }
        }

        // Cache miss or expired, establish new connection
        let connection_id = self.connect(socket).await?;

        // Store in cache
        {
            let mut cache = self.connection_cache.write();
            cache.insert(addr, CachedConnection::new(connection_id));
        }

        Ok(connection_id)
    }

    /// Invalidate cached connection for a tracker (called on errors)
    #[allow(dead_code)]
    fn invalidate_connection(&self, addr: &SocketAddr) {
        let mut cache = self.connection_cache.write();
        cache.remove(addr);
    }

    pub async fn announce(
        &self,
        params: &AnnounceParams<'_>,
    ) -> Result<AnnounceResponse, TrackerError> {
        let addr = self.resolve_tracker_addr(params.url).await?;
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(addr).await?;

        // Use cached connection ID if available
        let connection_id = self.get_connection_id(&socket, addr).await?;

        let transaction_id: u32 = rand::thread_rng().gen();

        let mut request = BytesMut::with_capacity(98);
        request.put_i64(connection_id);
        request.put_u32(UDP_ACTION_ANNOUNCE);
        request.put_u32(transaction_id);
        request.put_slice(params.info_hash.as_bytes());
        request.put_slice(params.peer_id);
        request.put_u64(params.downloaded);
        request.put_u64(params.left);
        request.put_u64(params.uploaded);
        request.put_u32(params.event.as_u32());
        request.put_u32(0); // IP address (0 = use source)
        request.put_u32(rand::thread_rng().gen()); // Key
        request.put_i32(-1); // num_want (-1 = default)
        request.put_u16(params.port);

        let mut buf = vec![0u8; 4096];

        // BEP-15: Exponential backoff for announce
        for retry in 0..UDP_MAX_RETRIES {
            let timeout_secs = 15u64 * (1 << retry);
            let timeout_duration = Duration::from_secs(timeout_secs.min(3840));

            socket.send(&request).await?;

            match timeout(timeout_duration, socket.recv(&mut buf)).await {
                Ok(Ok(n)) if n >= 20 => {
                    return self.parse_announce_response(&buf[..n], transaction_id);
                }
                Ok(Ok(_)) => {
                    return Err(TrackerError::InvalidResponse(
                        "announce response too short".into(),
                    ));
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => continue, // Timeout, retry
            }
        }

        Err(TrackerError::Timeout)
    }

    /// BEP-15: Connect with exponential backoff retry.
    /// Timeout starts at 15 seconds and doubles each retry: 15, 30, 60, 120...
    async fn connect(&self, socket: &UdpSocket) -> Result<i64, TrackerError> {
        let transaction_id: u32 = rand::thread_rng().gen();

        let mut request = BytesMut::with_capacity(16);
        request.put_i64(UDP_TRACKER_PROTOCOL_ID);
        request.put_u32(UDP_ACTION_CONNECT);
        request.put_u32(transaction_id);

        let mut buf = vec![0u8; 16];

        // BEP-15: Exponential backoff starting at 15 seconds
        for retry in 0..UDP_MAX_RETRIES {
            let timeout_secs = 15u64 * (1 << retry); // 15, 30, 60, 120, 240, 480, 960, 1920
            let timeout_duration = Duration::from_secs(timeout_secs.min(3840)); // Cap at ~1 hour

            socket.send(&request).await?;

            match timeout(timeout_duration, socket.recv(&mut buf)).await {
                Ok(Ok(n)) if n >= 16 => {
                    let mut cursor = &buf[..];
                    let action = cursor.get_u32();
                    let recv_transaction_id = cursor.get_u32();
                    let connection_id = cursor.get_i64();

                    if action == UDP_ACTION_ERROR {
                        return Err(TrackerError::TrackerFailure("connect failed".into()));
                    }

                    if action != UDP_ACTION_CONNECT {
                        return Err(TrackerError::InvalidAction);
                    }

                    if recv_transaction_id != transaction_id {
                        continue; // Wrong transaction, retry
                    }

                    return Ok(connection_id);
                }
                Ok(Ok(_)) => {
                    return Err(TrackerError::InvalidResponse(
                        "connect response too short".into(),
                    ));
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => continue, // Timeout, retry with longer timeout
            }
        }

        Err(TrackerError::Timeout)
    }

    fn parse_announce_response(
        &self,
        data: &[u8],
        expected_transaction_id: u32,
    ) -> Result<AnnounceResponse, TrackerError> {
        if data.len() < 20 {
            return Err(TrackerError::InvalidResponse(
                "announce response too short".into(),
            ));
        }

        let mut cursor = data;
        let action = cursor.get_u32();
        let transaction_id = cursor.get_u32();

        if transaction_id != expected_transaction_id {
            return Err(TrackerError::InvalidTransactionId);
        }

        if action == UDP_ACTION_ERROR {
            let error_msg = String::from_utf8_lossy(cursor).to_string();
            return Err(TrackerError::TrackerFailure(error_msg));
        }

        if action != UDP_ACTION_ANNOUNCE {
            return Err(TrackerError::InvalidAction);
        }

        let interval = cursor.get_u32();
        let incomplete = cursor.get_u32();
        let complete = cursor.get_u32();

        let peers = Peer::from_compact_v4(cursor);

        Ok(AnnounceResponse {
            interval,
            min_interval: None,
            complete: Some(complete),
            incomplete: Some(incomplete),
            peers,
            peers6: Vec::new(),
            warning_message: None,
            tracker_id: None,
        })
    }

    pub async fn scrape(
        &self,
        url: &str,
        info_hashes: &[InfoHashV1],
    ) -> Result<ScrapeResponse, TrackerError> {
        if info_hashes.is_empty() {
            return Ok(ScrapeResponse { files: Vec::new() });
        }

        let addr = self.resolve_tracker_addr(url).await?;
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(addr).await?;

        // Use cached connection ID if available
        let connection_id = self.get_connection_id(&socket, addr).await?;

        let transaction_id: u32 = rand::thread_rng().gen();

        let mut request = BytesMut::with_capacity(16 + info_hashes.len() * 20);
        request.put_i64(connection_id);
        request.put_u32(UDP_ACTION_SCRAPE);
        request.put_u32(transaction_id);

        for hash in info_hashes {
            request.put_slice(hash.as_bytes());
        }

        socket.send(&request).await?;

        let mut buf = vec![0u8; 8 + info_hashes.len() * 12];
        let n = timeout(UDP_TRACKER_REQUEST_TIMEOUT, socket.recv(&mut buf))
            .await
            .map_err(|_| TrackerError::Timeout)??;

        self.parse_scrape_response(&buf[..n], transaction_id, info_hashes)
    }

    fn parse_scrape_response(
        &self,
        data: &[u8],
        expected_transaction_id: u32,
        info_hashes: &[InfoHashV1],
    ) -> Result<ScrapeResponse, TrackerError> {
        if data.len() < 8 {
            return Err(TrackerError::InvalidResponse(
                "scrape response too short".into(),
            ));
        }

        let mut cursor = data;
        let action = cursor.get_u32();
        let transaction_id = cursor.get_u32();

        if transaction_id != expected_transaction_id {
            return Err(TrackerError::InvalidTransactionId);
        }

        if action == UDP_ACTION_ERROR {
            let error_msg = String::from_utf8_lossy(cursor).to_string();
            return Err(TrackerError::TrackerFailure(error_msg));
        }

        if action != UDP_ACTION_SCRAPE {
            return Err(TrackerError::InvalidAction);
        }

        let mut files = Vec::new();

        for hash in info_hashes.iter() {
            if cursor.len() < 12 {
                break;
            }

            let complete = cursor.get_u32();
            let downloaded = cursor.get_u32();
            let incomplete = cursor.get_u32();

            files.push((
                *hash.as_bytes(),
                ScrapeStats {
                    complete,
                    incomplete,
                    downloaded,
                },
            ));
        }

        Ok(ScrapeResponse { files })
    }

    async fn resolve_tracker_addr(&self, url: &str) -> Result<SocketAddr, TrackerError> {
        let parsed = Url::parse(url)?;

        let host = parsed
            .host_str()
            .ok_or_else(|| TrackerError::InvalidResponse("missing host".into()))?;

        let port = parsed
            .port()
            .ok_or_else(|| TrackerError::InvalidResponse("UDP tracker URL missing port".into()))?;

        let addr_str = format!("{}:{}", host, port);

        let addrs: Vec<_> = tokio::net::lookup_host(&addr_str).await?.collect();
        addrs
            .into_iter()
            .next()
            .ok_or(TrackerError::ConnectionRefused)
    }
}

impl Default for UdpTracker {
    fn default() -> Self {
        Self::new()
    }
}
