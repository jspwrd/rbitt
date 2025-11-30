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
