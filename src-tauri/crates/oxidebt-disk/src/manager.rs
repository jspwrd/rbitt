
use crate::error::DiskError;
use crate::storage::{AllocationMode, FileEntry, PieceFileSpan, PieceInfo};
use bytes::Bytes;
use dashmap::DashMap;
use parking_lot::RwLock;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex as TokioMutex, Semaphore};

const MAX_CONCURRENT_OPS: usize = 512;
const FILE_HANDLE_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

struct PerFileHandle {
    file: TokioMutex<File>,
    last_used: parking_lot::Mutex<Instant>,
    is_write: bool,
}

struct FileHandleCache {
    handles: DashMap<usize, Arc<PerFileHandle>>,
    base_path: PathBuf,
    files: Vec<FileEntry>,
}

impl FileHandleCache {
    fn new(base_path: PathBuf, files: Vec<FileEntry>) -> Self {
        Self {
            handles: DashMap::new(),
            base_path,
            files,
        }
    }

    fn file_path(&self, file_index: usize) -> PathBuf {
        self.base_path.join(&self.files[file_index].path)
    }

    async fn ensure_parent_dirs(path: &std::path::Path) -> Result<(), DiskError> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        Ok(())
    }

    async fn get_or_open_read(&self, file_index: usize) -> Result<Arc<PerFileHandle>, DiskError> {
        if let Some(handle) = self.handles.get(&file_index) {
            *handle.last_used.lock() = Instant::now();
            return Ok(handle.clone());
        }

        let path = self.file_path(file_index);
        let file = File::open(&path)
            .await
            .map_err(|_| DiskError::FileNotFound(path.display().to_string()))?;

        let handle = Arc::new(PerFileHandle {
            file: TokioMutex::new(file),
            last_used: parking_lot::Mutex::new(Instant::now()),
            is_write: false,
        });

        self.handles.insert(file_index, handle.clone());
        Ok(handle)
    }

    async fn get_or_open_write(&self, file_index: usize) -> Result<Arc<PerFileHandle>, DiskError> {
        if let Some(handle) = self.handles.get(&file_index) {
            if handle.is_write {
                *handle.last_used.lock() = Instant::now();
                return Ok(handle.clone());
            }
            drop(handle);
            self.handles.remove(&file_index);
        }

        let path = self.file_path(file_index);
        Self::ensure_parent_dirs(&path).await?;

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .await
            .map_err(DiskError::from)?;

        let handle = Arc::new(PerFileHandle {
            file: TokioMutex::new(file),
            last_used: parking_lot::Mutex::new(Instant::now()),
            is_write: true,
        });

        self.handles.insert(file_index, handle.clone());
        Ok(handle)
    }

    async fn flush_all(&self) {
        let keys: Vec<usize> = self.handles.iter().map(|r| *r.key()).collect();
        for key in keys {
            if let Some((_, handle)) = self.handles.remove(&key) {
                if handle.is_write {
                    let file = handle.file.lock().await;
                    let _ = file.sync_data().await;
                }
            }
        }
    }

    async fn evict_idle(&self) {
        let now = Instant::now();
        let to_evict: Vec<usize> = self
            .handles
            .iter()
            .filter(|r| now.duration_since(*r.last_used.lock()) > FILE_HANDLE_IDLE_TIMEOUT)
            .map(|r| *r.key())
            .collect();

        for idx in to_evict {
            if let Some((_, handle)) = self.handles.remove(&idx) {
                if handle.is_write {
                    let file = handle.file.lock().await;
                    let _ = file.sync_data().await;
                }
            }
        }
    }
}

pub struct TorrentStorage {
    base_path: PathBuf,
    files: Vec<FileEntry>,
    pieces: Vec<PieceInfo>,
    total_length: u64,
    allocation_mode: AllocationMode,
    is_v2: bool,
    handle_cache: FileHandleCache,
}

impl TorrentStorage {
    pub fn new(
        base_path: PathBuf,
        files: Vec<FileEntry>,
        pieces: Vec<PieceInfo>,
        total_length: u64,
        is_v2: bool,
    ) -> Self {
        let handle_cache = FileHandleCache::new(base_path.clone(), files.clone());
        Self {
            base_path,
            files,
            pieces,
            total_length,
            allocation_mode: AllocationMode::Sparse,
            is_v2,
            handle_cache,
        }
    }

    pub fn with_allocation_mode(mut self, mode: AllocationMode) -> Self {
        self.allocation_mode = mode;
        self
    }

    pub fn total_length(&self) -> u64 {
        self.total_length
    }

    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }

    pub fn piece_length(&self, index: u32) -> u64 {
        if let Some(piece) = self.pieces.get(index as usize) {
            piece.length
        } else {
            0
        }
    }

    fn piece_file_spans(&self, piece_index: u32) -> Result<Vec<PieceFileSpan>, DiskError> {
        let piece = self
            .pieces
            .get(piece_index as usize)
            .ok_or(DiskError::InvalidPieceIndex(piece_index))?;

        let mut spans = Vec::new();
        let mut remaining = piece.length;
        let mut current_offset = piece.offset;

        for (file_idx, file) in self.files.iter().enumerate() {
            if remaining == 0 {
                break;
            }

            let file_end = file.offset + file.length;

            if current_offset >= file.offset && current_offset < file_end {
                let file_offset = current_offset - file.offset;
                let available = file_end - current_offset;
                let take = remaining.min(available);

                spans.push(PieceFileSpan {
                    file_index: file_idx,
                    file_offset,
                    length: take,
                });

                current_offset += take;
                remaining -= take;
            }
        }

        Ok(spans)
    }

    fn block_file_spans(
        &self,
        piece_index: u32,
        offset: u32,
        length: u32,
    ) -> Result<Vec<PieceFileSpan>, DiskError> {
        let piece = self
            .pieces
            .get(piece_index as usize)
            .ok_or(DiskError::InvalidPieceIndex(piece_index))?;

        if offset as u64 + length as u64 > piece.length {
            return Err(DiskError::InvalidBlockOffset {
                piece: piece_index,
                offset,
            });
        }

        let block_start = piece.offset + offset as u64;
        let mut spans = Vec::new();
        let mut remaining = length as u64;
        let mut current_offset = block_start;

        for (file_idx, file) in self.files.iter().enumerate() {
            if remaining == 0 {
                break;
            }

            let file_end = file.offset + file.length;

            if current_offset >= file.offset && current_offset < file_end {
                let file_offset = current_offset - file.offset;
                let available = file_end - current_offset;
                let take = remaining.min(available);

                spans.push(PieceFileSpan {
                    file_index: file_idx,
                    file_offset,
                    length: take,
                });

                current_offset += take;
                remaining -= take;
            }
        }

        Ok(spans)
    }

    fn file_path(&self, file: &FileEntry) -> PathBuf {
        self.base_path.join(&file.path)
    }

    async fn ensure_parent_dirs(&self, path: &std::path::Path) -> Result<(), DiskError> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        Ok(())
    }

    pub async fn preallocate(&self) -> Result<(), DiskError> {
        for file in &self.files {
            let path = self.file_path(file);
            self.ensure_parent_dirs(&path).await?;

            let f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&path)
                .await?;

            match self.allocation_mode {
                AllocationMode::Sparse => {
                    f.set_len(file.length).await?;
                }
                AllocationMode::Full => {
                    f.set_len(file.length).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn read_piece(&self, piece_index: u32) -> Result<Bytes, DiskError> {
        let piece = self
            .pieces
            .get(piece_index as usize)
            .ok_or(DiskError::InvalidPieceIndex(piece_index))?;

        let spans = self.piece_file_spans(piece_index)?;
        let mut data = Vec::with_capacity(piece.length as usize);

        for span in spans {
            let handle = self.handle_cache.get_or_open_read(span.file_index).await?;
            let mut file = handle.file.lock().await;
            file.seek(SeekFrom::Start(span.file_offset)).await?;

            let mut buf = vec![0u8; span.length as usize];
            file.read_exact(&mut buf).await?;
            data.extend_from_slice(&buf);
        }

        Ok(Bytes::from(data))
    }

    pub async fn read_block(
        &self,
        piece_index: u32,
        offset: u32,
        length: u32,
    ) -> Result<Bytes, DiskError> {
        let spans = self.block_file_spans(piece_index, offset, length)?;
        let mut data = Vec::with_capacity(length as usize);

        for span in spans {
            let handle = self.handle_cache.get_or_open_read(span.file_index).await?;
            let mut file = handle.file.lock().await;
            file.seek(SeekFrom::Start(span.file_offset)).await?;

            let mut buf = vec![0u8; span.length as usize];
            file.read_exact(&mut buf).await?;
            data.extend_from_slice(&buf);
        }

        Ok(Bytes::from(data))
    }

    pub async fn write_piece(&self, piece_index: u32, data: &[u8]) -> Result<(), DiskError> {
        let piece = self
            .pieces
            .get(piece_index as usize)
            .ok_or(DiskError::InvalidPieceIndex(piece_index))?;

        if data.len() != piece.length as usize {
            return Err(DiskError::InvalidPieceIndex(piece_index));
        }

        let spans = self.piece_file_spans(piece_index)?;
        let mut data_offset = 0usize;

        for span in spans {
            let handle = self.handle_cache.get_or_open_write(span.file_index).await?;
            let mut file = handle.file.lock().await;
            file.seek(SeekFrom::Start(span.file_offset)).await?;

            let chunk = &data[data_offset..data_offset + span.length as usize];
            file.write_all(chunk).await?;

            data_offset += span.length as usize;
        }

        Ok(())
    }

    pub async fn write_block(
        &self,
        piece_index: u32,
        offset: u32,
        data: &[u8],
    ) -> Result<(), DiskError> {
        let spans = self.block_file_spans(piece_index, offset, data.len() as u32)?;
        let mut data_offset = 0usize;

        for span in spans {
            let handle = self.handle_cache.get_or_open_write(span.file_index).await?;
            let mut file = handle.file.lock().await;
            file.seek(SeekFrom::Start(span.file_offset)).await?;

            let chunk = &data[data_offset..data_offset + span.length as usize];
            file.write_all(chunk).await?;

            data_offset += span.length as usize;
        }

        Ok(())
    }

    /// Verify a piece by reading and hashing - uses cached handles
    pub async fn verify_piece(&self, piece_index: u32) -> Result<bool, DiskError> {
        let piece = self
            .pieces
            .get(piece_index as usize)
            .ok_or(DiskError::InvalidPieceIndex(piece_index))?;

        let data = self.read_piece(piece_index).await?;
        let expected_hash = piece.hash.clone();
        let is_v2 = self.is_v2;

        // Offload hashing to blocking thread pool
        let hash = tokio::task::spawn_blocking(move || {
            if is_v2 {
                let mut hasher = Sha256::new();
                hasher.update(&data);
                hasher.finalize().to_vec()
            } else {
                let mut hasher = Sha1::new();
                hasher.update(&data);
                hasher.finalize().to_vec()
            }
        })
        .await
        .map_err(|e| DiskError::Io(std::io::Error::other(e)))?;

        Ok(hash == expected_hash)
    }

    /// Verify all pieces with improved batching
    pub async fn verify_all(&self) -> Result<Vec<bool>, DiskError> {
        use tokio::time::{timeout, Duration};

        let piece_count = self.pieces.len();
        if piece_count == 0 {
            return Ok(vec![]);
        }

        tracing::debug!("Starting verification of {} pieces", piece_count);

        const BATCH_SIZE: usize = 32;
        const BATCH_TIMEOUT: Duration = Duration::from_secs(120);

        let mut results = vec![false; piece_count];
        let mut verified_count = 0usize;

        for batch_start in (0..piece_count).step_by(BATCH_SIZE) {
            let batch_end = (batch_start + BATCH_SIZE).min(piece_count);
            let mut futures = Vec::with_capacity(batch_end - batch_start);

            for i in batch_start..batch_end {
                futures.push(self.verify_piece(i as u32));
            }

            let batch_results =
                match timeout(BATCH_TIMEOUT, futures::future::join_all(futures)).await {
                    Ok(results) => results,
                    Err(_) => {
                        tracing::warn!(
                            "Verification batch {}-{} timed out, marking as invalid",
                            batch_start,
                            batch_end
                        );
                        continue;
                    }
                };

            for (i, result) in batch_results.into_iter().enumerate() {
                let piece_idx = batch_start + i;
                results[piece_idx] = match result {
                    Ok(valid) => {
                        if valid {
                            verified_count += 1;
                        }
                        valid
                    }
                    Err(DiskError::FileNotFound(_)) => false,
                    Err(e) => {
                        tracing::trace!("Piece {} verification error: {}", piece_idx, e);
                        false
                    }
                };
            }

            if piece_count > 100 && batch_end.is_multiple_of(100) {
                tracing::debug!(
                    "Verified {}/{} pieces ({} valid so far)",
                    batch_end,
                    piece_count,
                    verified_count
                );
            }
        }

        tracing::debug!(
            "Verification complete: {}/{} pieces valid",
            verified_count,
            piece_count
        );

        Ok(results)
    }

    pub async fn flush(&self) {
        self.handle_cache.flush_all().await;
    }

    pub async fn evict_idle_handles(&self) {
        self.handle_cache.evict_idle().await;
    }
}

pub struct DiskManager {
    torrents: RwLock<HashMap<String, Arc<TorrentStorage>>>,
    semaphore: Arc<Semaphore>,
}

impl DiskManager {
    pub fn new() -> Self {
        Self {
            torrents: RwLock::new(HashMap::new()),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_OPS)),
        }
    }

    pub fn register(&self, info_hash: String, storage: TorrentStorage) {
        self.torrents.write().insert(info_hash, Arc::new(storage));
    }

    pub fn unregister(&self, info_hash: &str) {
        if let Some(storage) = self.torrents.write().remove(info_hash) {
            // Spawn a task to flush the storage before dropping
            tokio::spawn(async move {
                storage.flush().await;
            });
        }
    }

    fn get_storage(&self, info_hash: &str) -> Result<Arc<TorrentStorage>, DiskError> {
        self.torrents
            .read()
            .get(info_hash)
            .cloned()
            .ok_or_else(|| DiskError::TorrentNotFound(info_hash.to_string()))
    }

    pub async fn read_piece(&self, info_hash: &str, piece_index: u32) -> Result<Bytes, DiskError> {
        let storage = self.get_storage(info_hash)?;
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            DiskError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "semaphore closed",
            ))
        })?;
        storage.read_piece(piece_index).await
    }

    pub async fn read_block(
        &self,
        info_hash: &str,
        piece_index: u32,
        offset: u32,
        length: u32,
    ) -> Result<Bytes, DiskError> {
        let storage = self.get_storage(info_hash)?;
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            DiskError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "semaphore closed",
            ))
        })?;
        storage.read_block(piece_index, offset, length).await
    }

    pub async fn write_piece(
        &self,
        info_hash: &str,
        piece_index: u32,
        data: &[u8],
    ) -> Result<(), DiskError> {
        let storage = self.get_storage(info_hash)?;
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            DiskError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "semaphore closed",
            ))
        })?;
        storage.write_piece(piece_index, data).await
    }

    pub async fn write_block(
        &self,
        info_hash: &str,
        piece_index: u32,
        offset: u32,
        data: &[u8],
    ) -> Result<(), DiskError> {
        let storage = self.get_storage(info_hash)?;
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            DiskError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "semaphore closed",
            ))
        })?;
        storage.write_block(piece_index, offset, data).await
    }

    pub async fn verify_piece(&self, info_hash: &str, piece_index: u32) -> Result<bool, DiskError> {
        let storage = self.get_storage(info_hash)?;
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            DiskError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "semaphore closed",
            ))
        })?;
        storage.verify_piece(piece_index).await
    }

    pub async fn write_and_verify(
        &self,
        info_hash: &str,
        piece_index: u32,
        data: &[u8],
    ) -> Result<bool, DiskError> {
        self.write_piece(info_hash, piece_index, data).await?;
        self.verify_piece(info_hash, piece_index).await
    }

    pub async fn verify_all(&self, info_hash: &str) -> Result<Vec<bool>, DiskError> {
        let storage = self.get_storage(info_hash)?;
        storage.verify_all().await
    }

    pub fn piece_count(&self, info_hash: &str) -> Result<usize, DiskError> {
        let storage = self.get_storage(info_hash)?;
        Ok(storage.pieces.len())
    }

    /// Flush all cached file handles for a torrent
    pub async fn flush(&self, info_hash: &str) -> Result<(), DiskError> {
        let storage = self.get_storage(info_hash)?;
        storage.flush().await;
        Ok(())
    }

    /// Evict idle file handles across all torrents
    pub async fn evict_idle_handles(&self) {
        let storages: Vec<Arc<TorrentStorage>> = self.torrents.read().values().cloned().collect();
        for storage in storages {
            storage.evict_idle_handles().await;
        }
    }
}

impl Default for DiskManager {
    fn default() -> Self {
        Self::new()
    }
}
