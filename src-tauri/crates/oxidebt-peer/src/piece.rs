use crate::bitfield::Bitfield;
use crate::error::PeerError;
use bytes::Bytes;
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const BLOCK_SIZE: u32 = 16384;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockRequest {
    pub piece_index: u32,
    pub offset: u32,
    pub length: u32,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub piece_index: u32,
    pub offset: u32,
    pub data: Bytes,
}

#[derive(Debug)]
struct PieceState {
    blocks: HashMap<u32, Bytes>,
    pending_blocks: HashMap<u32, Instant>,
    piece_length: u32,
    #[allow(dead_code)]
    started_at: Instant,
}

impl PieceState {
    fn new(piece_length: u32) -> Self {
        Self {
            blocks: HashMap::new(),
            pending_blocks: HashMap::new(),
            piece_length,
            started_at: Instant::now(),
        }
    }

    fn is_complete(&self) -> bool {
        let block_count = self.piece_length.div_ceil(BLOCK_SIZE);
        self.blocks.len() as u32 == block_count
    }

    fn assemble(&self) -> Bytes {
        let mut data = Vec::with_capacity(self.piece_length as usize);
        let block_count = self.piece_length.div_ceil(BLOCK_SIZE);

        for i in 0..block_count {
            let offset = i * BLOCK_SIZE;
            if let Some(block) = self.blocks.get(&offset) {
                data.extend_from_slice(block);
            }
        }

        Bytes::from(data)
    }

    fn expired_requests(&self) -> Vec<u32> {
        let now = Instant::now();
        self.pending_blocks
            .iter()
            .filter(|(_, &sent_at)| now.duration_since(sent_at) > REQUEST_TIMEOUT)
            .map(|(&offset, _)| offset)
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
struct PieceWithAvailability {
    availability: u32,
    piece_index: u32,
}

impl PartialEq for PieceWithAvailability {
    fn eq(&self, other: &Self) -> bool {
        self.availability == other.availability && self.piece_index == other.piece_index
    }
}

impl Eq for PieceWithAvailability {}

impl PartialOrd for PieceWithAvailability {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for PieceWithAvailability {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.availability
            .cmp(&other.availability)
            .then(self.piece_index.cmp(&other.piece_index))
    }
}

struct AvailabilityIndex {
    piece_to_availability: Vec<AtomicU32>,
}

impl AvailabilityIndex {
    fn new(piece_count: usize) -> Self {
        let mut piece_to_availability = Vec::with_capacity(piece_count);
        for _ in 0..piece_count {
            piece_to_availability.push(AtomicU32::new(0));
        }
        Self {
            piece_to_availability,
        }
    }

    fn get_availability(&self, piece_index: u32) -> u32 {
        self.piece_to_availability
            .get(piece_index as usize)
            .map(|a| a.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    fn increment(&self, piece_index: u32) -> (u32, u32) {
        if let Some(atomic) = self.piece_to_availability.get(piece_index as usize) {
            let old = atomic.fetch_add(1, Ordering::Relaxed);
            (old, old + 1)
        } else {
            (0, 0)
        }
    }

    fn decrement(&self, piece_index: u32) -> (u32, u32) {
        if let Some(atomic) = self.piece_to_availability.get(piece_index as usize) {
            loop {
                let old = atomic.load(Ordering::Relaxed);
                if old == 0 {
                    return (0, 0);
                }
                match atomic.compare_exchange_weak(
                    old,
                    old - 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return (old, old - 1),
                    Err(_) => continue,
                }
            }
        } else {
            (0, 0)
        }
    }
}

struct DownloadablePieces {
    candidates: BTreeSet<PieceWithAvailability>,
    in_candidates: HashSet<u32>,
}

impl DownloadablePieces {
    fn new() -> Self {
        Self {
            candidates: BTreeSet::new(),
            in_candidates: HashSet::new(),
        }
    }

    fn add(&mut self, piece_index: u32, availability: u32) {
        if self.in_candidates.insert(piece_index) {
            self.candidates.insert(PieceWithAvailability {
                availability,
                piece_index,
            });
        }
    }

    fn remove(&mut self, piece_index: u32, availability: u32) {
        if self.in_candidates.remove(&piece_index) {
            self.candidates.remove(&PieceWithAvailability {
                availability,
                piece_index,
            });
        }
    }

    fn update_availability(&mut self, piece_index: u32, old_avail: u32, new_avail: u32) {
        if self.in_candidates.contains(&piece_index) {
            self.candidates.remove(&PieceWithAvailability {
                availability: old_avail,
                piece_index,
            });
            self.candidates.insert(PieceWithAvailability {
                availability: new_avail,
                piece_index,
            });
        }
    }
}

pub struct PieceManager {
    piece_count: usize,
    piece_length: u64,
    total_length: u64,
    our_bitfield: RwLock<Bitfield>,
    active_pieces: RwLock<HashMap<u32, PieceState>>,
    availability_index: AvailabilityIndex,
    downloadable: RwLock<DownloadablePieces>,
    completed_pieces: RwLock<HashSet<u32>>,
    verified_pieces: RwLock<HashSet<u32>>,
    verification_complete: AtomicBool,
    verifying_pieces: RwLock<HashSet<u32>>,
}

impl PieceManager {
    pub fn new(piece_count: usize, piece_length: u64, total_length: u64) -> Arc<Self> {
        let mut downloadable = DownloadablePieces::new();
        for i in 0..piece_count {
            downloadable.add(i as u32, 0);
        }
        Arc::new(Self {
            piece_count,
            piece_length,
            total_length,
            our_bitfield: RwLock::new(Bitfield::new(piece_count)),
            active_pieces: RwLock::new(HashMap::new()),
            availability_index: AvailabilityIndex::new(piece_count),
            downloadable: RwLock::new(downloadable),
            completed_pieces: RwLock::new(HashSet::new()),
            verified_pieces: RwLock::new(HashSet::new()),
            verification_complete: AtomicBool::new(false),
            verifying_pieces: RwLock::new(HashSet::new()),
        })
    }

    pub fn bitfield(&self) -> Bitfield {
        self.our_bitfield.read().clone()
    }

    pub fn is_complete(&self) -> bool {
        self.our_bitfield.read().is_complete()
    }

    pub fn have_count(&self) -> usize {
        self.our_bitfield.read().count()
    }

    /// Returns the number of pieces currently being downloaded
    pub fn active_piece_count(&self) -> usize {
        self.active_pieces.read().len()
    }

    pub fn mark_piece_complete(&self, index: u32) {
        let mut bf = self.our_bitfield.write();
        bf.set_piece(index as usize);
        self.completed_pieces.write().insert(index);
        self.active_pieces.write().remove(&index);
        self.verifying_pieces.write().remove(&index);
        let avail = self.availability_index.get_availability(index);
        self.downloadable.write().remove(index, avail);
    }

    pub fn mark_piece_failed(&self, index: u32) {
        self.active_pieces.write().remove(&index);
        self.verifying_pieces.write().remove(&index);
        let avail = self.availability_index.get_availability(index);
        self.downloadable.write().add(index, avail);
    }

    pub fn start_verifying(&self, index: u32) -> bool {
        let mut verifying = self.verifying_pieces.write();
        if verifying.contains(&index) {
            false
        } else {
            verifying.insert(index);
            true
        }
    }

    pub fn finish_verifying(&self, index: u32) {
        self.verifying_pieces.write().remove(&index);
    }

    pub fn mark_piece_verified(&self, index: u32) {
        self.verified_pieces.write().insert(index);
    }

    pub fn mark_verification_complete(&self) {
        self.verification_complete.store(true, Ordering::Release);
    }

    pub fn is_verification_complete(&self) -> bool {
        self.verification_complete.load(Ordering::Acquire)
    }

    pub fn is_piece_verified(&self, index: u32) -> bool {
        if self.verification_complete.load(Ordering::Acquire) {
            return true;
        }
        self.verified_pieces.read().contains(&index)
    }

    pub fn verified_count(&self) -> usize {
        if self.verification_complete.load(Ordering::Acquire) {
            self.piece_count
        } else {
            self.verified_pieces.read().len()
        }
    }

    pub fn update_availability(&self, peer_bitfield: &Bitfield) {
        let mut downloadable = self.downloadable.write();
        for i in 0..self.piece_count {
            if peer_bitfield.has_piece(i) {
                let (old_avail, new_avail) = self.availability_index.increment(i as u32);
                downloadable.update_availability(i as u32, old_avail, new_avail);
            }
        }
    }

    pub fn decrement_availability(&self, peer_bitfield: &Bitfield) {
        let mut downloadable = self.downloadable.write();
        for i in 0..self.piece_count {
            if peer_bitfield.has_piece(i) {
                let (old_avail, new_avail) = self.availability_index.decrement(i as u32);
                downloadable.update_availability(i as u32, old_avail, new_avail);
            }
        }
    }

    pub fn increment_piece_availability(&self, index: usize) {
        if index < self.piece_count {
            let (old_avail, new_avail) = self.availability_index.increment(index as u32);
            self.downloadable
                .write()
                .update_availability(index as u32, old_avail, new_avail);
        }
    }

    /// Picks the next piece to download using a hybrid strategy:
    /// - Cold start (< 4 pieces): Random selection to avoid all peers requesting same piece
    /// - Normal: Rarest-first to improve swarm health
    /// - Endgame: Already in-progress pieces for parallel completion
    pub fn pick_piece(&self, peer_bitfield: &Bitfield) -> Option<u32> {
        let have_count = self.have_count();
        let active = self.active_pieces.read();
        let downloadable = self.downloadable.read();

        // Cold start: use random selection from first N available pieces
        // This avoids all peers requesting the same rarest piece at startup
        const COLD_START_THRESHOLD: usize = 4;
        const RANDOM_POOL_SIZE: usize = 10;

        if have_count < COLD_START_THRESHOLD {
            // Collect available pieces (not active, peer has them)
            let mut candidates: Vec<u32> = downloadable
                .candidates
                .iter()
                .filter(|pwa| {
                    peer_bitfield.has_piece(pwa.piece_index as usize)
                        && !active.contains_key(&pwa.piece_index)
                })
                .take(RANDOM_POOL_SIZE)
                .map(|pwa| pwa.piece_index)
                .collect();

            if !candidates.is_empty() {
                let mut rng = rand::rng();
                candidates.shuffle(&mut rng);
                return candidates.first().copied();
            }
        }

        // Normal mode: rarest-first selection
        for pwa in &downloadable.candidates {
            let idx = pwa.piece_index;
            if peer_bitfield.has_piece(idx as usize) && !active.contains_key(&idx) {
                return Some(idx);
            }
        }

        // Try to help with pieces already in progress (for parallelism)
        for pwa in &downloadable.candidates {
            let idx = pwa.piece_index;
            if peer_bitfield.has_piece(idx as usize) {
                if let Some(state) = active.get(&idx) {
                    let piece_len = self.piece_size(idx) as u32;
                    let block_count = piece_len.div_ceil(BLOCK_SIZE);
                    let received_or_pending = state.blocks.len() + state.pending_blocks.len();
                    if (received_or_pending as u32) < block_count {
                        return Some(idx);
                    }
                }
            }
        }

        None
    }

    pub fn pick_piece_sequential(&self, peer_bitfield: &Bitfield) -> Option<u32> {
        self.pick_piece_sequential_with_priorities(peer_bitfield, None)
    }

    /// Pick a piece sequentially, optionally respecting file priorities.
    /// `piece_priorities` is an optional slice where 0 means "skip this piece".
    pub fn pick_piece_sequential_with_priorities(
        &self,
        peer_bitfield: &Bitfield,
        piece_priorities: Option<&[u8]>,
    ) -> Option<u32> {
        let our_bf = self.our_bitfield.read();
        let active = self.active_pieces.read();

        for i in 0..self.piece_count {
            // Skip pieces with priority 0 (Skip)
            if let Some(priorities) = piece_priorities {
                if i < priorities.len() && priorities[i] == 0 {
                    continue;
                }
            }

            if !our_bf.has_piece(i)
                && peer_bitfield.has_piece(i)
                && !active.contains_key(&(i as u32))
            {
                return Some(i as u32);
            }
        }

        // Try in-progress pieces
        for i in 0..self.piece_count {
            if let Some(priorities) = piece_priorities {
                if i < priorities.len() && priorities[i] == 0 {
                    continue;
                }
            }

            if !our_bf.has_piece(i) && peer_bitfield.has_piece(i) {
                if let Some(state) = active.get(&(i as u32)) {
                    let piece_len = self.piece_size(i as u32) as u32;
                    let block_count = piece_len.div_ceil(BLOCK_SIZE);
                    let received_or_pending = state.blocks.len() + state.pending_blocks.len();
                    if (received_or_pending as u32) < block_count {
                        return Some(i as u32);
                    }
                }
            }
        }

        None
    }

    /// Pick a piece using rarest-first, optionally respecting file priorities.
    /// `piece_priorities` is an optional slice where 0 means "skip this piece".
    pub fn pick_piece_with_priorities(
        &self,
        peer_bitfield: &Bitfield,
        piece_priorities: Option<&[u8]>,
    ) -> Option<u32> {
        let have_count = self.have_count();
        let active = self.active_pieces.read();
        let downloadable = self.downloadable.read();

        // Cold start: use random selection from first N available pieces
        const COLD_START_THRESHOLD: usize = 4;
        const RANDOM_POOL_SIZE: usize = 10;

        if have_count < COLD_START_THRESHOLD {
            let mut candidates: Vec<u32> = downloadable
                .candidates
                .iter()
                .filter(|pwa| {
                    // Skip pieces with priority 0
                    if let Some(priorities) = piece_priorities {
                        if (pwa.piece_index as usize) < priorities.len()
                            && priorities[pwa.piece_index as usize] == 0
                        {
                            return false;
                        }
                    }
                    peer_bitfield.has_piece(pwa.piece_index as usize)
                        && !active.contains_key(&pwa.piece_index)
                })
                .take(RANDOM_POOL_SIZE)
                .map(|pwa| pwa.piece_index)
                .collect();

            if !candidates.is_empty() {
                let mut rng = rand::rng();
                candidates.shuffle(&mut rng);
                return candidates.first().copied();
            }
        }

        // Normal mode: rarest-first selection
        for pwa in &downloadable.candidates {
            let idx = pwa.piece_index;

            // Skip pieces with priority 0
            if let Some(priorities) = piece_priorities {
                if (idx as usize) < priorities.len() && priorities[idx as usize] == 0 {
                    continue;
                }
            }

            if peer_bitfield.has_piece(idx as usize) && !active.contains_key(&idx) {
                return Some(idx);
            }
        }

        // Try to help with pieces already in progress
        for pwa in &downloadable.candidates {
            let idx = pwa.piece_index;

            if let Some(priorities) = piece_priorities {
                if (idx as usize) < priorities.len() && priorities[idx as usize] == 0 {
                    continue;
                }
            }

            if peer_bitfield.has_piece(idx as usize) {
                if let Some(state) = active.get(&idx) {
                    let piece_len = self.piece_size(idx) as u32;
                    let block_count = piece_len.div_ceil(BLOCK_SIZE);
                    let received_or_pending = state.blocks.len() + state.pending_blocks.len();
                    if (received_or_pending as u32) < block_count {
                        return Some(idx);
                    }
                }
            }
        }

        None
    }

    pub fn get_block_requests(&self, piece_index: u32) -> Vec<BlockRequest> {
        let piece_len = self.piece_size(piece_index);
        let active = self.active_pieces.read();
        let mut requests = Vec::new();

        let state = active.get(&piece_index);

        let mut offset = 0u32;
        while offset < piece_len as u32 {
            let length = std::cmp::min(BLOCK_SIZE, piece_len as u32 - offset);

            let should_request = match state {
                Some(s) => {
                    !s.blocks.contains_key(&offset) && !s.pending_blocks.contains_key(&offset)
                }
                None => true,
            };

            if should_request {
                requests.push(BlockRequest {
                    piece_index,
                    offset,
                    length,
                });
            }
            offset += length;
        }

        requests
    }

    pub fn start_piece(&self, piece_index: u32) {
        let piece_len = self.piece_size(piece_index) as u32;
        let mut active = self.active_pieces.write();
        active
            .entry(piece_index)
            .or_insert_with(|| PieceState::new(piece_len));
    }

    pub fn add_pending_block(&self, request: &BlockRequest) {
        let mut active = self.active_pieces.write();
        if let Some(state) = active.get_mut(&request.piece_index) {
            state.pending_blocks.insert(request.offset, Instant::now());
        }
    }

    pub fn receive_block(&self, block: Block) -> Result<bool, PeerError> {
        let mut active = self.active_pieces.write();

        let state = active
            .get_mut(&block.piece_index)
            .ok_or(PeerError::InvalidPieceIndex(block.piece_index))?;

        state.pending_blocks.remove(&block.offset);
        state.blocks.insert(block.offset, block.data);

        Ok(state.is_complete())
    }

    pub fn cancel_piece(&self, piece_index: u32) {
        self.active_pieces.write().remove(&piece_index);
    }

    pub fn get_stale_pieces(&self) -> Vec<u32> {
        let active = self.active_pieces.read();
        active
            .iter()
            .filter(|(_, state)| !state.expired_requests().is_empty())
            .map(|(&idx, _)| idx)
            .collect()
    }

    pub fn cleanup_stale_pieces(&self) -> Vec<u32> {
        let stale = self.get_stale_pieces();
        let mut active = self.active_pieces.write();
        for &idx in &stale {
            active.remove(&idx);
        }
        stale
    }

    pub fn assemble_piece(&self, piece_index: u32) -> Option<Bytes> {
        let active = self.active_pieces.read();
        active.get(&piece_index).map(|state| state.assemble())
    }

    pub fn piece_size(&self, index: u32) -> u64 {
        if self.piece_count == 0 {
            return 0;
        }
        if (index as usize) < self.piece_count - 1 {
            self.piece_length
        } else {
            let remainder = self.total_length % self.piece_length;
            if remainder == 0 {
                self.piece_length
            } else {
                remainder
            }
        }
    }

    pub fn get_endgame_requests(&self) -> Vec<BlockRequest> {
        let active = self.active_pieces.read();
        let mut requests = Vec::new();

        for (&piece_index, state) in active.iter() {
            for &offset in state.pending_blocks.keys() {
                let remaining = state.piece_length - offset;
                let length = std::cmp::min(BLOCK_SIZE, remaining);
                requests.push(BlockRequest {
                    piece_index,
                    offset,
                    length,
                });
            }
        }

        requests
    }

    pub fn is_endgame(&self) -> bool {
        let remaining = self.piece_count - self.have_count();
        remaining <= 10 && remaining > 0
    }

    pub fn cancel_block(&self, request: &BlockRequest) {
        let mut active = self.active_pieces.write();
        if let Some(state) = active.get_mut(&request.piece_index) {
            state.pending_blocks.remove(&request.offset);
        }
    }
}
