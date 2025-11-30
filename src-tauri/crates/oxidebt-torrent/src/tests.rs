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
