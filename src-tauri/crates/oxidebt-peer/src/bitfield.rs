use crate::error::PeerError;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub struct Bitfield {
    bits: Vec<u8>,
    piece_count: usize,
}

impl Bitfield {
    pub fn new(piece_count: usize) -> Self {
        let byte_count = piece_count.div_ceil(8);
        Self {
            bits: vec![0; byte_count],
            piece_count,
        }
    }

    pub fn from_bytes(bytes: &[u8], piece_count: usize) -> Result<Self, PeerError> {
        let expected_len = piece_count.div_ceil(8);
        if bytes.len() != expected_len {
            return Err(PeerError::BitfieldLengthMismatch {
                expected: expected_len,
                actual: bytes.len(),
            });
        }

        let mut bf = Self {
            bits: bytes.to_vec(),
            piece_count,
        };

        bf.clear_spare_bits();
        Ok(bf)
    }

    pub fn full(piece_count: usize) -> Self {
        let byte_count = piece_count.div_ceil(8);
        let mut bf = Self {
            bits: vec![0xFF; byte_count],
            piece_count,
        };
        bf.clear_spare_bits();
        bf
    }

    pub fn has_piece(&self, index: usize) -> bool {
        if index >= self.piece_count {
            return false;
        }
        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        (self.bits[byte_index] >> bit_index) & 1 == 1
    }

    pub fn set_piece(&mut self, index: usize) {
        if index >= self.piece_count {
            return;
        }
        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        self.bits[byte_index] |= 1 << bit_index;
    }

    pub fn clear_piece(&mut self, index: usize) {
        if index >= self.piece_count {
            return;
        }
        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        self.bits[byte_index] &= !(1 << bit_index);
    }

    pub fn count(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }

    pub fn is_complete(&self) -> bool {
        self.count() == self.piece_count
    }

    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&b| b == 0)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bits
    }

    pub fn piece_count(&self) -> usize {
        self.piece_count
    }

    pub fn count_ones(&self) -> usize {
        self.count()
    }

    pub fn len(&self) -> usize {
        self.piece_count
    }

    pub fn to_bytes(&self) -> bytes::Bytes {
        bytes::Bytes::copy_from_slice(&self.bits)
    }

    pub fn missing_pieces(&self, our_bitfield: &Bitfield) -> Vec<usize> {
        let mut missing = Vec::new();
        for i in 0..self.piece_count {
            if self.has_piece(i) && !our_bitfield.has_piece(i) {
                missing.push(i);
            }
        }
        missing
    }

    fn clear_spare_bits(&mut self) {
        let spare = (self.bits.len() * 8) - self.piece_count;
        if spare > 0 && spare < 8 && !self.bits.is_empty() {
            let mask = 0xFFu8 << spare;
            let last = self.bits.len() - 1;
            self.bits[last] &= mask;
        }
    }
}

#[allow(dead_code)]
pub struct CachedBitfield {
    bits: Vec<u8>,
    piece_count: usize,
    cached_count: AtomicUsize,
}

#[allow(dead_code)]
impl CachedBitfield {
    pub fn new(piece_count: usize) -> Self {
        let byte_count = piece_count.div_ceil(8);
        Self {
            bits: vec![0; byte_count],
            piece_count,
            cached_count: AtomicUsize::new(0),
        }
    }

    pub fn from_bytes(bytes: &[u8], piece_count: usize) -> Result<Self, PeerError> {
        let expected_len = piece_count.div_ceil(8);
        if bytes.len() != expected_len {
            return Err(PeerError::BitfieldLengthMismatch {
                expected: expected_len,
                actual: bytes.len(),
            });
        }

        let mut bf = Self {
            bits: bytes.to_vec(),
            piece_count,
            cached_count: AtomicUsize::new(0),
        };

        bf.clear_spare_bits();
        bf.cached_count.store(bf.compute_count(), Ordering::Relaxed);
        Ok(bf)
    }

    pub fn full(piece_count: usize) -> Self {
        let byte_count = piece_count.div_ceil(8);
        let mut bf = Self {
            bits: vec![0xFF; byte_count],
            piece_count,
            cached_count: AtomicUsize::new(piece_count),
        };
        bf.clear_spare_bits();
        bf.cached_count.store(piece_count, Ordering::Relaxed);
        bf
    }

    pub fn has_piece(&self, index: usize) -> bool {
        if index >= self.piece_count {
            return false;
        }
        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        (self.bits[byte_index] >> bit_index) & 1 == 1
    }

    pub fn set_piece(&mut self, index: usize) {
        if index >= self.piece_count {
            return;
        }
        if !self.has_piece(index) {
            let byte_index = index / 8;
            let bit_index = 7 - (index % 8);
            self.bits[byte_index] |= 1 << bit_index;
            self.cached_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn clear_piece(&mut self, index: usize) {
        if index >= self.piece_count {
            return;
        }
        if self.has_piece(index) {
            let byte_index = index / 8;
            let bit_index = 7 - (index % 8);
            self.bits[byte_index] &= !(1 << bit_index);
            self.cached_count.fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub fn count(&self) -> usize {
        self.cached_count.load(Ordering::Relaxed)
    }

    fn compute_count(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }

    pub fn is_complete(&self) -> bool {
        self.count() == self.piece_count
    }

    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bits
    }

    pub fn piece_count(&self) -> usize {
        self.piece_count
    }

    pub fn to_bytes(&self) -> bytes::Bytes {
        bytes::Bytes::copy_from_slice(&self.bits)
    }

    pub fn to_bitfield(&self) -> Bitfield {
        Bitfield {
            bits: self.bits.clone(),
            piece_count: self.piece_count,
        }
    }

    fn clear_spare_bits(&mut self) {
        let spare = (self.bits.len() * 8) - self.piece_count;
        if spare > 0 && spare < 8 && !self.bits.is_empty() {
            let mask = 0xFFu8 << spare;
            let last = self.bits.len() - 1;
            self.bits[last] &= mask;
        }
    }
}
