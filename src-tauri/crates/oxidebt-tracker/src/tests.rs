
use super::*;
use crate::response::{Peer, ScrapeStats};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
fn test_tracker_event_str() {
    assert_eq!(TrackerEvent::None.as_str(), None);
    assert_eq!(TrackerEvent::Started.as_str(), Some("started"));
    assert_eq!(TrackerEvent::Stopped.as_str(), Some("stopped"));
    assert_eq!(TrackerEvent::Completed.as_str(), Some("completed"));
}

#[test]
fn test_tracker_event_u32() {
    assert_eq!(TrackerEvent::None.as_u32(), 0);
    assert_eq!(TrackerEvent::Completed.as_u32(), 1);
    assert_eq!(TrackerEvent::Started.as_u32(), 2);
    assert_eq!(TrackerEvent::Stopped.as_u32(), 3);
}

#[test]
fn test_peer_from_compact_v4() {
    let data = [
        127, 0, 0, 1, 0x1A, 0xE1,
        192, 168, 1, 1, 0x1A, 0xE2,
    ];

    let peers = Peer::from_compact_v4(&data);
    assert_eq!(peers.len(), 2);

    assert_eq!(
        peers[0].addr,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6881)
    );
    assert_eq!(
        peers[1].addr,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 6882)
    );
}

#[test]
fn test_peer_from_compact_v6() {
    let mut data = [0u8; 18];
    data[15] = 1;
    data[16] = 0x1A;
    data[17] = 0xE1;

    let peers = Peer::from_compact_v6(&data);
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].addr.port(), 6881);
}

#[test]
fn test_announce_response_all_peers() {
    let peers_v4 = vec![Peer {
        addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6881),
        peer_id: None,
    }];

    let peers_v6 = vec![Peer {
        addr: SocketAddr::new(
            IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)),
            6881,
        ),
        peer_id: None,
    }];

    let response = AnnounceResponse {
        interval: 1800,
        min_interval: None,
        complete: Some(10),
        incomplete: Some(5),
        peers: peers_v4,
        peers6: peers_v6,
        warning_message: None,
        tracker_id: None,
    };

    let all = response.all_peers();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_scrape_stats() {
    let stats = ScrapeStats {
        complete: 100,
        incomplete: 50,
        downloaded: 1000,
    };

    assert_eq!(stats.complete, 100);
    assert_eq!(stats.incomplete, 50);
    assert_eq!(stats.downloaded, 1000);
}
