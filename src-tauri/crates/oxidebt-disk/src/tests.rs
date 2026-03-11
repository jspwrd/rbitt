use super::*;
use crate::storage::{FileEntry, PieceInfo};
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_storage(temp: &TempDir, piece_length: u64, file_size: u64) -> TorrentStorage {
    let base_path = temp.path().to_path_buf();
    let piece_count = file_size.div_ceil(piece_length) as usize;

    let files = vec![FileEntry::new(PathBuf::from("test.dat"), file_size, 0)];

    let pieces: Vec<PieceInfo> = (0..piece_count)
        .map(|i| {
            let offset = i as u64 * piece_length;
            let length = if i == piece_count - 1 {
                let rem = file_size % piece_length;
                if rem == 0 { piece_length } else { rem }
            } else {
                piece_length
            };
            PieceInfo::v1(i as u32, [0u8; 20], offset, length)
        })
        .collect();

    TorrentStorage::new(base_path, files, pieces, file_size, false).expect("test storage creation")
}

#[tokio::test]
async fn test_preallocate() {
    let temp = TempDir::new().unwrap();
    let storage = create_test_storage(&temp, 16384, 65536);

    storage.preallocate().await.unwrap();

    let path = temp.path().join("test.dat");
    let metadata = tokio::fs::metadata(&path).await.unwrap();
    assert_eq!(metadata.len(), 65536);
}

#[tokio::test]
async fn test_write_and_read_piece() {
    let temp = TempDir::new().unwrap();
    let storage = create_test_storage(&temp, 16384, 32768);

    storage.preallocate().await.unwrap();

    let data: Vec<u8> = (0..16384).map(|i| (i % 256) as u8).collect();
    storage.write_piece(0, &data).await.unwrap();

    let read_data = storage.read_piece(0).await.unwrap();
    assert_eq!(read_data.as_ref(), data.as_slice());
}

#[tokio::test]
async fn test_write_and_read_block() {
    let temp = TempDir::new().unwrap();
    let storage = create_test_storage(&temp, 32768, 65536);

    storage.preallocate().await.unwrap();

    let block_data: Vec<u8> = (0..16384).map(|i| (i % 256) as u8).collect();

    storage.write_block(0, 0, &block_data).await.unwrap();
    storage.write_block(0, 16384, &block_data).await.unwrap();

    let read_block = storage.read_block(0, 0, 16384).await.unwrap();
    assert_eq!(read_block.as_ref(), block_data.as_slice());

    let read_block2 = storage.read_block(0, 16384, 16384).await.unwrap();
    assert_eq!(read_block2.as_ref(), block_data.as_slice());
}

#[tokio::test]
async fn test_multifile_storage() {
    let temp = TempDir::new().unwrap();
    let base_path = temp.path().to_path_buf();

    let files = vec![
        FileEntry::new(PathBuf::from("file1.dat"), 10000, 0),
        FileEntry::new(PathBuf::from("file2.dat"), 10000, 10000),
    ];

    let pieces = vec![
        PieceInfo::v1(0, [0u8; 20], 0, 16384),
        PieceInfo::v1(1, [0u8; 20], 16384, 3616),
    ];

    let storage =
        TorrentStorage::new(base_path, files, pieces, 20000, false).expect("test storage creation");
    storage.preallocate().await.unwrap();

    let data: Vec<u8> = (0..16384).map(|i| (i % 256) as u8).collect();
    storage.write_piece(0, &data).await.unwrap();

    let read_data = storage.read_piece(0).await.unwrap();
    assert_eq!(read_data.as_ref(), data.as_slice());
}

#[tokio::test]
async fn test_disk_manager() {
    let temp = TempDir::new().unwrap();
    let storage = create_test_storage(&temp, 16384, 32768);
    storage.preallocate().await.unwrap();

    let manager = DiskManager::new();
    manager.register("test_hash".to_string(), storage);

    let data: Vec<u8> = (0..16384).map(|i| (i % 256) as u8).collect();
    manager.write_piece("test_hash", 0, &data).await.unwrap();

    let read_data = manager.read_piece("test_hash", 0).await.unwrap();
    assert_eq!(read_data.as_ref(), data.as_slice());

    manager.unregister("test_hash");
    assert!(manager.read_piece("test_hash", 0).await.is_err());
}

#[tokio::test]
async fn test_invalid_piece_index() {
    let temp = TempDir::new().unwrap();
    let storage = create_test_storage(&temp, 16384, 32768);
    storage.preallocate().await.unwrap();

    let result = storage.read_piece(999).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_invalid_block_offset() {
    let temp = TempDir::new().unwrap();
    let storage = create_test_storage(&temp, 16384, 32768);
    storage.preallocate().await.unwrap();

    let result = storage.read_block(0, 20000, 1000).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn stress_test_large_file() {
    let temp = TempDir::new().unwrap();
    let file_size = 10 * 1024 * 1024;
    let piece_length = 262144;
    let storage = create_test_storage(&temp, piece_length, file_size);

    storage.preallocate().await.unwrap();

    let piece_count = file_size.div_ceil(piece_length) as usize;

    for i in 0..piece_count {
        let length = if i == piece_count - 1 {
            let rem = file_size % piece_length;
            if rem == 0 { piece_length } else { rem }
        } else {
            piece_length
        };
        let data: Vec<u8> = (0..length)
            .map(|j| ((i + j as usize) % 256) as u8)
            .collect();
        storage.write_piece(i as u32, &data).await.unwrap();
    }

    for i in 0..piece_count {
        let length = if i == piece_count - 1 {
            let rem = file_size % piece_length;
            if rem == 0 { piece_length } else { rem }
        } else {
            piece_length
        };
        let expected: Vec<u8> = (0..length)
            .map(|j| ((i + j as usize) % 256) as u8)
            .collect();
        let read_data = storage.read_piece(i as u32).await.unwrap();
        assert_eq!(read_data.as_ref(), expected.as_slice());
    }
}

#[tokio::test]
async fn stress_test_concurrent_reads() {
    let temp = TempDir::new().unwrap();
    let storage = std::sync::Arc::new(create_test_storage(&temp, 16384, 163840));
    storage.preallocate().await.unwrap();

    for i in 0..10 {
        let data: Vec<u8> = (0..16384).map(|j| ((i + j) % 256) as u8).collect();
        storage.write_piece(i as u32, &data).await.unwrap();
    }

    let mut handles = vec![];
    for i in 0..100 {
        let storage_clone = storage.clone();
        let piece_idx = (i % 10) as u32;
        handles.push(tokio::spawn(async move {
            storage_clone.read_piece(piece_idx).await.unwrap()
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn stress_test_concurrent_writes() {
    let temp = TempDir::new().unwrap();
    let storage = std::sync::Arc::new(create_test_storage(&temp, 16384, 163840));
    storage.preallocate().await.unwrap();

    let mut handles = vec![];
    for i in 0..10 {
        let storage_clone = storage.clone();
        handles.push(tokio::spawn(async move {
            let data: Vec<u8> = (0..16384).map(|j| ((i + j) % 256) as u8).collect();
            storage_clone.write_piece(i as u32, &data).await
        }));
    }

    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    for i in 0..10 {
        let expected: Vec<u8> = (0..16384).map(|j| ((i + j) % 256) as u8).collect();
        let read_data = storage.read_piece(i as u32).await.unwrap();
        assert_eq!(read_data.as_ref(), expected.as_slice());
    }
}

#[tokio::test]
async fn stress_test_block_operations() {
    let temp = TempDir::new().unwrap();
    let storage = create_test_storage(&temp, 65536, 65536);
    storage.preallocate().await.unwrap();

    let block_size = 16384u32;
    let blocks_per_piece = 4u32;

    for i in 0..blocks_per_piece {
        let offset = i * block_size;
        let data: Vec<u8> = (0..block_size as usize)
            .map(|j| ((i as usize + j) % 256) as u8)
            .collect();
        storage.write_block(0, offset, &data).await.unwrap();
    }

    for i in 0..blocks_per_piece {
        let offset = i * block_size;
        let expected: Vec<u8> = (0..block_size as usize)
            .map(|j| ((i as usize + j) % 256) as u8)
            .collect();
        let read_data = storage.read_block(0, offset, block_size).await.unwrap();
        assert_eq!(read_data.as_ref(), expected.as_slice());
    }
}

#[tokio::test]
async fn stress_test_many_small_files() {
    let temp = TempDir::new().unwrap();
    let base_path = temp.path().to_path_buf();

    let file_count = 100;
    let file_size = 1000u64;
    let total_size = file_count * file_size;
    let piece_length = 16384u64;

    let files: Vec<FileEntry> = (0..file_count)
        .map(|i| {
            FileEntry::new(
                PathBuf::from(format!("file{:03}.dat", i)),
                file_size,
                i * file_size,
            )
        })
        .collect();

    let piece_count = total_size.div_ceil(piece_length) as usize;
    let pieces: Vec<PieceInfo> = (0..piece_count)
        .map(|i| {
            let offset = i as u64 * piece_length;
            let length = if i == piece_count - 1 {
                let rem = total_size % piece_length;
                if rem == 0 { piece_length } else { rem }
            } else {
                piece_length
            };
            PieceInfo::v1(i as u32, [0u8; 20], offset, length)
        })
        .collect();

    let storage = TorrentStorage::new(base_path, files, pieces, total_size, false)
        .expect("test storage creation");
    storage.preallocate().await.unwrap();

    for i in 0..piece_count {
        let length = if i == piece_count - 1 {
            let rem = total_size % piece_length;
            if rem == 0 { piece_length } else { rem }
        } else {
            piece_length
        } as usize;
        let data: Vec<u8> = (0..length).map(|j| ((i + j) % 256) as u8).collect();
        storage.write_piece(i as u32, &data).await.unwrap();
    }

    for i in 0..piece_count {
        let length = if i == piece_count - 1 {
            let rem = total_size % piece_length;
            if rem == 0 { piece_length } else { rem }
        } else {
            piece_length
        } as usize;
        let expected: Vec<u8> = (0..length).map(|j| ((i + j) % 256) as u8).collect();
        let read_data = storage.read_piece(i as u32).await.unwrap();
        assert_eq!(read_data.as_ref(), expected.as_slice());
    }
}

#[tokio::test]
async fn stress_test_disk_manager_multiple_torrents() {
    let temp1 = TempDir::new().unwrap();
    let temp2 = TempDir::new().unwrap();
    let temp3 = TempDir::new().unwrap();

    let storage1 = create_test_storage(&temp1, 16384, 65536);
    let storage2 = create_test_storage(&temp2, 32768, 131072);
    let storage3 = create_test_storage(&temp3, 8192, 32768);

    storage1.preallocate().await.unwrap();
    storage2.preallocate().await.unwrap();
    storage3.preallocate().await.unwrap();

    let manager = std::sync::Arc::new(DiskManager::new());
    manager.register("hash1".to_string(), storage1);
    manager.register("hash2".to_string(), storage2);
    manager.register("hash3".to_string(), storage3);

    let mut handles = vec![];
    for i in 0..30 {
        let manager_clone = manager.clone();
        let hash = format!("hash{}", (i % 3) + 1);
        handles.push(tokio::spawn(async move {
            let data: Vec<u8> = (0..16384).map(|j| ((i + j) % 256) as u8).collect();
            manager_clone.write_block(&hash, 0, 0, &data).await
        }));
    }

    for handle in handles {
        let _ = handle.await.unwrap();
    }
}

#[tokio::test]
async fn stress_test_random_access_pattern() {
    let temp = TempDir::new().unwrap();
    let piece_length = 16384u64;
    let file_size = 10 * piece_length;
    let storage = create_test_storage(&temp, piece_length, file_size);
    storage.preallocate().await.unwrap();

    let access_order = [7, 3, 9, 1, 5, 2, 8, 0, 6, 4];

    for &i in &access_order {
        let data: Vec<u8> = (0..piece_length as usize)
            .map(|j| ((i + j) % 256) as u8)
            .collect();
        storage.write_piece(i as u32, &data).await.unwrap();
    }

    for &i in &access_order {
        let expected: Vec<u8> = (0..piece_length as usize)
            .map(|j| ((i + j) % 256) as u8)
            .collect();
        let read_data = storage.read_piece(i as u32).await.unwrap();
        assert_eq!(read_data.as_ref(), expected.as_slice());
    }
}

#[tokio::test]
async fn stress_test_piece_spanning_multiple_files() {
    let temp = TempDir::new().unwrap();
    let base_path = temp.path().to_path_buf();

    let files = vec![
        FileEntry::new(PathBuf::from("a.dat"), 5000, 0),
        FileEntry::new(PathBuf::from("b.dat"), 5000, 5000),
        FileEntry::new(PathBuf::from("c.dat"), 5000, 10000),
    ];

    let total_size = 15000u64;
    let pieces = vec![
        PieceInfo::v1(0, [0u8; 20], 0, 8192),
        PieceInfo::v1(1, [0u8; 20], 8192, 6808),
    ];

    let storage = TorrentStorage::new(base_path, files, pieces, total_size, false)
        .expect("test storage creation");
    storage.preallocate().await.unwrap();

    let data0: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();
    storage.write_piece(0, &data0).await.unwrap();

    let data1: Vec<u8> = (0..6808).map(|i| ((i + 100) % 256) as u8).collect();
    storage.write_piece(1, &data1).await.unwrap();

    let read0 = storage.read_piece(0).await.unwrap();
    assert_eq!(read0.as_ref(), data0.as_slice());

    let read1 = storage.read_piece(1).await.unwrap();
    assert_eq!(read1.as_ref(), data1.as_slice());
}

#[tokio::test]
async fn stress_test_rapid_register_unregister() {
    let manager = DiskManager::new();

    for i in 0..100 {
        let temp = TempDir::new().unwrap();
        let storage = create_test_storage(&temp, 16384, 32768);
        storage.preallocate().await.unwrap();

        let hash = format!("hash_{}", i);
        manager.register(hash.clone(), storage);

        let data: Vec<u8> = (0..16384).map(|j| ((i + j) % 256) as u8).collect();
        manager.write_piece(&hash, 0, &data).await.unwrap();

        manager.unregister(&hash);
        assert!(manager.read_piece(&hash, 0).await.is_err());
    }
}

#[tokio::test]
async fn stress_test_subdirectory_files() {
    let temp = TempDir::new().unwrap();
    let base_path = temp.path().to_path_buf();

    let files = vec![
        FileEntry::new(PathBuf::from("folder1/subfolder/file1.dat"), 5000, 0),
        FileEntry::new(PathBuf::from("folder2/file2.dat"), 5000, 5000),
        FileEntry::new(PathBuf::from("file3.dat"), 5000, 10000),
    ];

    let total_size = 15000u64;
    let pieces = vec![PieceInfo::v1(0, [0u8; 20], 0, 15000)];

    let storage = TorrentStorage::new(base_path.clone(), files, pieces, total_size, false)
        .expect("test storage creation");
    storage.preallocate().await.unwrap();

    assert!(base_path.join("folder1/subfolder").exists());
    assert!(base_path.join("folder2").exists());

    let data: Vec<u8> = (0..15000).map(|i| (i % 256) as u8).collect();
    storage.write_piece(0, &data).await.unwrap();

    let read_data = storage.read_piece(0).await.unwrap();
    assert_eq!(read_data.as_ref(), data.as_slice());
}
