use super::*;
use crate::choking::PeerStats;
use crate::message::Handshake;
use bytes::Bytes;
use std::net::SocketAddr;

#[test]
fn test_peer_id_generation() {
    let id = PeerId::generate();
    assert_eq!(&id.0[..8], b"-OX0001-");
}

#[test]
fn test_peer_id_client_detection() {
    let id = PeerId::generate();
    let name = id.client_name().unwrap();
    assert!(name.starts_with("OxideBT"));
}

#[test]
fn test_bitfield_new() {
    let bf = Bitfield::new(100);
    assert_eq!(bf.piece_count(), 100);
    assert_eq!(bf.count(), 0);
    assert!(bf.is_empty());
}

#[test]
fn test_bitfield_set_get() {
    let mut bf = Bitfield::new(100);
    bf.set_piece(0);
    bf.set_piece(50);
    bf.set_piece(99);

    assert!(bf.has_piece(0));
    assert!(!bf.has_piece(1));
    assert!(bf.has_piece(50));
    assert!(bf.has_piece(99));
    assert_eq!(bf.count(), 3);
}

#[test]
fn test_bitfield_full() {
    let bf = Bitfield::full(16);
    assert!(bf.is_complete());
    assert_eq!(bf.count(), 16);
    for i in 0..16 {
        assert!(bf.has_piece(i));
    }
}

#[test]
fn test_bitfield_from_bytes() {
    let bytes = vec![0b11110000, 0b00001111];
    let bf = Bitfield::from_bytes(&bytes, 16).unwrap();

    for i in 0..4 {
        assert!(bf.has_piece(i));
    }
    for i in 4..8 {
        assert!(!bf.has_piece(i));
    }
    for i in 8..12 {
        assert!(!bf.has_piece(i));
    }
    for i in 12..16 {
        assert!(bf.has_piece(i));
    }
}

#[test]
fn test_bitfield_spare_bits() {
    let bf = Bitfield::full(10);
    assert_eq!(bf.count(), 10);
    let bytes = bf.as_bytes();
    assert_eq!(bytes.len(), 2);
    assert_eq!(bytes[1] & 0b00111111, 0);
}

#[test]
fn test_message_keepalive() {
    let msg = Message::KeepAlive;
    let encoded = msg.encode();
    assert!(encoded.is_empty());

    let decoded = Message::parse(&[]).unwrap();
    assert_eq!(decoded, Message::KeepAlive);
}

#[test]
fn test_message_choke() {
    let msg = Message::Choke;
    let encoded = msg.encode();
    assert_eq!(encoded.as_ref(), &[0u8]);

    let decoded = Message::parse(&[0]).unwrap();
    assert_eq!(decoded, Message::Choke);
}

#[test]
fn test_message_have() {
    let msg = Message::Have { piece_index: 42 };
    let encoded = msg.encode();
    assert_eq!(encoded.len(), 5);

    let decoded = Message::parse(&encoded).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn test_message_request() {
    let msg = Message::Request {
        index: 10,
        begin: 16384,
        length: 16384,
    };
    let encoded = msg.encode();
    assert_eq!(encoded.len(), 13);

    let decoded = Message::parse(&encoded).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn test_message_piece() {
    let data = Bytes::from(vec![1, 2, 3, 4, 5]);
    let msg = Message::Piece {
        index: 5,
        begin: 0,
        data: data.clone(),
    };
    let encoded = msg.encode();

    let decoded = Message::parse(&encoded).unwrap();
    match decoded {
        Message::Piece {
            index,
            begin,
            data: d,
        } => {
            assert_eq!(index, 5);
            assert_eq!(begin, 0);
            assert_eq!(d, data);
        }
        _ => panic!("wrong message type"),
    }
}

#[test]
fn test_handshake_encode_decode() {
    let info_hash = [1u8; 20];
    let peer_id = [2u8; 20];
    let handshake = Handshake::new(info_hash, peer_id);

    let encoded = handshake.encode();
    assert_eq!(encoded.len(), 68);

    let decoded = Handshake::parse(&encoded).unwrap();
    assert_eq!(decoded.info_hash, info_hash);
    assert_eq!(decoded.peer_id, peer_id);
    assert!(decoded.supports_extensions());
    assert!(decoded.supports_dht());
}

#[test]
fn test_handshake_invalid_length() {
    let result = Handshake::parse(&[0u8; 50]);
    assert!(matches!(result, Err(PeerError::InvalidHandshake(_))));
}

#[test]
fn test_block_request() {
    let req = BlockRequest {
        piece_index: 0,
        offset: 0,
        length: 16384,
    };
    assert_eq!(req.piece_index, 0);
    assert_eq!(req.offset, 0);
    assert_eq!(req.length, 16384);
}

#[test]
fn test_piece_manager_pick_rarest() {
    let pm = PieceManager::new(10, 262144, 2621440);

    pm.mark_verification_complete();

    // Mark 4 pieces complete to get past cold start mode (random selection)
    pm.mark_piece_complete(0);
    pm.mark_piece_complete(1);
    pm.mark_piece_complete(2);
    pm.mark_piece_complete(3);

    let mut peer_bf = Bitfield::new(10);
    peer_bf.set_piece(5);
    peer_bf.set_piece(7);
    pm.update_availability(&peer_bf);

    let mut peer_bf2 = Bitfield::new(10);
    peer_bf2.set_piece(5);
    pm.update_availability(&peer_bf2);

    // Now rarest-first should pick piece 7 (availability 1) over piece 5 (availability 2)
    let picked = pm.pick_piece(&peer_bf).unwrap();
    assert_eq!(picked, 7);
}

#[test]
fn test_piece_manager_sequential() {
    let pm = PieceManager::new(10, 262144, 2621440);

    pm.mark_verification_complete();

    let mut peer_bf = Bitfield::new(10);
    peer_bf.set_piece(3);
    peer_bf.set_piece(5);
    peer_bf.set_piece(7);

    let picked = pm.pick_piece_sequential(&peer_bf).unwrap();
    assert_eq!(picked, 3);
}

#[test]
fn test_choking_algorithm() {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};

    let mut algo = ChokingAlgorithm::default();
    let mut peers = HashMap::new();

    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 6881);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 5)), 6881);

    let mut stats1 = PeerStats::new(addr1);
    stats1.is_interested = true;
    stats1.download_rate = 100.0;
    stats1.is_choked_by_us = true;

    let mut stats2 = PeerStats::new(addr2);
    stats2.is_interested = true;
    stats2.download_rate = 50.0;
    stats2.is_choked_by_us = true;

    peers.insert(addr1, stats1);
    peers.insert(addr2, stats2);

    let decisions = algo.run(&peers);
    assert!(decisions.values().all(|&d| d == ChokingDecision::NoChange));
}

#[test]
fn stress_test_bitfield_large() {
    let sizes = [100, 1000, 10_000, 100_000];

    for size in sizes {
        let mut bf = Bitfield::new(size);

        for i in (0..size).step_by(2) {
            bf.set_piece(i);
        }

        assert_eq!(bf.count(), size.div_ceil(2));

        for i in 0..size {
            if i % 2 == 0 {
                assert!(bf.has_piece(i), "Piece {} should be set", i);
            } else {
                assert!(!bf.has_piece(i), "Piece {} should not be set", i);
            }
        }
    }
}

#[test]
fn stress_test_bitfield_roundtrip() {
    for piece_count in [1, 7, 8, 9, 15, 16, 17, 100, 1000] {
        let mut bf = Bitfield::new(piece_count);

        for i in (0..piece_count).step_by(3) {
            bf.set_piece(i);
        }

        let bytes = bf.to_bytes();
        let reconstructed = Bitfield::from_bytes(&bytes, piece_count).unwrap();

        assert_eq!(bf.count(), reconstructed.count());
        for i in 0..piece_count {
            assert_eq!(bf.has_piece(i), reconstructed.has_piece(i));
        }
    }
}

#[test]
fn stress_test_message_encoding_all_types() {
    let messages = vec![
        Message::KeepAlive,
        Message::Choke,
        Message::Unchoke,
        Message::Interested,
        Message::NotInterested,
        Message::Have { piece_index: 0 },
        Message::Have {
            piece_index: u32::MAX,
        },
        Message::Bitfield(Bytes::from(vec![0xFF, 0x00, 0xAA])),
        Message::Request {
            index: 0,
            begin: 0,
            length: 16384,
        },
        Message::Request {
            index: u32::MAX,
            begin: u32::MAX,
            length: u32::MAX,
        },
        Message::Piece {
            index: 0,
            begin: 0,
            data: Bytes::from(vec![1, 2, 3, 4, 5]),
        },
        Message::Cancel {
            index: 10,
            begin: 100,
            length: 16384,
        },
        Message::Port(6881),
        Message::HaveAll,
        Message::HaveNone,
        Message::SuggestPiece { piece_index: 42 },
        Message::RejectRequest {
            index: 1,
            begin: 2,
            length: 3,
        },
        Message::AllowedFast { piece_index: 99 },
    ];

    for msg in messages {
        let encoded = msg.encode();
        let decoded = Message::parse(&encoded).unwrap();
        assert_eq!(decoded, msg);
    }
}

#[test]
fn stress_test_message_large_piece() {
    let data = Bytes::from(vec![0xAB; 16384]);
    let msg = Message::Piece {
        index: 0,
        begin: 0,
        data: data.clone(),
    };

    let encoded = msg.encode();
    let decoded = Message::parse(&encoded).unwrap();

    match decoded {
        Message::Piece {
            index,
            begin,
            data: d,
        } => {
            assert_eq!(index, 0);
            assert_eq!(begin, 0);
            assert_eq!(d.len(), 16384);
            assert_eq!(d, data);
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn stress_test_handshake_variations() {
    for i in 0..100 {
        let mut info_hash = [0u8; 20];
        let mut peer_id = [0u8; 20];

        for j in 0..20 {
            info_hash[j] = ((i + j) % 256) as u8;
            peer_id[j] = ((i * 2 + j) % 256) as u8;
        }

        let handshake = Handshake::new(info_hash, peer_id);
        let encoded = handshake.encode();
        let decoded = Handshake::parse(&encoded).unwrap();

        assert_eq!(decoded.info_hash, info_hash);
        assert_eq!(decoded.peer_id, peer_id);
    }
}

#[test]
fn stress_test_piece_manager_availability() {
    let pm = PieceManager::new(100, 262144, 26214400);

    pm.mark_verification_complete();

    for i in 0..50 {
        let mut bf = Bitfield::new(100);
        for j in (i % 10..100).step_by(10) {
            bf.set_piece(j);
        }
        pm.update_availability(&bf);
    }

    let bf = Bitfield::full(100);
    for _ in 0..10 {
        if let Some(piece) = pm.pick_piece(&bf) {
            pm.start_piece(piece);
            pm.mark_piece_complete(piece);
        }
    }
}

#[test]
fn stress_test_piece_manager_block_requests() {
    let piece_length = 262144u64;
    let total_size = piece_length * 10;
    let pm = PieceManager::new(10, piece_length, total_size);

    pm.start_piece(0);
    let requests = pm.get_block_requests(0);

    assert_eq!(requests.len(), 16);

    let mut total_length = 0u32;
    for req in &requests {
        assert_eq!(req.piece_index, 0);
        assert_eq!(req.length, 16384);
        total_length += req.length;
    }
    assert_eq!(total_length as u64, piece_length);
}

#[test]
fn stress_test_piece_manager_complete_flow() {
    let pm = PieceManager::new(5, 32768, 163840);

    pm.mark_verification_complete();

    let peer_bf = Bitfield::full(5);
    pm.update_availability(&peer_bf);

    for _ in 0..5 {
        let piece = pm.pick_piece(&peer_bf).unwrap();
        pm.start_piece(piece);

        let requests = pm.get_block_requests(piece);
        for req in requests {
            let block = Block {
                piece_index: req.piece_index,
                offset: req.offset,
                data: Bytes::from(vec![0u8; req.length as usize]),
            };
            let complete = pm.receive_block(block).unwrap();
            if complete {
                pm.mark_piece_complete(piece);
            }
        }
    }

    assert!(pm.is_complete());
}

#[test]
fn stress_test_peer_id_uniqueness() {
    use std::collections::HashSet;

    let mut ids = HashSet::new();
    for _ in 0..1000 {
        let id = PeerId::generate();
        let bytes = id.as_bytes().to_vec();
        ids.insert(bytes);
    }

    assert_eq!(ids.len(), 1000);
}

#[test]
fn stress_test_bitfield_operations() {
    let mut bf1 = Bitfield::new(1000);
    let mut bf2 = Bitfield::new(1000);

    for i in 0..500 {
        bf1.set_piece(i);
    }
    for i in 250..750 {
        bf2.set_piece(i);
    }

    let mut common = 0;
    for i in 0..1000 {
        if bf1.has_piece(i) && bf2.has_piece(i) {
            common += 1;
        }
    }
    assert_eq!(common, 250);

    let mut interesting = 0;
    for i in 0..1000 {
        if bf2.has_piece(i) && !bf1.has_piece(i) {
            interesting += 1;
        }
    }
    assert_eq!(interesting, 250);
}

#[test]
fn stress_test_message_rapid_encode_decode() {
    let data = Bytes::from(vec![0xAB; 1000]);

    for _ in 0..10_000 {
        let msg = Message::Piece {
            index: 42,
            begin: 16384,
            data: data.clone(),
        };
        let encoded = msg.encode();
        let decoded = Message::parse(&encoded).unwrap();
        assert_eq!(decoded, msg);
    }
}

#[test]
fn stress_test_choking_many_peers() {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};

    let mut algo = ChokingAlgorithm::new(
        4,
        std::time::Duration::from_secs(10),
        std::time::Duration::from_secs(30),
    );
    let mut peers = HashMap::new();

    for i in 0..100 {
        let addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(10, 0, (i / 256) as u8, (i % 256) as u8)),
            6881,
        );
        let mut stats = PeerStats::new(addr);
        stats.is_interested = i % 2 == 0;
        stats.download_rate = (i * 10) as f64;
        stats.is_choked_by_us = true;
        peers.insert(addr, stats);
    }

    let decisions = algo.run(&peers);
    assert_eq!(decisions.len(), 100);
}

#[test]
fn stress_test_bitfield_edge_cases() {
    let edge_cases = [
        1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257,
    ];

    for &count in &edge_cases {
        let bf = Bitfield::full(count);
        assert_eq!(bf.count(), count);
        assert!(bf.is_complete());

        let bytes = bf.to_bytes();
        let expected_len = count.div_ceil(8);
        assert_eq!(bytes.len(), expected_len);

        let reconstructed = Bitfield::from_bytes(&bytes, count).unwrap();
        assert_eq!(reconstructed.count(), count);
    }
}

#[test]
fn stress_test_extended_messages() {
    let handshake = ExtensionHandshake::new();
    let encoded = handshake.encode();
    assert!(!encoded.is_empty());

    let msg = Message::Extended(ExtensionMessage::Handshake(encoded.clone()));
    let msg_encoded = msg.encode();

    let decoded = Message::parse(&msg_encoded).unwrap();
    match decoded {
        Message::Extended(ExtensionMessage::Handshake(data)) => {
            assert_eq!(data, encoded);
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_piece_manager_verification_initial_state() {
    let pm = PieceManager::new(10, 262144, 2621440);

    assert!(!pm.is_verification_complete());

    assert_eq!(pm.verified_count(), 0);

    for i in 0..10 {
        assert!(!pm.is_piece_verified(i));
    }
}

#[test]
fn test_piece_manager_mark_piece_verified() {
    let pm = PieceManager::new(10, 262144, 2621440);

    pm.mark_piece_verified(0);
    pm.mark_piece_verified(5);
    pm.mark_piece_verified(9);

    assert_eq!(pm.verified_count(), 3);

    assert!(pm.is_piece_verified(0));
    assert!(!pm.is_piece_verified(1));
    assert!(pm.is_piece_verified(5));
    assert!(!pm.is_piece_verified(6));
    assert!(pm.is_piece_verified(9));
}

#[test]
fn test_piece_manager_verification_complete() {
    let pm = PieceManager::new(10, 262144, 2621440);

    pm.mark_verification_complete();

    assert!(pm.is_verification_complete());

    assert_eq!(pm.verified_count(), 10);
    for i in 0..10 {
        assert!(pm.is_piece_verified(i));
    }
}

#[test]
fn test_piece_manager_pick_only_verified_pieces() {
    let pm = PieceManager::new(10, 262144, 2621440);

    let peer_bf = Bitfield::full(10);
    pm.update_availability(&peer_bf);

    let picked = pm.pick_piece(&peer_bf).unwrap();
    assert!(picked < 10);

    pm.start_piece(picked);

    let picked2 = pm.pick_piece(&peer_bf).unwrap();
    assert_ne!(picked2, picked);
}

#[test]
fn test_piece_manager_pick_all_after_verification_complete() {
    let pm = PieceManager::new(10, 262144, 2621440);

    let peer_bf = Bitfield::full(10);
    pm.update_availability(&peer_bf);

    let picked_before = pm.pick_piece(&peer_bf);
    assert!(picked_before.is_some());

    pm.mark_verification_complete();

    let picked = pm.pick_piece(&peer_bf);
    assert!(picked.is_some());
}

#[test]
fn test_piece_manager_sequential_pick_only_verified() {
    let pm = PieceManager::new(10, 262144, 2621440);

    let peer_bf = Bitfield::full(10);

    assert_eq!(pm.pick_piece_sequential(&peer_bf).unwrap(), 0);

    pm.start_piece(0);

    assert_eq!(pm.pick_piece_sequential(&peer_bf).unwrap(), 1);
}

#[test]
fn test_piece_manager_verified_count_with_completion() {
    let pm = PieceManager::new(10, 262144, 2621440);

    pm.mark_piece_verified(0);
    pm.mark_piece_verified(1);
    pm.mark_piece_verified(2);
    assert_eq!(pm.verified_count(), 3);

    pm.mark_verification_complete();

    assert_eq!(pm.verified_count(), 10);
}

#[test]
fn stress_test_piece_manager_progressive_verification() {
    let pm = PieceManager::new(100, 262144, 26214400);
    let peer_bf = Bitfield::full(100);
    pm.update_availability(&peer_bf);

    assert_eq!(pm.verified_count(), 0);
    let picked = pm.pick_piece(&peer_bf);
    assert!(picked.is_some());

    for i in 0..10 {
        pm.mark_piece_verified(i);
    }
    assert_eq!(pm.verified_count(), 10);

    for i in 10..100 {
        pm.mark_piece_verified(i);
    }
    assert_eq!(pm.verified_count(), 100);

    pm.mark_verification_complete();

    assert_eq!(pm.verified_count(), 100);
    assert!(pm.is_verification_complete());
}

// ========================
// Extension Handshake tests
// ========================

#[test]
fn test_extension_handshake_encode_decode() {
    let hs = ExtensionHandshake::new()
        .with_metadata_size(12345)
        .with_listen_port(6881);

    let encoded = hs.encode();
    let parsed = ExtensionHandshake::parse(&encoded).unwrap();

    assert_eq!(parsed.metadata_size, Some(12345));
    assert_eq!(parsed.listen_port, Some(6881));
    assert_eq!(parsed.client, Some("oxidebt/0.1.0".to_string()));
    assert!(parsed.ut_metadata.is_some());
    assert!(parsed.ut_pex.is_some());
    assert_eq!(parsed.reqq, Some(250));
}

#[test]
fn test_extension_handshake_default() {
    let hs = ExtensionHandshake::default();
    assert_eq!(hs.metadata_size, None);
    assert_eq!(hs.listen_port, None);
    assert!(hs.ut_metadata.is_some());
    assert!(hs.ut_pex.is_some());
}

#[test]
fn test_extension_handshake_parse_empty() {
    let result = ExtensionHandshake::parse(b"");
    assert!(result.is_none());
}

#[test]
fn test_extension_handshake_parse_invalid() {
    let result = ExtensionHandshake::parse(b"not bencode");
    assert!(result.is_none());
}

// ========================
// MetadataMessage tests
// ========================

#[test]
fn test_metadata_request_encode_decode() {
    let msg = extension::MetadataMessage::Request { piece: 5 };
    let encoded = msg.encode();
    let parsed = extension::MetadataMessage::parse(&encoded).unwrap();

    match parsed {
        extension::MetadataMessage::Request { piece } => assert_eq!(piece, 5),
        _ => panic!("wrong message type"),
    }
}

#[test]
fn test_metadata_data_encode_decode() {
    let data = Bytes::from(vec![0xAB; 1024]);
    let msg = extension::MetadataMessage::Data {
        piece: 3,
        total_size: 16384,
        data: data.clone(),
    };
    let encoded = msg.encode();
    let parsed = extension::MetadataMessage::parse(&encoded).unwrap();

    match parsed {
        extension::MetadataMessage::Data {
            piece,
            total_size,
            data: parsed_data,
        } => {
            assert_eq!(piece, 3);
            assert_eq!(total_size, 16384);
            assert_eq!(parsed_data, data);
        }
        _ => panic!("wrong message type"),
    }
}

#[test]
fn test_metadata_reject_encode_decode() {
    let msg = extension::MetadataMessage::Reject { piece: 7 };
    let encoded = msg.encode();
    let parsed = extension::MetadataMessage::parse(&encoded).unwrap();

    match parsed {
        extension::MetadataMessage::Reject { piece } => assert_eq!(piece, 7),
        _ => panic!("wrong message type"),
    }
}

#[test]
fn test_metadata_message_parse_invalid() {
    let result = extension::MetadataMessage::parse(b"");
    assert!(result.is_none());

    let result = extension::MetadataMessage::parse(b"not bencode");
    assert!(result.is_none());
}

// ========================
// PEX Message (extension module) tests
// ========================

#[test]
fn test_extension_pex_encode_decode() {
    let mut msg = extension::PexMessage::new();
    msg.added.push(extension::PexPeer {
        ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 1)),
        port: 6881,
    });
    msg.added_flags.push(0x01); // encryption

    msg.added6.push(extension::PexPeer {
        ip: std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        port: 6882,
    });

    let encoded = msg.encode();
    let parsed = extension::PexMessage::parse(&encoded).unwrap();

    assert_eq!(parsed.added.len(), 1);
    assert_eq!(parsed.added[0].port, 6881);
    assert_eq!(parsed.added_flags.len(), 1);
    assert_eq!(parsed.added_flags[0], 0x01);
    assert_eq!(parsed.added6.len(), 1);
    assert_eq!(parsed.added6[0].port, 6882);
}

#[test]
fn test_extension_pex_with_dropped() {
    let mut msg = extension::PexMessage::new();
    msg.dropped.push(extension::PexPeer {
        ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1)),
        port: 51413,
    });
    msg.dropped6.push(extension::PexPeer {
        ip: std::net::IpAddr::V6(std::net::Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        port: 6881,
    });

    let encoded = msg.encode();
    let parsed = extension::PexMessage::parse(&encoded).unwrap();

    assert_eq!(parsed.dropped.len(), 1);
    assert_eq!(parsed.dropped[0].port, 51413);
    assert_eq!(parsed.dropped6.len(), 1);
    assert_eq!(parsed.dropped6[0].port, 6881);
}

#[test]
fn test_extension_pex_parse_empty() {
    let result = extension::PexMessage::parse(b"");
    assert!(result.is_none());
}

// ========================
// Handshake tests
// ========================

#[test]
fn test_handshake_supports_extensions() {
    let mut hs = Handshake::new([0xAB; 20], [0xCD; 20]);
    // BEP-10: bit 20 (0x00100000 in reserved)
    hs.reserved[5] |= 0x10;
    assert!(hs.supports_extensions());
}

#[test]
fn test_handshake_supports_dht() {
    let mut hs = Handshake::new([0xAB; 20], [0xCD; 20]);
    // BEP-5: bit 0
    hs.reserved[7] |= 0x01;
    assert!(hs.supports_dht());
}

#[test]
fn test_handshake_supports_fast() {
    let mut hs = Handshake::new([0xAB; 20], [0xCD; 20]);
    // BEP-6: bit 2
    hs.reserved[7] |= 0x04;
    assert!(hs.supports_fast());
}

// ========================
// Bitfield edge cases
// ========================

#[test]
fn test_bitfield_clear_piece() {
    let mut bf = Bitfield::new(100);
    bf.set_piece(50);
    assert!(bf.has_piece(50));

    bf.clear_piece(50);
    assert!(!bf.has_piece(50));
}

#[test]
fn test_bitfield_from_bytes_spare_bits() {
    // 10 pieces = 2 bytes, but only 10 bits are valid
    let bytes = [0xFF, 0xC0]; // 11111111 11000000 - all 10 pieces set
    let bf = Bitfield::from_bytes(&bytes, 10).unwrap();
    assert_eq!(bf.count(), 10);
    assert!(bf.is_complete());
}

#[test]
fn test_bitfield_from_bytes_spare_bits_ignored() {
    // 10 pieces = need 2 bytes, spare bits should be 0 per spec
    // but the implementation tolerates set spare bits (lenient parsing)
    let bytes = [0xFF, 0xFF]; // Spare bits are set
    let result = Bitfield::from_bytes(&bytes, 10);
    assert!(result.is_ok());
    // Only the first 10 bits count as pieces
    let bf = result.unwrap();
    assert_eq!(bf.count(), 10);
}

#[test]
fn test_bitfield_missing_pieces() {
    let mut our = Bitfield::new(10);
    our.set_piece(0);
    our.set_piece(1);

    let mut peer = Bitfield::new(10);
    peer.set_piece(1);
    peer.set_piece(2);
    peer.set_piece(3);

    let missing = peer.missing_pieces(&our);
    // Pieces peer has that we don't
    assert!(missing.contains(&2));
    assert!(missing.contains(&3));
    assert!(!missing.contains(&1)); // We already have this
}

#[test]
fn test_bitfield_to_bytes() {
    let mut bf = Bitfield::new(16);
    bf.set_piece(0);
    bf.set_piece(8);

    let bytes = bf.to_bytes();
    assert_eq!(bytes.len(), 2);
}

// ========================
// PeerId additional tests
// ========================

#[test]
fn test_peer_id_from_bytes() {
    let bytes = [0xAB; 20];
    let id = PeerId::from_bytes(&bytes).unwrap();
    assert_eq!(id.0, bytes);
}

#[test]
fn test_peer_id_from_bytes_wrong_length() {
    assert!(PeerId::from_bytes(&[0u8; 10]).is_none());
}

#[test]
fn test_peer_id_as_bytes() {
    let id = PeerId::generate();
    let bytes = id.as_bytes();
    assert_eq!(bytes.len(), 20);
}

// ========================
// FastExtensionState tests
// ========================

#[test]
fn test_fast_extension_state_init() {
    let mut state = FastExtensionState::new();
    let info_hash = [0xAB; 20];
    state.init_for_peer(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 1)),
        &info_hash,
        1000,
    );

    let allowed = state.get_outgoing_allowed_fast();
    assert!(!allowed.is_empty());
}

#[test]
fn test_fast_extension_can_request_while_choked() {
    let mut state = FastExtensionState::new();

    // Add incoming allowed fast piece
    state.add_incoming_allowed_fast(42);
    assert!(state.can_request_while_choked(42));
    assert!(!state.can_request_while_choked(43));
}

#[test]
fn test_fast_extension_suggestions() {
    let mut state = FastExtensionState::new();
    state.add_suggestion(10);
    state.add_suggestion(20);

    state.clear_suggestions();
    // After clearing, no suggestions should remain
    // (suggestions are kept in a separate vec, clearing removes them)
}
