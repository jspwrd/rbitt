use crate::coalescer::{FlushRequest, WriteCoalescer};
use crate::error::DiskError;
use crate::io_queue::{IoQueue, WriteOp};
use crate::manager::DiskManager;
use bytes::Bytes;
use oxidebt_cache::{BlockCache, BufferPool, MemoryBudget, PieceCache};
use std::sync::Arc;
use tokio::sync::mpsc;

pub enum WriteResult {
    Buffered,
    PieceComplete { valid: bool },
}

pub struct CachingDiskManager {
    storage: Arc<DiskManager>,
    block_cache: Arc<BlockCache>,
    piece_cache: Arc<PieceCache>,
    #[allow(dead_code)]
    buffer_pool: Arc<BufferPool>,
    #[allow(dead_code)]
    memory_budget: Arc<MemoryBudget>,
    #[allow(dead_code)]
    io_queue: Arc<IoQueue>,
    #[allow(dead_code)]
    coalescer: Arc<WriteCoalescer>,
    flush_tx: mpsc::UnboundedSender<FlushRequest>,
}

impl CachingDiskManager {
    pub fn new(memory_limit: usize) -> Self {
        let memory_budget = MemoryBudget::new(memory_limit);
        let buffer_pool = BufferPool::new();

        let block_cache_limit = memory_budget.block_cache_limit();
        let piece_cache_capacity = memory_budget.piece_cache_limit() / (2 * 1024 * 1024);

        let block_cache = BlockCache::new(block_cache_limit);
        let piece_cache = PieceCache::new(piece_cache_capacity);

        let (io_queue, _worker_rxs) = IoQueue::new(4);
        let (flush_tx, _flush_rx) = mpsc::unbounded_channel();
        let coalescer = WriteCoalescer::new(flush_tx.clone());

        Self {
            storage: Arc::new(DiskManager::new()),
            block_cache,
            piece_cache,
            buffer_pool,
            memory_budget,
            io_queue: Arc::new(io_queue),
            coalescer: Arc::new(coalescer),
            flush_tx,
        }
    }

    pub fn storage(&self) -> &DiskManager {
        &self.storage
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn write_block(
        &self,
        info_hash: &str,
        piece_index: u32,
        offset: u32,
        data: Bytes,
        piece_length: u32,
        expected_hash: &[u8],
        is_v2: bool,
    ) -> Result<WriteResult, DiskError> {
        let hash_version = if is_v2 { 2 } else { 1 };
        let is_complete = self.block_cache.add_block(
            info_hash,
            piece_index,
            offset,
            data,
            piece_length,
            hash_version,
        );

        if is_complete {
            let valid = self
                .block_cache
                .finalize_and_verify(info_hash, piece_index, expected_hash);

            if valid {
                if let Some(piece_data) = self.block_cache.remove_piece(info_hash, piece_index) {
                    self.piece_cache
                        .insert(info_hash, piece_index, piece_data.clone(), true);

                    self.storage
                        .write_piece(info_hash, piece_index, &piece_data)
                        .await?;
                }
            } else {
                self.block_cache.remove_piece(info_hash, piece_index);
            }

            Ok(WriteResult::PieceComplete { valid })
        } else {
            Ok(WriteResult::Buffered)
        }
    }

    pub async fn read_block(
        &self,
        info_hash: &str,
        piece_index: u32,
        offset: u32,
        length: u32,
    ) -> Result<Bytes, DiskError> {
        if let Some(piece_data) = self.piece_cache.get(info_hash, piece_index) {
            let start = offset as usize;
            let end = start + length as usize;
            if end <= piece_data.len() {
                return Ok(piece_data.slice(start..end));
            }
        }

        if let Some(piece_data) = self.block_cache.get_assembled_piece(info_hash, piece_index) {
            let start = offset as usize;
            let end = start + length as usize;
            if end <= piece_data.len() {
                return Ok(piece_data.slice(start..end));
            }
        }

        let data = self
            .storage
            .read_block(info_hash, piece_index, offset, length)
            .await?;

        Ok(data)
    }

    pub async fn read_piece(&self, info_hash: &str, piece_index: u32) -> Result<Bytes, DiskError> {
        if let Some(piece_data) = self.piece_cache.get(info_hash, piece_index) {
            return Ok(piece_data);
        }

        if let Some(piece_data) = self.block_cache.get_assembled_piece(info_hash, piece_index) {
            return Ok(piece_data);
        }

        let data = self.storage.read_piece(info_hash, piece_index).await?;
        self.piece_cache
            .insert(info_hash, piece_index, data.clone(), true);

        Ok(data)
    }

    #[allow(dead_code)]
    pub fn submit_write_op(&self, op: WriteOp) -> bool {
        self.io_queue.submit(op)
    }

    #[allow(dead_code)]
    pub fn submit_flush(&self, request: FlushRequest) -> bool {
        self.flush_tx.send(request).is_ok()
    }

    pub fn block_cache_memory_used(&self) -> usize {
        self.block_cache.memory_used()
    }

    pub fn piece_cache_memory_used(&self) -> usize {
        self.piece_cache.memory_used()
    }

    pub fn is_under_memory_pressure(&self) -> bool {
        self.block_cache.is_under_pressure()
    }

    pub fn clear_caches(&self) {
        self.block_cache.clear();
        self.piece_cache.clear();
    }
}
