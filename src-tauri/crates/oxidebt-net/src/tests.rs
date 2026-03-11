use super::*;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

#[test]
fn test_pex_flags() {
    let flags = PexFlags {
        encryption: true,
        seed: false,
        utp: true,
        holepunch: false,
        connectable: true,
    };

    let byte = flags.to_byte();
    let decoded = PexFlags::from_byte(byte);

    assert_eq!(flags, decoded);
}

#[test]
fn test_pex_encode_decode_v4() {
    let mut msg = PexMessage::new();
    msg.add_peer(PexPeer {
        addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 1), 6881)),
        flags: PexFlags {
            encryption: true,
            ..Default::default()
        },
    });
    msg.add_peer(PexPeer {
        addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 51413)),
        flags: PexFlags {
            seed: true,
            ..Default::default()
        },
    });

    let added = msg.encode_added();
    let flags = msg.encode_added_flags();

    let decoded = PexMessage::decode_added(&added, &flags);
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].addr, msg.added[0].addr);
    assert!(decoded[0].flags.encryption);
    assert_eq!(decoded[1].addr, msg.added[1].addr);
    assert!(decoded[1].flags.seed);
}

#[test]
fn test_pex_dropped() {
    let mut msg = PexMessage::new();
    msg.drop_peer(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(192, 168, 1, 1),
        6881,
    )));

    let dropped = msg.encode_dropped();
    let decoded = PexMessage::decode_dropped(&dropped);

    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0], msg.dropped[0]);
}

#[tokio::test]
async fn test_bandwidth_limiter_unlimited() {
    let limiter = BandwidthLimiter::unlimited();

    let start = std::time::Instant::now();
    limiter.acquire_download(1_000_000).await;
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 100);
}

#[tokio::test]
async fn test_rate_limiter_basic() {
    let limiter = RateLimiter::new(10000);

    let wait = limiter.acquire(1000).await;
    assert!(wait.is_zero() || wait.as_millis() < 10);

    assert!(limiter.available() < 20000);
}

#[test]
fn test_port_mapping_protocol() {
    let _tcp = Protocol::Tcp;
    let _udp = Protocol::Udp;
}

// ========================
// PEX IPv6 encode/decode
// ========================

#[test]
fn test_pex_encode_decode_v6() {
    let mut msg = PexMessage::new();
    msg.add_peer(PexPeer {
        addr: SocketAddr::V6(std::net::SocketAddrV6::new(
            std::net::Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1),
            6881,
            0,
            0,
        )),
        flags: PexFlags {
            utp: true,
            ..Default::default()
        },
    });

    let added6 = msg.encode_added6();
    let flags6 = msg.encode_added6_flags();

    let decoded = PexMessage::decode_added6(&added6, &flags6);
    assert_eq!(decoded.len(), 1);
    assert!(decoded[0].addr.is_ipv6());
    assert_eq!(decoded[0].addr.port(), 6881);
    assert!(decoded[0].flags.utp);
}

#[test]
fn test_pex_dropped_v6() {
    let mut msg = PexMessage::new();
    msg.drop_peer(SocketAddr::V6(std::net::SocketAddrV6::new(
        std::net::Ipv6Addr::LOCALHOST,
        6881,
        0,
        0,
    )));

    let dropped6 = msg.encode_dropped6();
    let decoded = PexMessage::decode_dropped6(&dropped6);
    assert_eq!(decoded.len(), 1);
    assert!(decoded[0].is_ipv6());
    assert_eq!(decoded[0].port(), 6881);
}

#[test]
fn test_pex_add_peer_routing() {
    let mut msg = PexMessage::new();

    // IPv4 peer should go to added
    msg.add_peer(PexPeer::new(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(1, 2, 3, 4),
        6881,
    ))));

    // IPv6 peer should go to added6
    msg.add_peer(PexPeer::new(SocketAddr::V6(std::net::SocketAddrV6::new(
        std::net::Ipv6Addr::LOCALHOST,
        6882,
        0,
        0,
    ))));

    assert_eq!(msg.added.len(), 1);
    assert_eq!(msg.added6.len(), 1);
}

#[test]
fn test_pex_drop_peer_routing() {
    let mut msg = PexMessage::new();

    msg.drop_peer(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(1, 2, 3, 4),
        6881,
    )));
    msg.drop_peer(SocketAddr::V6(std::net::SocketAddrV6::new(
        std::net::Ipv6Addr::LOCALHOST,
        6882,
        0,
        0,
    )));

    assert_eq!(msg.dropped.len(), 1);
    assert_eq!(msg.dropped6.len(), 1);
}

#[test]
fn test_pex_is_empty() {
    let msg = PexMessage::new();
    assert!(msg.is_empty());

    let mut msg2 = PexMessage::new();
    msg2.add_peer(PexPeer::new(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(1, 2, 3, 4),
        6881,
    ))));
    assert!(!msg2.is_empty());
}

#[test]
fn test_pex_flags_all_set() {
    let flags = PexFlags {
        encryption: true,
        seed: true,
        utp: true,
        holepunch: true,
        connectable: true,
    };

    let byte = flags.to_byte();
    assert_eq!(byte, 0x01 | 0x02 | 0x04 | 0x08 | 0x10);

    let decoded = PexFlags::from_byte(byte);
    assert_eq!(flags, decoded);
}

#[test]
fn test_pex_flags_none_set() {
    let flags = PexFlags::default();
    assert_eq!(flags.to_byte(), 0);
    let decoded = PexFlags::from_byte(0);
    assert_eq!(flags, decoded);
}

#[test]
fn test_pex_peer_with_flags() {
    let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 6881));
    let flags = PexFlags {
        seed: true,
        ..Default::default()
    };
    let peer = PexPeer::with_flags(addr, flags);
    assert_eq!(peer.addr, addr);
    assert!(peer.flags.seed);
}

// ========================
// BandwidthLimiter tests
// ========================

#[tokio::test]
async fn test_bandwidth_limiter_with_limits() {
    let limiter = BandwidthLimiter::new(100_000, 50_000);

    // Small transfers should still be fast (within burst allowance)
    let start = std::time::Instant::now();
    limiter.acquire_download(1000).await;
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 100);
}

#[tokio::test]
async fn test_bandwidth_limiter_set_limits() {
    let mut limiter = BandwidthLimiter::unlimited();

    limiter.set_download_limit(100_000);
    limiter.set_upload_limit(50_000);

    // Should still work
    let start = std::time::Instant::now();
    limiter.acquire_download(1000).await;
    limiter.acquire_upload(1000).await;
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 100);
}

#[tokio::test]
async fn test_bandwidth_limiter_zero_means_unlimited() {
    let mut limiter = BandwidthLimiter::new(100_000, 100_000);

    // Setting to 0 should make it unlimited
    limiter.set_download_limit(0);
    limiter.set_upload_limit(0);

    let start = std::time::Instant::now();
    limiter.acquire_download(1_000_000).await;
    limiter.acquire_upload(1_000_000).await;
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 100);
}

#[tokio::test]
async fn test_rate_limiter_tokens_deplete() {
    let limiter = RateLimiter::new(10_000); // 10KB/s

    // Consume all burst tokens (2 * rate = 20KB)
    let _ = limiter.acquire(20_000).await;

    // Next acquire should require waiting
    let wait = limiter.acquire(10_000).await;
    assert!(
        wait.as_millis() > 0,
        "Should have to wait after depleting tokens"
    );
}

#[tokio::test]
async fn test_rate_limiter_set_rate() {
    let limiter = RateLimiter::new(10_000);

    // Change rate - should not panic
    limiter.set_rate(1_000_000);

    // Acquire within the existing token budget (started with 20,000 tokens)
    let wait = limiter.acquire(1_000).await;
    assert!(wait.is_zero() || wait.as_millis() < 10);
}

#[test]
fn test_bandwidth_limiter_get_limiters() {
    let limiter = BandwidthLimiter::new(100_000, 50_000);
    let dl = limiter.download_limiter();
    let ul = limiter.upload_limiter();
    assert!(dl.available() > 0);
    assert!(ul.available() > 0);
}

// ========================
// PortMapper tests
// ========================

#[test]
fn test_port_mapper_new_not_available() {
    let mapper = PortMapper::new();
    assert!(!mapper.is_available());
}

#[test]
fn test_protocol_equality() {
    assert_eq!(Protocol::Tcp, Protocol::Tcp);
    assert_eq!(Protocol::Udp, Protocol::Udp);
    assert_ne!(Protocol::Tcp, Protocol::Udp);
}

#[test]
fn test_port_mapping_struct() {
    let mapping = PortMapping {
        internal_port: 6881,
        external_port: 6881,
        protocol: Protocol::Tcp,
        lifetime: 3600,
    };

    assert_eq!(mapping.internal_port, 6881);
    assert_eq!(mapping.external_port, 6881);
    assert_eq!(mapping.protocol, Protocol::Tcp);
    assert_eq!(mapping.lifetime, 3600);
}
