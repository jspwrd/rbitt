use super::*;
use crate::http::HttpTracker;
use crate::response::{Peer, ScrapeStats};
use crate::udp::UdpTracker;
use bytes::{BufMut, BytesMut};
use oxidebt_bencode::{encode, Value};
use std::collections::BTreeMap;
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
    let data = [127, 0, 0, 1, 0x1A, 0xE1, 192, 168, 1, 1, 0x1A, 0xE2];

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

// ========================
// HTTP announce response parsing
// ========================

#[test]
fn test_http_parse_announce_compact() {
    let tracker = HttpTracker::new();

    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"interval"),
        Value::Integer(1800),
    );
    dict.insert(
        bytes::Bytes::from_static(b"complete"),
        Value::Integer(42),
    );
    dict.insert(
        bytes::Bytes::from_static(b"incomplete"),
        Value::Integer(7),
    );

    // Compact IPv4 peers: 127.0.0.1:6881, 10.0.0.1:51413
    let mut peer_data = Vec::new();
    peer_data.extend_from_slice(&[127, 0, 0, 1, 0x1A, 0xE1]);
    peer_data.extend_from_slice(&[10, 0, 0, 1, 0xC8, 0xD5]);
    dict.insert(
        bytes::Bytes::from_static(b"peers"),
        Value::Bytes(bytes::Bytes::from(peer_data)),
    );

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let result = tracker.parse_announce_response(&encoded);
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert_eq!(resp.interval, 1800);
    assert_eq!(resp.complete, Some(42));
    assert_eq!(resp.incomplete, Some(7));
    assert_eq!(resp.peers.len(), 2);
    assert_eq!(
        resp.peers[0].addr,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6881)
    );
    assert_eq!(
        resp.peers[1].addr,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 51413)
    );
}

#[test]
fn test_http_parse_announce_dict_peers() {
    let tracker = HttpTracker::new();

    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"interval"),
        Value::Integer(900),
    );

    let mut peer_dict = BTreeMap::new();
    peer_dict.insert(bytes::Bytes::from_static(b"ip"), Value::string("192.168.1.1"));
    peer_dict.insert(
        bytes::Bytes::from_static(b"port"),
        Value::Integer(6881),
    );

    let peer_id = vec![0xAB; 20];
    peer_dict.insert(
        bytes::Bytes::from_static(b"peer id"),
        Value::Bytes(bytes::Bytes::from(peer_id)),
    );

    dict.insert(
        bytes::Bytes::from_static(b"peers"),
        Value::List(vec![Value::Dict(peer_dict)]),
    );

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let result = tracker.parse_announce_response(&encoded);
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert_eq!(resp.interval, 900);
    assert_eq!(resp.peers.len(), 1);
    assert_eq!(
        resp.peers[0].addr,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 6881)
    );
    assert!(resp.peers[0].peer_id.is_some());
}

#[test]
fn test_http_parse_announce_failure() {
    let tracker = HttpTracker::new();

    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"failure reason"),
        Value::string("torrent not registered"),
    );

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let result = tracker.parse_announce_response(&encoded);
    assert!(result.is_err());

    match result.unwrap_err() {
        TrackerError::TrackerFailure(msg) => {
            assert!(msg.contains("not registered"));
        }
        e => panic!("wrong error type: {:?}", e),
    }
}

#[test]
fn test_http_parse_announce_with_warning() {
    let tracker = HttpTracker::new();

    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"interval"),
        Value::Integer(1800),
    );
    dict.insert(
        bytes::Bytes::from_static(b"warning message"),
        Value::string("slow down"),
    );

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let resp = tracker.parse_announce_response(&encoded).unwrap();
    assert_eq!(resp.warning_message, Some("slow down".to_string()));
}

#[test]
fn test_http_parse_announce_with_tracker_id() {
    let tracker = HttpTracker::new();

    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"interval"),
        Value::Integer(1800),
    );
    dict.insert(
        bytes::Bytes::from_static(b"tracker id"),
        Value::string("abc123"),
    );

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let resp = tracker.parse_announce_response(&encoded).unwrap();
    assert_eq!(resp.tracker_id, Some("abc123".to_string()));
}

#[test]
fn test_http_parse_announce_with_min_interval() {
    let tracker = HttpTracker::new();

    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"interval"),
        Value::Integer(1800),
    );
    dict.insert(
        bytes::Bytes::from_static(b"min interval"),
        Value::Integer(60),
    );

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let resp = tracker.parse_announce_response(&encoded).unwrap();
    assert_eq!(resp.min_interval, Some(60));
}

#[test]
fn test_http_parse_announce_missing_interval() {
    let tracker = HttpTracker::new();

    let dict = BTreeMap::new();
    let encoded = encode(&Value::Dict(dict)).unwrap();
    let result = tracker.parse_announce_response(&encoded);
    assert!(result.is_err());
}

#[test]
fn test_http_parse_announce_invalid_bencode() {
    let tracker = HttpTracker::new();
    let result = tracker.parse_announce_response(b"invalid data");
    assert!(result.is_err());
}

#[test]
fn test_http_parse_announce_with_peers6() {
    let tracker = HttpTracker::new();

    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"interval"),
        Value::Integer(1800),
    );

    // Compact IPv6: ::1 port 6881
    let mut peer6_data = vec![0u8; 16];
    peer6_data[15] = 1;
    peer6_data.extend_from_slice(&[0x1A, 0xE1]);
    dict.insert(
        bytes::Bytes::from_static(b"peers6"),
        Value::Bytes(bytes::Bytes::from(peer6_data)),
    );

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let resp = tracker.parse_announce_response(&encoded).unwrap();
    assert_eq!(resp.peers6.len(), 1);
    assert_eq!(resp.peers6[0].addr.port(), 6881);
    assert!(resp.peers6[0].addr.is_ipv6());
}

// ========================
// HTTP scrape response parsing
// ========================

#[test]
fn test_http_parse_scrape_response() {
    let tracker = HttpTracker::new();

    let info_hash = [0xAB; 20];
    let mut stats = BTreeMap::new();
    stats.insert(
        bytes::Bytes::from_static(b"complete"),
        Value::Integer(100),
    );
    stats.insert(
        bytes::Bytes::from_static(b"incomplete"),
        Value::Integer(50),
    );
    stats.insert(
        bytes::Bytes::from_static(b"downloaded"),
        Value::Integer(1000),
    );

    let mut files = BTreeMap::new();
    files.insert(
        bytes::Bytes::from(info_hash.to_vec()),
        Value::Dict(stats),
    );

    let mut dict = BTreeMap::new();
    dict.insert(bytes::Bytes::from_static(b"files"), Value::Dict(files));

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let resp = tracker.parse_scrape_response(&encoded).unwrap();
    assert_eq!(resp.files.len(), 1);
    assert_eq!(resp.files[0].0, info_hash);
    assert_eq!(resp.files[0].1.complete, 100);
    assert_eq!(resp.files[0].1.incomplete, 50);
    assert_eq!(resp.files[0].1.downloaded, 1000);
}

#[test]
fn test_http_parse_scrape_failure() {
    let tracker = HttpTracker::new();

    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"failure reason"),
        Value::string("scrape not allowed"),
    );

    let encoded = encode(&Value::Dict(dict)).unwrap();
    let result = tracker.parse_scrape_response(&encoded);
    assert!(result.is_err());
}

#[test]
fn test_http_announce_to_scrape_url() {
    let tracker = HttpTracker::new();

    let url = "http://tracker.example.com/announce";
    let scrape = tracker.announce_to_scrape_url(url).unwrap();
    assert_eq!(scrape, "http://tracker.example.com/scrape");
}

#[test]
fn test_http_announce_to_scrape_url_with_path() {
    let tracker = HttpTracker::new();

    let url = "http://tracker.example.com/path/to/announce?passkey=abc";
    let scrape = tracker.announce_to_scrape_url(url).unwrap();
    assert_eq!(
        scrape,
        "http://tracker.example.com/path/to/scrape?passkey=abc"
    );
}

#[test]
fn test_http_announce_to_scrape_url_invalid() {
    let tracker = HttpTracker::new();

    let url = "http://tracker.example.com/something";
    let result = tracker.announce_to_scrape_url(url);
    assert!(result.is_err());
}

// ========================
// UDP announce response parsing
// ========================

#[test]
fn test_udp_parse_announce_response() {
    let tracker = UdpTracker::new();
    let transaction_id: u32 = 12345;

    let mut buf = BytesMut::new();
    buf.put_u32(1); // action: announce
    buf.put_u32(transaction_id);
    buf.put_u32(1800); // interval
    buf.put_u32(5); // leechers (incomplete)
    buf.put_u32(10); // seeders (complete)
    // Compact peers
    buf.put_slice(&[127, 0, 0, 1, 0x1A, 0xE1]); // 127.0.0.1:6881
    buf.put_slice(&[10, 0, 0, 1, 0xC8, 0xD5]); // 10.0.0.1:51413

    let resp = tracker
        .parse_announce_response(&buf, transaction_id)
        .unwrap();
    assert_eq!(resp.interval, 1800);
    assert_eq!(resp.complete, Some(10));
    assert_eq!(resp.incomplete, Some(5));
    assert_eq!(resp.peers.len(), 2);
}

#[test]
fn test_udp_parse_announce_wrong_transaction() {
    let tracker = UdpTracker::new();

    let mut buf = BytesMut::new();
    buf.put_u32(1); // action: announce
    buf.put_u32(99999); // wrong transaction id
    buf.put_u32(1800);
    buf.put_u32(0);
    buf.put_u32(0);

    let result = tracker.parse_announce_response(&buf, 12345);
    assert!(matches!(result, Err(TrackerError::InvalidTransactionId)));
}

#[test]
fn test_udp_parse_announce_error_action() {
    let tracker = UdpTracker::new();
    let transaction_id: u32 = 12345;

    let mut buf = BytesMut::new();
    buf.put_u32(3); // action: error
    buf.put_u32(transaction_id);
    buf.put_slice(b"torrent not found");

    let result = tracker.parse_announce_response(&buf, transaction_id);
    assert!(matches!(result, Err(TrackerError::TrackerFailure(_))));
}

#[test]
fn test_udp_parse_announce_invalid_action() {
    let tracker = UdpTracker::new();
    let transaction_id: u32 = 12345;

    let mut buf = BytesMut::new();
    buf.put_u32(255); // invalid action
    buf.put_u32(transaction_id);
    buf.put_u32(1800);
    buf.put_u32(0);
    buf.put_u32(0);

    let result = tracker.parse_announce_response(&buf, transaction_id);
    assert!(matches!(result, Err(TrackerError::InvalidAction)));
}

#[test]
fn test_udp_parse_announce_too_short() {
    let tracker = UdpTracker::new();
    let result = tracker.parse_announce_response(&[0u8; 10], 12345);
    assert!(result.is_err());
}

// ========================
// UDP scrape response parsing
// ========================

#[test]
fn test_udp_parse_scrape_response() {
    let tracker = UdpTracker::new();
    let transaction_id: u32 = 12345;
    let info_hash = oxidebt_torrent::InfoHashV1([0xAB; 20]);

    let mut buf = BytesMut::new();
    buf.put_u32(2); // action: scrape
    buf.put_u32(transaction_id);
    buf.put_u32(100); // complete (seeders)
    buf.put_u32(1000); // downloaded
    buf.put_u32(50); // incomplete (leechers)

    let resp = tracker
        .parse_scrape_response(&buf, transaction_id, &[info_hash])
        .unwrap();
    assert_eq!(resp.files.len(), 1);
    assert_eq!(resp.files[0].1.complete, 100);
    assert_eq!(resp.files[0].1.downloaded, 1000);
    assert_eq!(resp.files[0].1.incomplete, 50);
}

#[test]
fn test_udp_parse_scrape_wrong_transaction() {
    let tracker = UdpTracker::new();
    let info_hash = oxidebt_torrent::InfoHashV1([0xAB; 20]);

    let mut buf = BytesMut::new();
    buf.put_u32(2);
    buf.put_u32(99999);
    buf.put_u32(0);
    buf.put_u32(0);
    buf.put_u32(0);

    let result = tracker.parse_scrape_response(&buf, 12345, &[info_hash]);
    assert!(matches!(result, Err(TrackerError::InvalidTransactionId)));
}

#[test]
fn test_udp_parse_scrape_error_action() {
    let tracker = UdpTracker::new();
    let transaction_id: u32 = 12345;
    let info_hash = oxidebt_torrent::InfoHashV1([0xAB; 20]);

    let mut buf = BytesMut::new();
    buf.put_u32(3); // error action
    buf.put_u32(transaction_id);
    buf.put_slice(b"scrape denied");

    let result = tracker.parse_scrape_response(&buf, transaction_id, &[info_hash]);
    assert!(matches!(result, Err(TrackerError::TrackerFailure(_))));
}

#[test]
fn test_udp_parse_scrape_too_short() {
    let tracker = UdpTracker::new();
    let info_hash = oxidebt_torrent::InfoHashV1([0xAB; 20]);

    let result = tracker.parse_scrape_response(&[0u8; 4], 12345, &[info_hash]);
    assert!(result.is_err());
}

// ========================
// TrackerClient protocol routing
// ========================

#[tokio::test]
async fn test_tracker_client_unsupported_protocol() {
    let client = TrackerClient::new();
    let info_hash = oxidebt_torrent::InfoHashV1([0xAB; 20]);
    let peer_id = [0u8; 20];

    let params = AnnounceParams {
        url: "ftp://tracker.example.com/announce",
        info_hash: &info_hash,
        peer_id: &peer_id,
        port: 6881,
        uploaded: 0,
        downloaded: 0,
        left: 1000,
        event: TrackerEvent::Started,
    };

    let result = client.announce(params).await;
    assert!(matches!(result, Err(TrackerError::UnsupportedProtocol(_))));
}

#[tokio::test]
async fn test_tracker_client_scrape_unsupported_protocol() {
    let client = TrackerClient::new();
    let result = client.scrape("ftp://bad.com/scrape", &[]).await;
    assert!(matches!(result, Err(TrackerError::UnsupportedProtocol(_))));
}

// ========================
// Peer compact format edge cases
// ========================

#[test]
fn test_peer_from_compact_v4_empty() {
    let peers = Peer::from_compact_v4(&[]);
    assert!(peers.is_empty());
}

#[test]
fn test_peer_from_compact_v4_incomplete_chunk() {
    // 7 bytes - not a multiple of 6, last byte should be ignored
    let data = [127, 0, 0, 1, 0x1A, 0xE1, 0xFF];
    let peers = Peer::from_compact_v4(&data);
    assert_eq!(peers.len(), 1);
}

#[test]
fn test_peer_from_compact_v6_empty() {
    let peers = Peer::from_compact_v6(&[]);
    assert!(peers.is_empty());
}

#[test]
fn test_announce_response_empty_peers() {
    let response = AnnounceResponse {
        interval: 1800,
        min_interval: None,
        complete: None,
        incomplete: None,
        peers: vec![],
        peers6: vec![],
        warning_message: None,
        tracker_id: None,
    };

    let all = response.all_peers();
    assert!(all.is_empty());
}
