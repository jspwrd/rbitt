use std::time::Duration;

pub const CLIENT_PREFIX: &str = "-OX0001-";

pub const USER_AGENT: &str = "OxideBT/0.1.0";

pub const DEFAULT_PORT: u16 = 6881;

pub const LSD_PORT: u16 = 6771;

pub const SSDP_PORT: u16 = 1900;

pub const NATPMP_PORT: u16 = 5351;

pub const LSD_MULTICAST_V4: &str = "239.192.152.143";

pub const LSD_MULTICAST_V6: &str = "ff15::efc0:988f";

pub const SSDP_MULTICAST: &str = "239.255.255.250";

/// Maximum peers per torrent (qBittorrent default: 100, Transmission: 60)
/// Increased to 200 for high-bandwidth connections
pub const MAX_PEERS_PER_TORRENT: usize = 200;

/// Maximum half-open (connecting) connections per torrent (libtorrent default: 100)
/// This limits connections in progress to prevent resource exhaustion
/// Increased to 200 for faster peer acquisition on high-bandwidth connections
pub const MAX_HALF_OPEN: usize = 200;

/// Global connection limit (qBittorrent: 500, Transmission: 240, libtorrent: 200)
/// We use 500 to match qBittorrent defaults
pub const MAX_GLOBAL_CONNECTIONS: usize = 500;

pub const MAX_PENDING_DHT_QUERIES: usize = 1024;

/// Maximum peers we keep unchoked for uploads (qBittorrent: 4, libtorrent: 8)
/// This is upload slots - how many peers can download FROM us simultaneously
pub const MAX_UNCHOKED_PEERS: usize = 4;

/// Upload slots available for seeding (libtorrent default: 8)
pub const DEFAULT_UPLOAD_SLOTS: usize = 8;

pub const MAX_PEER_RETRY_ATTEMPTS: u32 = 5;

/// Maximum outstanding block requests per peer for request pipelining.
/// qBittorrent/libtorrent default: 500. Higher values improve throughput.
pub const MAX_REQUESTS_PER_PEER: usize = 500;

pub const DEFAULT_ALLOWED_FAST_COUNT: usize = 10;

/// Below this threshold, trigger emergency re-announce (very few peers)
pub const PEER_THRESHOLD_CRITICAL: usize = 10;

/// Below this threshold, use aggressive peer discovery
pub const PEER_THRESHOLD_LOW: usize = 30;

/// Target peer count - matches qBittorrent max_connections_per_torrent
pub const PEER_THRESHOLD_MEDIUM: usize = 100;

/// Connection attempts per second (libtorrent default: 30)
/// Increased to 100 for faster peer acquisition on high-bandwidth connections
pub const CONNECTION_SPEED: usize = 100;

pub const BLOCK_SIZE: usize = 16384;

/// Maximum request length per BEP 3 (128KB). Requests larger than this are suspicious.
pub const MAX_REQUEST_LENGTH: u32 = 131072;

pub const MAX_CONCURRENT_PIECES: usize = 50;

/// Maximum pieces to work on in parallel per peer connection.
/// Higher values improve parallelism but increase memory usage.
pub const MAX_PARALLEL_PIECES: usize = 32;

pub const ENDGAME_PIECES_THRESHOLD: usize = 10;

pub const METADATA_PIECE_SIZE: usize = 16384;

/// TCP connection timeout (libtorrent default: 10s, reduced for faster failure detection)
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(3);

/// Handshake timeout after TCP connect (libtorrent default: 10s)
/// Reduced from 20s to clear failed half-open connections faster
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub const PEER_READ_TIMEOUT: Duration = Duration::from_secs(180);

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

pub const SNUB_TIMEOUT: Duration = Duration::from_secs(60);

pub const BLOCK_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub const DHT_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

pub const HTTP_TRACKER_TIMEOUT: Duration = Duration::from_secs(30);

pub const UDP_TRACKER_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub const UDP_TRACKER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub const UPNP_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);

pub const UPNP_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

pub const UPNP_SOCKET_READ_TIMEOUT: Duration = Duration::from_millis(500);

pub const METADATA_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

pub const METADATA_READ_TIMEOUT: Duration = Duration::from_secs(5);

pub const MAGNET_METADATA_TIMEOUT: Duration = Duration::from_secs(10);

pub const TRACKER_ANNOUNCE_INTERVAL: Duration = Duration::from_secs(1800);

pub const TRACKER_MIN_INTERVAL: Duration = Duration::from_secs(60);

pub const TRACKER_AGGRESSIVE_INTERVAL: Duration = Duration::from_secs(300);

pub const TRACKER_MODERATE_INTERVAL: Duration = Duration::from_secs(900);

pub const DHT_INTERVAL_CRITICAL: Duration = Duration::from_secs(15);

pub const DHT_INTERVAL_LOW: Duration = Duration::from_secs(30);

pub const DHT_INTERVAL_MEDIUM: Duration = Duration::from_secs(60);

pub const DHT_INTERVAL_HIGH: Duration = Duration::from_secs(180);

pub const CHOKING_INTERVAL: Duration = Duration::from_secs(10);

pub const OPTIMISTIC_UNCHOKE_INTERVAL: Duration = Duration::from_secs(30);

pub const PEER_RETRY_BASE_DELAY: Duration = Duration::from_secs(60);

pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(120);

pub const LSD_ANNOUNCE_INTERVAL: Duration = Duration::from_secs(300);

/// PEX messages should be sent no more than once per minute per BEP-11
pub const PEX_SEND_INTERVAL: Duration = Duration::from_secs(60);

/// Delay before sending first PEX - reduced from 120s for faster peer discovery
/// BEP-11 doesn't mandate a delay, but we wait for extension handshake
pub const PEX_INITIAL_DELAY: Duration = Duration::from_secs(30);

pub const RATE_CALC_WINDOW: Duration = Duration::from_secs(5);

pub const RATE_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

pub const LOOP_INTERVAL_FAST: Duration = Duration::from_millis(200);

pub const LOOP_INTERVAL_NORMAL: Duration = Duration::from_millis(500);

pub const LOOP_INTERVAL_STABLE: Duration = Duration::from_secs(1);

pub const PAUSED_SLEEP_INTERVAL: Duration = Duration::from_secs(1);

pub const CHECKING_SLEEP_INTERVAL: Duration = Duration::from_millis(500);

/// Socket receive buffer size (2MB for high throughput)
pub const SOCKET_RECV_BUFFER_SIZE: usize = 2097152;

/// Socket send buffer size (2MB for high throughput)
pub const SOCKET_SEND_BUFFER_SIZE: usize = 2097152;

/// Read buffer size for peer connections (512KB)
pub const READ_BUFFER_SIZE: usize = 524288;

pub const MAX_MESSAGE_SIZE: usize = 16777216;

pub const MAX_METADATA_SIZE: usize = 1048576;

pub const LSD_COOKIE_SIZE: usize = 8;

pub const LSD_CHANNEL_CAPACITY: usize = 64;

pub const DHT_BUCKET_SIZE: usize = 8;

pub const DHT_NUM_BUCKETS: usize = 160;

pub const DHT_ALPHA: usize = 8;

pub const DHT_MAX_ITERATIONS: usize = 15;

pub const DHT_PEERS_EARLY_RETURN: usize = 50;

pub const DHT_BOOTSTRAP_NODES: &[&str] = &[
    "router.bittorrent.com:6881",
    "router.utorrent.com:6881",
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401",
];

pub const PEX_MAX_PEERS_PER_MESSAGE: usize = 100;

pub const PEX_MAX_IPV4_PEERS: usize = 50;

pub const PEX_MAX_IPV6_PEERS: usize = 50;

pub const PEX_FLAG_PREFERS_ENCRYPTION: u8 = 0x01;

pub const PEX_FLAG_UPLOAD_ONLY: u8 = 0x02;

pub const PEX_FLAG_SUPPORTS_UTP: u8 = 0x04;

pub const PEX_FLAG_SUPPORTS_HOLEPUNCH: u8 = 0x08;

pub const PEX_FLAG_REACHABLE: u8 = 0x10;

pub const PROTOCOL_STRING: &str = "BitTorrent protocol";

pub const RESERVED_BYTES: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];

pub const EXTENSION_BIT: u8 = 0x10;

pub const DHT_BIT: u8 = 0x01;

pub const FAST_EXTENSION_BIT: u8 = 0x04;

pub const EXTENSION_HANDSHAKE_ID: u8 = 0;

pub const UT_METADATA_ID: u8 = 1;

pub const UT_PEX_ID: u8 = 2;

pub const UDP_TRACKER_PROTOCOL_ID: i64 = 0x41727101980;

pub const UDP_ACTION_CONNECT: u32 = 0;

pub const UDP_ACTION_ANNOUNCE: u32 = 1;

pub const UDP_ACTION_SCRAPE: u32 = 2;

pub const UDP_ACTION_ERROR: u32 = 3;

pub const SSDP_MX_VALUE: u8 = 3;

pub const BANDWIDTH_BURST_MULTIPLIER: u64 = 2;

pub const PROGRESS_LOG_INTERVAL: usize = 100;

pub const BACKOFF_EXPONENT_CAP: u32 = 4;

pub const DHT_QUERY_SLEEP: Duration = Duration::from_millis(250);

pub const PEER_RETRY_SLEEP: Duration = Duration::from_millis(250);

pub const CONNECTION_RETRY_SLEEP: Duration = Duration::from_millis(100);

pub const DEFAULT_CACHE_MEMORY: usize = 256 * 1024 * 1024;

pub const MAX_CACHE_MEMORY: usize = 1024 * 1024 * 1024;

pub const BLOCK_CACHE_RATIO: f32 = 0.6;

pub const PIECE_CACHE_RATIO: f32 = 0.3;

pub const WRITE_COALESCE_TIMEOUT: Duration = Duration::from_secs(5);

pub const IO_BATCH_SIZE: usize = 64;

pub const IO_BATCH_TIMEOUT: Duration = Duration::from_millis(10);

pub const IO_WORKERS: usize = 4;

pub const BUFFER_POOL_BLOCKS: usize = 1024;

pub const BUFFER_POOL_PIECES: usize = 64;
