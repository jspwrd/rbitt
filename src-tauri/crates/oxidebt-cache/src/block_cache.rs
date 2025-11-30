use bytes::Bytes;
use dashmap::DashMap;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const BLOCK_SIZE: u32 = 16384;

#[derive(Clone)]
pub enum HashState {
    V1(Sha1),
    V2(Sha256),
}

impl HashState {
    pub fn new_v1() -> Self {
        HashState::V1(Sha1::new())
    }

    pub fn new_v2() -> Self {
        HashState::V2(Sha256::new())
    }

    pub fn update(&mut self, data: &[u8]) {
        match self {
            HashState::V1(h) => h.update(data),
            HashState::V2(h) => h.update(data),
        }
    }

    pub fn finalize(self) -> Vec<u8> {
        match self {
            HashState::V1(h) => h.finalize().to_vec(),
            HashState::V2(h) => h.finalize().to_vec(),
        }
    }
}

struct PieceBlocks {
    blocks: BTreeMap<u32, Bytes>,
    piece_length: u32,
    bytes_hashed: u32,
    #[allow(dead_code)]
    started_at: Instant,
}

impl PieceBlocks {
    fn new(piece_length: u32) -> Self {
        Self {
            blocks: BTreeMap::new(),
            piece_length,
            bytes_hashed: 0,
            started_at: Instant::now(),
        }
    }

    fn is_complete(&self) -> bool {
        let block_count = self.piece_length.div_ceil(BLOCK_SIZE);
        self.blocks.len() as u32 == block_count
    }

    fn total_bytes(&self) -> usize {
        self.blocks.values().map(|b| b.len()).sum()
    }

    fn assemble(&self) -> Bytes {
        let mut data = Vec::with_capacity(self.piece_length as usize);
        for (_, block) in &self.blocks {
            data.extend_from_slice(block);
        }
        Bytes::from(data)
    }

    fn coalesce(&self) -> Vec<CoalescedRegion> {
        let mut regions = Vec::new();
        let mut current: Option<(u32, Vec<u8>)> = None;

        for (&offset, data) in &self.blocks {
            match &mut current {
                Some((start, buf)) if *start + buf.len() as u32 == offset => {
                    buf.extend_from_slice(data);
                }
                _ => {
                    if let Some((start, buf)) = current.take() {
                        regions.push(CoalescedRegion {
                            offset: start,
                            data: Bytes::from(buf),
                        });
                    }
                    current = Some((offset, data.to_vec()));
                }
            }
        }

        if let Some((start, buf)) = current {
            regions.push(CoalescedRegion {
                offset: start,
                data: Bytes::from(buf),
            });
        }

        regions
    }
}

pub struct CoalescedRegion {
    pub offset: u32,
    pub data: Bytes,
}

type CacheKey = (String, u32);

pub struct BlockCache {
    pieces: DashMap<CacheKey, PieceBlocks>,
    hash_states: DashMap<CacheKey, HashState>,
    memory_used: AtomicUsize,
    memory_limit: usize,
}

impl BlockCache {
    pub fn new(memory_limit: usize) -> Arc<Self> {
        Arc::new(Self {
            pieces: DashMap::new(),
            hash_states: DashMap::new(),
            memory_used: AtomicUsize::new(0),
            memory_limit,
        })
    }

    pub fn add_block(
        &self,
        info_hash: &str,
        piece_index: u32,
        offset: u32,
        data: Bytes,
        piece_length: u32,
        hash_version: u8,
    ) -> bool {
        let key = (info_hash.to_string(), piece_index);
        let data_len = data.len();

        {
            let mut piece = self.pieces.entry(key.clone()).or_insert_with(|| {
                let state = if hash_version == 2 {
                    HashState::new_v2()
                } else {
                    HashState::new_v1()
                };
                self.hash_states.insert(key.clone(), state);
                PieceBlocks::new(piece_length)
            });

            if piece.blocks.insert(offset, data).is_none() {
                self.memory_used.fetch_add(data_len, Ordering::Relaxed);
            }

            self.try_advance_hash(&key, &mut piece);
            piece.is_complete()
        }
    }

    fn try_advance_hash(&self, key: &CacheKey, piece: &mut PieceBlocks) {
        if let Some(mut state) = self.hash_states.get_mut(key) {
            let mut next_offset = piece.bytes_hashed;
            while let Some(block) = piece.blocks.get(&next_offset) {
                state.update(block);
                next_offset += block.len() as u32;
            }
            piece.bytes_hashed = next_offset;
        }
    }

    pub fn finalize_and_verify(&self, info_hash: &str, piece_index: u32, expected: &[u8]) -> bool {
        let key = (info_hash.to_string(), piece_index);

        if let Some(mut piece) = self.pieces.get_mut(&key) {
            self.try_advance_hash(&key, &mut piece);
            if piece.bytes_hashed != piece.piece_length {
                return false;
            }
        }

        if let Some((_, state)) = self.hash_states.remove(&key) {
            let computed = state.finalize();
            computed == expected
        } else {
            false
        }
    }

    pub fn get_coalesced_regions(&self, info_hash: &str, piece_index: u32) -> Vec<CoalescedRegion> {
        let key = (info_hash.to_string(), piece_index);
        self.pieces
            .get(&key)
            .map(|p| p.coalesce())
            .unwrap_or_default()
    }

    pub fn get_assembled_piece(&self, info_hash: &str, piece_index: u32) -> Option<Bytes> {
        let key = (info_hash.to_string(), piece_index);
        self.pieces.get(&key).map(|p| p.assemble())
    }

    pub fn remove_piece(&self, info_hash: &str, piece_index: u32) -> Option<Bytes> {
        let key = (info_hash.to_string(), piece_index);
        self.hash_states.remove(&key);
        if let Some((_, piece)) = self.pieces.remove(&key) {
            let bytes_freed = piece.total_bytes();
            self.memory_used.fetch_sub(bytes_freed, Ordering::Relaxed);
            Some(piece.assemble())
        } else {
            None
        }
    }

    pub fn has_piece(&self, info_hash: &str, piece_index: u32) -> bool {
        let key = (info_hash.to_string(), piece_index);
        self.pieces.contains_key(&key)
    }

    pub fn is_piece_complete(&self, info_hash: &str, piece_index: u32) -> bool {
        let key = (info_hash.to_string(), piece_index);
        self.pieces.get(&key).map(|p| p.is_complete()).unwrap_or(false)
    }

    pub fn memory_used(&self) -> usize {
        self.memory_used.load(Ordering::Relaxed)
    }

    pub fn memory_limit(&self) -> usize {
        self.memory_limit
    }

    pub fn is_under_pressure(&self) -> bool {
        self.memory_used() > (self.memory_limit as f32 * 0.9) as usize
    }

    pub fn pieces_count(&self) -> usize {
        self.pieces.len()
    }

    pub fn clear(&self) {
        self.pieces.clear();
        self.hash_states.clear();
        self.memory_used.store(0, Ordering::Relaxed);
    }
}
