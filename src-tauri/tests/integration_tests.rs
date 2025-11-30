
use bytes::Bytes;
use oxidebt_bencode::{decode, encode, Value};
use oxidebt_disk::{DiskManager, FileEntry, PieceInfo, TorrentStorage};
use oxidebt_peer::{Bitfield, Block, Message, PeerId, PieceManager};
use oxidebt_torrent::{InfoHash, Metainfo};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn integration_test_parse_torrent_and_create_storage() {
    let piece_data = vec![0u8; 20000];
    let mut hasher = Sha1::new();
    hasher.update(&piece_data);
    let piece_hash: [u8; 20] = hasher.finalize().into();

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(20000));
    info.insert(Bytes::from_static(b"name"), Value::string("test_file.dat"));
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(20000));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(piece_hash.to_vec())),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://tracker.example.com/announce"),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let encoded = encode(&Value::Dict(torrent)).unwrap();

    let metainfo = Metainfo::from_bytes(&encoded).unwrap();
    assert_eq!(metainfo.info.name, "test_file.dat");
    assert_eq!(metainfo.info.total_length, 20000);
    assert_eq!(metainfo.piece_count(), 1);

    let temp = TempDir::new().unwrap();
    let base_path = temp.path().to_path_buf();

    let files = vec![FileEntry::new(
        PathBuf::from(&metainfo.info.name),
        metainfo.info.total_length,
        0,
    )];

    let pieces = vec![PieceInfo::v1(0, piece_hash, 0, metainfo.info.total_length)];

    let storage = TorrentStorage::new(base_path, files, pieces, metainfo.info.total_length, false);

    storage.preallocate().await.unwrap();

    storage.write_piece(0, &piece_data).await.unwrap();

    let read_data = storage.read_piece(0).await.unwrap();
    assert_eq!(read_data.as_ref(), piece_data.as_slice());

    let mut verify_hasher = Sha1::new();
    verify_hasher.update(&read_data);
    let computed_hash: [u8; 20] = verify_hasher.finalize().into();
    assert_eq!(computed_hash, piece_hash);
}

#[tokio::test]
async fn integration_test_multifile_torrent_storage() {
    let file1_data = vec![0xAA; 5000];
    let file2_data = vec![0xBB; 3000];
    let total_size = 8000u64;

    let mut piece_data = file1_data.clone();
    piece_data.extend(&file2_data);

    let mut hasher = Sha1::new();
    hasher.update(&piece_data);
    let piece_hash: [u8; 20] = hasher.finalize().into();

    let temp = TempDir::new().unwrap();
    let base_path = temp.path().to_path_buf();

    let files = vec![
        FileEntry::new(PathBuf::from("subdir/file1.dat"), 5000, 0),
        FileEntry::new(PathBuf::from("file2.dat"), 3000, 5000),
    ];

    let pieces = vec![PieceInfo::v1(0, piece_hash, 0, total_size)];

    let storage = TorrentStorage::new(base_path.clone(), files, pieces, total_size, false);
    storage.preallocate().await.unwrap();

    storage.write_piece(0, &piece_data).await.unwrap();

    let read_data = storage.read_piece(0).await.unwrap();
    assert_eq!(read_data.as_ref(), piece_data.as_slice());

    let file1_read = storage.read_block(0, 0, 5000).await.unwrap();
    assert_eq!(file1_read.as_ref(), file1_data.as_slice());

    let file2_read = storage.read_block(0, 5000, 3000).await.unwrap();
    assert_eq!(file2_read.as_ref(), file2_data.as_slice());
}

#[tokio::test]
async fn integration_test_piece_manager_with_disk() {
    let temp = TempDir::new().unwrap();
    let base_path = temp.path().to_path_buf();

    let piece_length = 32768u64;
    let piece_count = 4;
    let total_size = piece_length * piece_count as u64;

    let mut piece_hashes = Vec::new();
    for i in 0..piece_count {
        let data: Vec<u8> = (0..piece_length as usize)
            .map(|j| ((i + j) % 256) as u8)
            .collect();
        let mut hasher = Sha1::new();
        hasher.update(&data);
        let hash: [u8; 20] = hasher.finalize().into();
        piece_hashes.push(hash);
    }

    let files = vec![FileEntry::new(PathBuf::from("test.dat"), total_size, 0)];
    let pieces: Vec<PieceInfo> = piece_hashes
        .iter()
        .enumerate()
        .map(|(i, hash)| PieceInfo::v1(i as u32, *hash, i as u64 * piece_length, piece_length))
        .collect();

    let storage = TorrentStorage::new(base_path, files, pieces, total_size, false);
    storage.preallocate().await.unwrap();

    let piece_manager = PieceManager::new(piece_count, piece_length, total_size);

    piece_manager.mark_verification_complete();

    let peer_bitfield = Bitfield::full(piece_count);
    piece_manager.update_availability(&peer_bitfield);

    for _ in 0..piece_count {
        let piece_idx = piece_manager.pick_piece(&peer_bitfield).unwrap();
        piece_manager.start_piece(piece_idx);

        let requests = piece_manager.get_block_requests(piece_idx);

        let mut piece_data = Vec::new();
        for req in requests {
            let block_data: Vec<u8> = (req.offset as usize..(req.offset + req.length) as usize)
                .map(|j| ((piece_idx as usize + j) % 256) as u8)
                .collect();
            piece_data.extend(&block_data);

            let block = Block {
                piece_index: req.piece_index,
                offset: req.offset,
                data: Bytes::from(block_data),
            };
            let complete = piece_manager.receive_block(block).unwrap();

            if complete {
                storage.write_piece(piece_idx, &piece_data).await.unwrap();

                let read_data = storage.read_piece(piece_idx).await.unwrap();
                let mut hasher = Sha1::new();
                hasher.update(&read_data);
                let computed_hash: [u8; 20] = hasher.finalize().into();
                assert_eq!(computed_hash, piece_hashes[piece_idx as usize]);

                piece_manager.mark_piece_complete(piece_idx);
            }
        }
    }

    assert!(piece_manager.is_complete());
}

#[test]
fn integration_test_message_bitfield_roundtrip() {
    let piece_count = 100;
    let mut bitfield = Bitfield::new(piece_count);

    for i in (0..piece_count).step_by(3) {
        bitfield.set_piece(i);
    }

    let msg = Message::Bitfield(bitfield.to_bytes());
    let encoded = msg.encode();

    let decoded = Message::parse(&encoded).unwrap();

    match decoded {
        Message::Bitfield(data) => {
            let reconstructed = Bitfield::from_bytes(&data, piece_count).unwrap();
            assert_eq!(bitfield.count(), reconstructed.count());
            for i in 0..piece_count {
                assert_eq!(bitfield.has_piece(i), reconstructed.has_piece(i));
            }
        }
        _ => panic!("Expected Bitfield message"),
    }
}

#[test]
fn integration_test_message_piece_data_integrity() {
    let piece_data: Vec<u8> = (0..16384).map(|i| (i % 256) as u8).collect();
    let msg = Message::Piece {
        index: 42,
        begin: 0,
        data: Bytes::from(piece_data.clone()),
    };

    let encoded = msg.encode();
    let decoded = Message::parse(&encoded).unwrap();

    match decoded {
        Message::Piece { index, begin, data } => {
            assert_eq!(index, 42);
            assert_eq!(begin, 0);
            assert_eq!(data.as_ref(), piece_data.as_slice());
        }
        _ => panic!("Expected Piece message"),
    }
}

#[tokio::test]
async fn integration_test_disk_manager_multiple_torrents() {
    let manager = Arc::new(DiskManager::new());

    let mut handles = Vec::new();

    for torrent_id in 0..5 {
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            let temp = TempDir::new().unwrap();
            let base_path = temp.path().to_path_buf();

            let piece_length = 16384u64;
            let piece_count = 10;
            let total_size = piece_length * piece_count as u64;

            let files = vec![FileEntry::new(
                PathBuf::from(format!("torrent_{}.dat", torrent_id)),
                total_size,
                0,
            )];

            let pieces: Vec<PieceInfo> = (0..piece_count)
                .map(|i| PieceInfo::v1(i as u32, [0u8; 20], i as u64 * piece_length, piece_length))
                .collect();

            let storage = TorrentStorage::new(base_path, files, pieces, total_size, false);
            storage.preallocate().await.unwrap();

            let hash = format!("hash_{}", torrent_id);
            manager_clone.register(hash.clone(), storage);

            for i in 0..piece_count {
                let data: Vec<u8> = (0..piece_length as usize)
                    .map(|j| ((torrent_id + i + j) % 256) as u8)
                    .collect();
                manager_clone
                    .write_piece(&hash, i as u32, &data)
                    .await
                    .unwrap();
            }

            for i in 0..piece_count {
                let expected: Vec<u8> = (0..piece_length as usize)
                    .map(|j| ((torrent_id + i + j) % 256) as u8)
                    .collect();
                let read_data = manager_clone.read_piece(&hash, i as u32).await.unwrap();
                assert_eq!(read_data.as_ref(), expected.as_slice());
            }

            manager_clone.unregister(&hash);
            (torrent_id, temp)
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[test]
fn integration_test_bencode_metainfo_roundtrip() {
    let piece_hashes = (0..10)
        .flat_map(|i| {
            let mut hash = [0u8; 20];
            for (j, byte) in hash.iter_mut().enumerate() {
                *byte = ((i + j) % 256) as u8;
            }
            hash.into_iter()
        })
        .collect::<Vec<u8>>();

    let files = Value::List(vec![
        {
            let mut f = BTreeMap::new();
            f.insert(Bytes::from_static(b"length"), Value::Integer(10000));
            f.insert(
                Bytes::from_static(b"path"),
                Value::List(vec![Value::string("folder"), Value::string("file1.txt")]),
            );
            Value::Dict(f)
        },
        {
            let mut f = BTreeMap::new();
            f.insert(Bytes::from_static(b"length"), Value::Integer(5000));
            f.insert(
                Bytes::from_static(b"path"),
                Value::List(vec![Value::string("file2.txt")]),
            );
            Value::Dict(f)
        },
    ]);

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"files"), files);
    info.insert(Bytes::from_static(b"name"), Value::string("my_torrent"));
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(16384));
    info.insert(
        Bytes::from_static(b"pieces"),
        Value::Bytes(Bytes::from(piece_hashes)),
    );

    let mut torrent = BTreeMap::new();
    torrent.insert(
        Bytes::from_static(b"announce"),
        Value::string("http://tracker.example.com/announce"),
    );
    torrent.insert(Bytes::from_static(b"info"), Value::Dict(info));

    let encoded = encode(&Value::Dict(torrent)).unwrap();

    let decoded = decode(&encoded).unwrap();
    assert!(decoded.as_dict().is_some());

    let metainfo = Metainfo::from_bytes(&encoded).unwrap();
    assert_eq!(metainfo.info.name, "my_torrent");
    assert_eq!(metainfo.info.files.len(), 2);
    assert_eq!(metainfo.info.total_length, 15000);
    assert_eq!(metainfo.piece_count(), 10);
}

#[tokio::test]
async fn integration_test_simulated_download_flow() {
    let temp = TempDir::new().unwrap();
    let base_path = temp.path().to_path_buf();

    let piece_length = 32768u64;
    let piece_count = 8;
    let total_size = piece_length * piece_count as u64;

    let mut expected_pieces: Vec<Vec<u8>> = Vec::new();
    let mut piece_hashes: Vec<[u8; 20]> = Vec::new();

    for i in 0..piece_count {
        let data: Vec<u8> = (0..piece_length as usize)
            .map(|j| ((i * 100 + j) % 256) as u8)
            .collect();
        let mut hasher = Sha1::new();
        hasher.update(&data);
        let hash: [u8; 20] = hasher.finalize().into();
        expected_pieces.push(data);
        piece_hashes.push(hash);
    }

    let files = vec![FileEntry::new(PathBuf::from("download.dat"), total_size, 0)];
    let pieces: Vec<PieceInfo> = piece_hashes
        .iter()
        .enumerate()
        .map(|(i, hash)| PieceInfo::v1(i as u32, *hash, i as u64 * piece_length, piece_length))
        .collect();

    let storage = Arc::new(TorrentStorage::new(
        base_path, files, pieces, total_size, false,
    ));
    storage.preallocate().await.unwrap();

    let piece_manager = Arc::new(PieceManager::new(piece_count, piece_length, total_size));

    piece_manager.mark_verification_complete();

    let peer_bitfield = Bitfield::full(piece_count);
    piece_manager.update_availability(&peer_bitfield);

    let mut handles = Vec::new();
    let mut started_pieces = std::collections::HashSet::new();

    while !piece_manager.is_complete() {
        let piece_idx = match piece_manager.pick_piece(&peer_bitfield) {
            Some(idx) => idx,
            None => break,
        };

        if started_pieces.contains(&piece_idx) {
            break;
        }
        started_pieces.insert(piece_idx);

        piece_manager.start_piece(piece_idx);

        let storage_clone = storage.clone();
        let pm_clone = piece_manager.clone();
        let expected_data = expected_pieces[piece_idx as usize].clone();
        let expected_hash = piece_hashes[piece_idx as usize];

        let handle = tokio::spawn(async move {
            let requests = pm_clone.get_block_requests(piece_idx);

            let mut piece_data = vec![0u8; piece_length as usize];
            for req in requests {
                let block_start = req.offset as usize;
                let block_end = block_start + req.length as usize;
                let block_data = expected_data[block_start..block_end].to_vec();

                piece_data[block_start..block_end].copy_from_slice(&block_data);

                let block = Block {
                    piece_index: req.piece_index,
                    offset: req.offset,
                    data: Bytes::from(block_data),
                };
                pm_clone.receive_block(block).unwrap();
            }

            storage_clone
                .write_piece(piece_idx, &piece_data)
                .await
                .unwrap();

            let read_data = storage_clone.read_piece(piece_idx).await.unwrap();
            let mut hasher = Sha1::new();
            hasher.update(&read_data);
            let computed_hash: [u8; 20] = hasher.finalize().into();
            assert_eq!(computed_hash, expected_hash);

            pm_clone.mark_piece_complete(piece_idx);
            piece_idx
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    assert!(piece_manager.is_complete());

    for (i, expected) in expected_pieces.iter().enumerate() {
        let read_data = storage.read_piece(i as u32).await.unwrap();
        assert_eq!(read_data.as_ref(), expected.as_slice());
    }
}

#[test]
fn integration_test_peer_id_info_hash_encoding() {
    let peer_id = PeerId::generate();
    let peer_id_bytes = peer_id.as_bytes();

    let mut info = BTreeMap::new();
    info.insert(Bytes::from_static(b"length"), Value::Integer(1000));
    info.insert(Bytes::from_static(b"name"), Value::string("test"));
    info.insert(Bytes::from_static(b"piece length"), Value::Integer(1000));
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

    let encoded = encode(&Value::Dict(torrent)).unwrap();
    let metainfo = Metainfo::from_bytes(&encoded).unwrap();

    let info_hash = match &metainfo.info_hash {
        InfoHash::V1(h) => h.as_bytes(),
        _ => panic!("Expected V1 hash"),
    };

    assert_eq!(peer_id_bytes.len(), 20);
    assert_eq!(info_hash.len(), 20);

    assert!(peer_id_bytes.starts_with(b"-OX0001-"));
}

#[test]
fn integration_test_block_request_coverage() {
    let piece_length = 262144u64;
    let total_size = piece_length * 4;
    let block_size = 16384u32;

    let pm = PieceManager::new(4, piece_length, total_size);

    for piece_idx in 0..4 {
        pm.start_piece(piece_idx);
        let requests = pm.get_block_requests(piece_idx);

        let mut covered = vec![false; (piece_length / block_size as u64) as usize];
        let mut total_requested = 0u64;

        for req in &requests {
            assert_eq!(req.piece_index, piece_idx);
            assert_eq!(req.length, block_size);
            let block_idx = (req.offset / block_size) as usize;
            covered[block_idx] = true;
            total_requested += req.length as u64;
        }

        assert!(covered.iter().all(|&c| c));
        assert_eq!(total_requested, piece_length);
    }
}

#[test]
fn integration_test_piece_picker_endgame() {
    let piece_count = 100;
    let piece_length = 16384u64;
    let total_size = piece_length * piece_count as u64;

    let pm = PieceManager::new(piece_count, piece_length, total_size);

    let peer_bf = Bitfield::full(piece_count);
    pm.update_availability(&peer_bf);

    for i in 0..95 {
        pm.start_piece(i);
        pm.mark_piece_complete(i);
    }

    for _ in 0..5 {
        if let Some(piece) = pm.pick_piece(&peer_bf) {
            pm.start_piece(piece);
        }
    }
}
