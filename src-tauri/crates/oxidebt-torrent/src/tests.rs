use super::*;
use bytes::Bytes;
use oxidebt_bencode::{encode, Value};
use std::collections::BTreeMap;

fn create_test_torrent_v1() -> Vec<u8> {
    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(12345));
    info.insert(Bytes::from_static(b"name"), Value::string("test.txt"));
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(262144));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(vec![0u8; 20])),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://tracker.example.com/announce"),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    encode(&Value::Dict(torrent)).unwrap()
}

fn create_multifile_torrent_v1() -> Vec<u8> {
    let file1 = {
        let mut f = BTreeMap::new();
        f.insert(Bytes::from_static(b"length"), Value::Integer(1000));
        f.insert(
            Bytes::from_static(b"path"),
            Value::List(vec![Value::string("subdir"), Value::string("file1.txt")]),
        );
        Value::Dict(f)
    };

    let file2 = {
        let mut f = BTreeMap::new();
        f.insert(Bytes::from_static(b"length"), Value::Integer(2000));
        f.insert(
            Bytes::from_static(b"path"),
            Value::List(vec![Value::string("file2.txt")]),
        );
        Value::Dict(f)
    };

    let mut info = BTreeMap::new();
    info.insert(
        Bytes::from_static(b"files"),
        Value::List(vec![file1, file2]),
    );
    info.insert(Bytes::from_static(b"name"), Value::string("test_folder"));
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(262144));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(vec![0u8; 20])),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://tracker.example.com/announce"),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    encode(&Value::Dict(torrent)).unwrap()
}

#[test]
fn test_parse_v1_single_file() {
    let data = create_test_torrent_v1();
    let meta = Metainfo::from_bytes(&data).unwrap();

    assert_eq!(meta.info.name, "test.txt");
    assert_eq!(meta.info.piece_length, 262144);
    assert_eq!(meta.info.total_length, 12345);
    assert_eq!(meta.info.files.len(), 1);
    assert_eq!(meta.info.files[0].length, 12345);
    assert_eq!(meta.version, TorrentVersion::V1);
    assert!(meta.info_hash.v1().is_some());
}

#[test]
fn test_parse_v1_multifile() {
    let data = create_multifile_torrent_v1();
    let meta = Metainfo::from_bytes(&data).unwrap();

    assert_eq!(meta.info.name, "test_folder");
    assert_eq!(meta.info.total_length, 3000);
    assert_eq!(meta.info.files.len(), 2);
    assert_eq!(meta.info.files[0].length, 1000);
    assert_eq!(meta.info.files[1].length, 2000);
}

#[test]
fn test_info_hash_v1_hex() {
    let hash = InfoHashV1::from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap();
    assert_eq!(hash.to_hex(), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
}

#[test]
fn test_info_hash_v2_hex() {
    let hash =
        InfoHashV2::from_hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
            .unwrap();
    assert_eq!(
        hash.to_hex(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn test_info_hash_url_encode() {
    let hash = InfoHashV1([
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
        0xFF, 0x00, 0x11, 0x22, 0x33,
    ]);
    let encoded = hash.url_encode();
    assert!(encoded.contains("%"));
}

#[test]
fn test_magnet_parse_v1() {
    let uri = "magnet:?xt=urn:btih:da39a3ee5e6b4b0d3255bfef95601890afd80709&dn=test&tr=http://tracker.example.com/announce";
    let magnet = MagnetLink::parse(uri).unwrap();

    assert!(magnet.info_hash.v1().is_some());
    assert_eq!(magnet.display_name, Some("test".to_string()));
    assert_eq!(magnet.trackers.len(), 1);
}

#[test]
fn test_magnet_roundtrip() {
    let original = MagnetLink {
        info_hash: InfoHash::V1(
            InfoHashV1::from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap(),
        ),
        display_name: Some("test file".to_string()),
        trackers: vec!["http://tracker.example.com/announce".to_string()],
        web_seeds: vec![],
        peer_addresses: vec![],
    };

    let uri = original.to_uri();
    let parsed = MagnetLink::parse(&uri).unwrap();

    assert_eq!(
        parsed.info_hash.v1().unwrap().to_hex(),
        original.info_hash.v1().unwrap().to_hex()
    );
}

#[test]
fn test_merkle_tree_small() {
    let data = vec![0u8; 16384];
    let tree = MerkleTree::from_piece_data(&data);

    assert!(!tree.root().iter().all(|&b| b == 0));
}

#[test]
fn test_merkle_tree_proof() {
    let data = vec![0u8; 16384 * 4];
    let tree = MerkleTree::from_piece_data(&data);

    let proof = tree.generate_proof(0);
    let block = &data[0..16384];
    assert!(tree.verify_block(0, block, &proof).unwrap());
}

#[test]
fn test_piece_hash_extraction() {
    let data = create_test_torrent_v1();
    let meta = Metainfo::from_bytes(&data).unwrap();

    let hash = meta.piece_hash(0).unwrap();
    assert_eq!(hash, [0u8; 20]);
    assert!(meta.piece_hash(1).is_none());
}

#[test]
fn test_tracker_urls() {
    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(12345));
    info.insert(Bytes::from_static(b"name"), Value::string("test.txt"));
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(262144));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(vec![0u8; 20])),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://main.tracker.com/announce"),
    );
    torrent.insert(
        Bytes::from_static(b"announce-list"),
        Value::List(vec![
            Value::List(vec![
                Value::string("http://tier1a.com/announce"),
                Value::string("http://tier1b.com/announce"),
            ]),
            Value::List(vec![Value::string("http://tier2.com/announce")]),
        ]),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let data = encode(&Value::Dict(torrent)).unwrap();
    let meta = Metainfo::from_bytes(&data).unwrap();

    let urls = meta.tracker_urls();
    assert_eq!(urls.len(), 4);
    assert_eq!(urls[0], "http://main.tracker.com/announce");
}

// ========================
// InfoHashV1 error paths
// ========================

#[test]
fn test_info_hash_v1_from_bytes_valid() {
    let bytes = [0xAB; 20];
    let hash = InfoHashV1::from_bytes(&bytes).unwrap();
    assert_eq!(hash.0, bytes);
}

#[test]
fn test_info_hash_v1_from_bytes_wrong_length() {
    let result = InfoHashV1::from_bytes(&[0u8; 10]);
    assert!(result.is_err());
}

#[test]
fn test_info_hash_v1_from_hex_invalid() {
    let result = InfoHashV1::from_hex("not_hex");
    assert!(result.is_err());
}

#[test]
fn test_info_hash_v1_from_hex_wrong_length() {
    let result = InfoHashV1::from_hex("abcd");
    assert!(result.is_err());
}

#[test]
fn test_info_hash_v1_from_info_bytes() {
    let hash = InfoHashV1::from_info_bytes(b"test data");
    assert_eq!(hash.0.len(), 20);
    assert_ne!(hash.0, [0u8; 20]);
}

// ========================
// InfoHashV2 methods
// ========================

#[test]
fn test_info_hash_v2_from_bytes_valid() {
    let bytes = [0xCD; 32];
    let hash = InfoHashV2::from_bytes(&bytes).unwrap();
    assert_eq!(hash.0, bytes);
}

#[test]
fn test_info_hash_v2_from_bytes_wrong_length() {
    let result = InfoHashV2::from_bytes(&[0u8; 20]);
    assert!(result.is_err());
}

#[test]
fn test_info_hash_v2_from_hex_invalid() {
    let result = InfoHashV2::from_hex("not_hex_at_all");
    assert!(result.is_err());
}

#[test]
fn test_info_hash_v2_truncated() {
    let hash =
        InfoHashV2::from_hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
            .unwrap();
    let truncated = hash.truncated();
    assert_eq!(truncated.len(), 20);
    assert_eq!(&truncated, &hash.0[..20]);
}

#[test]
fn test_info_hash_v2_from_info_bytes() {
    let hash = InfoHashV2::from_info_bytes(b"test data");
    assert_eq!(hash.0.len(), 32);
    assert_ne!(hash.0, [0u8; 32]);
}

// ========================
// InfoHash enum
// ========================

#[test]
fn test_info_hash_v1_variant() {
    let hash = InfoHash::V1(InfoHashV1([0xAB; 20]));
    assert!(hash.v1().is_some());
    assert!(hash.v2().is_none());
    assert_eq!(hash.primary_bytes().len(), 20);
}

#[test]
fn test_info_hash_v2_variant() {
    let hash = InfoHash::V2(InfoHashV2([0xCD; 32]));
    assert!(hash.v1().is_none());
    assert!(hash.v2().is_some());
    assert_eq!(hash.primary_bytes().len(), 32);
}

#[test]
fn test_info_hash_hybrid_variant() {
    let hash = InfoHash::Hybrid {
        v1: InfoHashV1([0xAB; 20]),
        v2: InfoHashV2([0xCD; 32]),
    };
    assert!(hash.v1().is_some());
    assert!(hash.v2().is_some());
    assert_eq!(hash.primary_bytes().len(), 20);
}

// ========================
// Metainfo error paths
// ========================

#[test]
fn test_metainfo_from_bytes_invalid_bencode() {
    let result = Metainfo::from_bytes(b"not bencode");
    assert!(result.is_err());
}

#[test]
fn test_metainfo_from_bytes_missing_info() {
    let mut dict = BTreeMap::new();
    dict.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://test.com"),
    );
    let data = encode(&Value::Dict(dict)).unwrap();
    let result = Metainfo::from_bytes(&data);
    assert!(result.is_err());
}

#[test]
fn test_metainfo_is_private_false() {
    let data = create_test_torrent_v1();
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert!(!meta.is_private());
}

#[test]
fn test_metainfo_is_private_true() {
    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(12345));
    info.insert(Bytes::from_static(b"name"), Value::string("private.txt"));
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(262144));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(vec![0u8; 20])),
    );
    info.insert(Bytes::from_static(b"private"), Value::Integer(1));

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://private.tracker.com/announce"),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let data = encode(&Value::Dict(torrent)).unwrap();
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert!(meta.is_private());
}

#[test]
fn test_metainfo_piece_count() {
    let data = create_test_torrent_v1();
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.piece_count(), 1);
}

#[test]
fn test_metainfo_piece_length() {
    let data = create_test_torrent_v1();
    let meta = Metainfo::from_bytes(&data).unwrap();
    let piece_len = meta.piece_length(0);
    assert_eq!(piece_len, 12345);
}

#[test]
fn test_metainfo_raw_info() {
    let data = create_test_torrent_v1();
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert!(!meta.raw_info().is_empty());
}

// ========================
// MagnetLink error paths
// ========================

#[test]
fn test_magnet_parse_invalid_scheme() {
    let result = MagnetLink::parse("http://not-a-magnet.com");
    assert!(result.is_err());
}

#[test]
fn test_magnet_parse_missing_hash() {
    let result = MagnetLink::parse("magnet:?dn=test&tr=http://tracker.com");
    assert!(result.is_err());
}

#[test]
fn test_magnet_with_web_seeds() {
    let uri = "magnet:?xt=urn:btih:da39a3ee5e6b4b0d3255bfef95601890afd80709&ws=http://seed.example.com/file";
    let magnet = MagnetLink::parse(uri).unwrap();
    assert_eq!(magnet.web_seeds.len(), 1);
    assert_eq!(magnet.web_seeds[0], "http://seed.example.com/file");
}

#[test]
fn test_magnet_with_peer_address() {
    let uri = "magnet:?xt=urn:btih:da39a3ee5e6b4b0d3255bfef95601890afd80709&x.pe=192.168.1.1:6881";
    let magnet = MagnetLink::parse(uri).unwrap();
    assert_eq!(magnet.peer_addresses.len(), 1);
}

// ========================
// MerkleTree additional tests
// ========================

#[test]
fn test_merkle_tree_from_leaves() {
    let leaves = vec![[0u8; 32], [1u8; 32], [2u8; 32], [3u8; 32]];
    let tree = MerkleTree::from_leaves(leaves);
    assert!(!tree.root().iter().all(|&b| b == 0));
    assert!(tree.depth() > 0);
}

#[test]
fn test_merkle_tree_depth() {
    let data = vec![0u8; 16384 * 4];
    let tree = MerkleTree::from_piece_data(&data);
    assert!(tree.depth() >= 2);
}
