use super::*;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
fn test_node_id_generate() {
    let id1 = NodeId::generate();
    let id2 = NodeId::generate();
    assert_ne!(id1.0, id2.0);
}

#[test]
fn test_node_id_from_bytes() {
    let bytes = [1u8; 20];
    let id = NodeId::from_bytes(&bytes).unwrap();
    assert_eq!(id.0, bytes);
}

#[test]
fn test_node_id_from_bytes_invalid() {
    let bytes = [1u8; 10];
    assert!(NodeId::from_bytes(&bytes).is_err());
}

#[test]
fn test_node_id_distance() {
    let id1 = NodeId([0u8; 20]);
    let id2 = NodeId([0xFF; 20]);

    let dist = id1.distance(&id2);
    assert_eq!(dist, [0xFF; 20]);

    let dist_self = id1.distance(&id1);
    assert_eq!(dist_self, [0u8; 20]);
}

#[test]
fn test_node_id_bucket_index() {
    let id1 = NodeId([0u8; 20]);
    let mut id2_bytes = [0u8; 20];
    id2_bytes[0] = 0x80;
    let id2 = NodeId(id2_bytes);

    assert_eq!(id1.bucket_index(&id2), 0);
}

#[test]
fn test_node_compact() {
    let id = NodeId([1u8; 20]);
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6881);
    let node = Node::new(id, addr);

    let compact = node.to_compact().unwrap();
    assert_eq!(compact.len(), 26);

    let parsed = Node::from_compact(&compact).unwrap();
    assert_eq!(parsed.id.0, id.0);
    assert_eq!(parsed.addr, addr);
}

#[test]
fn test_node_state() {
    let id = NodeId::generate();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6881);
    let mut node = Node::new(id, addr);

    assert!(node.is_good());
    assert!(!node.is_bad());

    node.fail();
    node.fail();
    node.fail();
    assert!(node.is_bad());
}

#[test]
fn test_routing_table_add() {
    let our_id = NodeId::generate();
    let table = RoutingTable::new(our_id);

    for _ in 0..10 {
        let id = NodeId::generate();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6881);
        table.add_node(Node::new(id, addr));
    }

    assert!(table.node_count() > 0);
}

#[test]
fn test_routing_table_find_closest() {
    let our_id = NodeId::generate();
    let table = RoutingTable::new(our_id);

    for i in 0..20 {
        let mut id_bytes = [0u8; 20];
        id_bytes[0] = i;
        let id = NodeId(id_bytes);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, i)), 6881);
        table.add_node(Node::new(id, addr));
    }

    let target = NodeId([0u8; 20]);
    let closest = table.find_closest(&target, 8);
    assert!(closest.len() <= 8);
}

#[test]
fn test_dht_message_ping() {
    let our_id = NodeId::generate();
    let tid = bytes::Bytes::from_static(b"aa");

    let msg = DhtMessage::ping(tid.clone(), &our_id);
    let encoded = msg.encode().unwrap();

    let parsed = DhtMessage::parse(&encoded).unwrap();
    assert_eq!(parsed.transaction_id, tid);
    assert!(parsed.query.is_some());
}

#[test]
fn test_dht_message_find_node() {
    let our_id = NodeId::generate();
    let target = NodeId::generate();
    let tid = bytes::Bytes::from_static(b"bb");

    let msg = DhtMessage::find_node(tid.clone(), &our_id, target);
    let encoded = msg.encode().unwrap();

    let parsed = DhtMessage::parse(&encoded).unwrap();
    assert_eq!(parsed.transaction_id, tid);

    if let Some((name, query)) = parsed.query {
        assert_eq!(name, "find_node");
        match query {
            DhtQuery::FindNode { target: t } => {
                assert_eq!(t.0, target.0);
            }
            _ => panic!("wrong query type"),
        }
    } else {
        panic!("missing query");
    }
}

#[test]
fn test_dht_message_get_peers() {
    let our_id = NodeId::generate();
    let info_hash = [0xAB; 20];
    let tid = bytes::Bytes::from_static(b"cc");

    let msg = DhtMessage::get_peers(tid.clone(), &our_id, info_hash);
    let encoded = msg.encode().unwrap();

    let parsed = DhtMessage::parse(&encoded).unwrap();
    assert_eq!(parsed.transaction_id, tid);

    if let Some((name, query)) = parsed.query {
        assert_eq!(name, "get_peers");
        match query {
            DhtQuery::GetPeers { info_hash: h } => {
                assert_eq!(h, info_hash);
            }
            _ => panic!("wrong query type"),
        }
    } else {
        panic!("missing query");
    }
}

// ========================
// announce_peer message encode/decode
// ========================

#[test]
fn test_dht_message_announce_peer() {
    let our_id = NodeId::generate();
    let info_hash = [0xBB; 20];
    let tid = bytes::Bytes::from_static(b"dd");
    let token = bytes::Bytes::from_static(b"tok123");

    let msg = DhtMessage::announce_peer(tid.clone(), &our_id, info_hash, 6881, token.clone());
    let encoded = msg.encode().unwrap();

    let parsed = DhtMessage::parse(&encoded).unwrap();
    assert_eq!(parsed.transaction_id, tid);

    if let Some((name, query)) = parsed.query {
        assert_eq!(name, "announce_peer");
        match query {
            DhtQuery::AnnouncePeer {
                info_hash: h,
                port,
                token: t,
                implied_port,
            } => {
                assert_eq!(h, info_hash);
                assert_eq!(port, 6881);
                assert_eq!(t, token);
                assert!(!implied_port);
            }
            _ => panic!("wrong query type"),
        }
    } else {
        panic!("missing query");
    }
}

// ========================
// Response parsing
// ========================

#[test]
fn test_dht_parse_ping_response() {
    let responder_id = NodeId::generate();
    let tid = bytes::Bytes::from_static(b"pp");

    let msg = DhtMessage {
        transaction_id: tid.clone(),
        sender_id: Some(responder_id),
        query: None,
        response: Some(DhtResponse::Ping { id: responder_id }),
    };

    let encoded = msg.encode().unwrap();
    let parsed = DhtMessage::parse(&encoded).unwrap();

    assert_eq!(parsed.transaction_id, tid);
    assert!(parsed.query.is_none());
    assert!(parsed.response.is_some());

    match parsed.response.unwrap() {
        DhtResponse::Ping { id } => assert_eq!(id.0, responder_id.0),
        _ => panic!("wrong response type"),
    }
}

#[test]
fn test_dht_parse_find_node_response() {
    let responder_id = NodeId::generate();
    let tid = bytes::Bytes::from_static(b"fn");

    let node_id = NodeId([42u8; 20]);
    let node_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 6881);
    let node = Node::new(node_id, node_addr);

    let msg = DhtMessage {
        transaction_id: tid.clone(),
        sender_id: Some(responder_id),
        query: None,
        response: Some(DhtResponse::FindNode {
            id: responder_id,
            nodes: vec![node],
        }),
    };

    let encoded = msg.encode().unwrap();
    let parsed = DhtMessage::parse(&encoded).unwrap();

    match parsed.response.unwrap() {
        DhtResponse::FindNode { id, nodes } => {
            assert_eq!(id.0, responder_id.0);
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].id.0, node_id.0);
        }
        _ => panic!("wrong response type"),
    }
}

#[test]
fn test_dht_parse_get_peers_with_values() {
    let responder_id = NodeId::generate();
    let tid = bytes::Bytes::from_static(b"gp");
    let token = bytes::Bytes::from_static(b"secret");

    let peer_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 51413);

    let msg = DhtMessage {
        transaction_id: tid.clone(),
        sender_id: Some(responder_id),
        query: None,
        response: Some(DhtResponse::GetPeers {
            id: responder_id,
            token: token.clone(),
            peers: Some(vec![peer_addr]),
            nodes: None,
        }),
    };

    let encoded = msg.encode().unwrap();
    let parsed = DhtMessage::parse(&encoded).unwrap();

    match parsed.response.unwrap() {
        DhtResponse::GetPeers {
            id,
            token: t,
            peers,
            nodes,
        } => {
            assert_eq!(id.0, responder_id.0);
            assert_eq!(t, token);
            let peers = peers.unwrap();
            assert_eq!(peers.len(), 1);
            assert_eq!(peers[0], peer_addr);
            assert!(nodes.is_none());
        }
        _ => panic!("wrong response type"),
    }
}

#[test]
fn test_dht_parse_error_response() {
    let tid = bytes::Bytes::from_static(b"er");

    let msg = DhtMessage {
        transaction_id: tid.clone(),
        sender_id: None,
        query: None,
        response: Some(DhtResponse::Error {
            code: 201,
            message: "Generic Error".to_string(),
        }),
    };

    let encoded = msg.encode().unwrap();
    let parsed = DhtMessage::parse(&encoded).unwrap();

    match parsed.response.unwrap() {
        DhtResponse::Error { code, message } => {
            assert_eq!(code, 201);
            assert_eq!(message, "Generic Error");
        }
        _ => panic!("wrong response type"),
    }
}

// ========================
// Malformed message handling
// ========================

#[test]
fn test_dht_parse_invalid_bencode() {
    let result = DhtMessage::parse(b"not valid bencode");
    assert!(result.is_err());
}

#[test]
fn test_dht_parse_missing_transaction_id() {
    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"y"),
        oxidebt_bencode::Value::string("q"),
    );

    let encoded = oxidebt_bencode::encode(&oxidebt_bencode::Value::Dict(dict)).unwrap();
    let result = DhtMessage::parse(&encoded);
    assert!(result.is_err());
}

#[test]
fn test_dht_parse_missing_message_type() {
    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"t"),
        oxidebt_bencode::Value::Bytes(bytes::Bytes::from_static(b"aa")),
    );

    let encoded = oxidebt_bencode::encode(&oxidebt_bencode::Value::Dict(dict)).unwrap();
    let result = DhtMessage::parse(&encoded);
    assert!(result.is_err());
}

#[test]
fn test_dht_parse_unknown_message_type() {
    let mut dict = BTreeMap::new();
    dict.insert(
        bytes::Bytes::from_static(b"t"),
        oxidebt_bencode::Value::Bytes(bytes::Bytes::from_static(b"aa")),
    );
    dict.insert(
        bytes::Bytes::from_static(b"y"),
        oxidebt_bencode::Value::string("x"),
    );

    let encoded = oxidebt_bencode::encode(&oxidebt_bencode::Value::Dict(dict)).unwrap();
    let result = DhtMessage::parse(&encoded);
    assert!(result.is_err());
}

// ========================
// NodeId edge cases
// ========================

#[test]
fn test_node_id_bucket_index_adjacent() {
    let id1 = NodeId([0u8; 20]);
    let mut id2_bytes = [0u8; 20];
    id2_bytes[19] = 1; // Distance differs in last byte only
    let id2 = NodeId(id2_bytes);

    // Bucket index should be 159 (last bit)
    assert_eq!(id1.bucket_index(&id2), 159);
}

#[test]
fn test_node_id_bucket_index_self() {
    let id = NodeId([1u8; 20]);
    // Distance with self is all zeros - implementation returns 159 as fallback
    assert_eq!(id.bucket_index(&id), 159);
}

// ========================
// RoutingTable advanced tests
// ========================

#[test]
fn test_routing_table_find_closest_returns_sorted() {
    let our_id = NodeId([0u8; 20]);
    let table = RoutingTable::new(our_id);

    // Add nodes with known distances
    for i in 0u8..10 {
        let mut id_bytes = [0u8; 20];
        id_bytes[0] = i + 1; // Increasing distance from our_id (all zeros)
        let id = NodeId(id_bytes);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, i + 1)), 6881);
        table.add_node(Node::new(id, addr));
    }

    let target = NodeId([0u8; 20]); // same as our_id
    let closest = table.find_closest(&target, 5);
    assert!(closest.len() <= 5);

    // Verify sorted by distance (first byte determines distance here)
    for pair in closest.windows(2) {
        let d1 = target.distance(&pair[0].id);
        let d2 = target.distance(&pair[1].id);
        assert!(d1 <= d2, "Nodes should be sorted by distance to target");
    }
}

#[test]
fn test_routing_table_node_count() {
    let our_id = NodeId::generate();
    let table = RoutingTable::new(our_id);

    assert_eq!(table.node_count(), 0);

    for i in 0..5 {
        let id = NodeId::generate();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, i + 1)), 6881);
        table.add_node(Node::new(id, addr));
    }

    assert!(table.node_count() >= 1);
}

#[test]
fn test_node_failure_tracking() {
    let id = NodeId::generate();
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6881);
    let mut node = Node::new(id, addr);

    // Fresh node is good
    assert!(node.is_good());
    assert!(!node.is_bad());

    // One failure doesn't make it bad
    node.fail();
    assert!(!node.is_bad());

    // Two failures doesn't make it bad
    node.fail();
    assert!(!node.is_bad());

    // Three failures makes it bad
    node.fail();
    assert!(node.is_bad());
}
