use crate::error::TorrentError;
use sha2::{Digest, Sha256};

const BLOCK_SIZE: usize = 16384;

#[derive(Debug, Clone)]
pub struct MerkleTree {
    layers: Vec<Vec<[u8; 32]>>,
}

impl MerkleTree {
    pub fn from_piece_data(data: &[u8]) -> Self {
        let block_count = data.len().div_ceil(BLOCK_SIZE);
        let leaf_count = block_count.next_power_of_two();

        let mut leaves = Vec::with_capacity(leaf_count);

        for i in 0..leaf_count {
            if i < block_count {
                let start = i * BLOCK_SIZE;
                let end = std::cmp::min(start + BLOCK_SIZE, data.len());
                let block = &data[start..end];
                leaves.push(Self::hash_leaf(block));
            } else {
                leaves.push([0u8; 32]);
            }
        }

        Self::from_leaves(leaves)
    }

    pub fn from_leaves(leaves: Vec<[u8; 32]>) -> Self {
        if leaves.is_empty() {
            return Self {
                layers: vec![vec![[0u8; 32]]],
            };
        }

        let mut layers = vec![leaves];

        while layers.last().unwrap().len() > 1 {
            let current = layers.last().unwrap();
            let mut next = Vec::with_capacity(current.len().div_ceil(2));

            for pair in current.chunks(2) {
                let hash = if pair.len() == 2 {
                    Self::hash_pair(&pair[0], &pair[1])
                } else {
                    Self::hash_pair(&pair[0], &[0u8; 32])
                };
                next.push(hash);
            }

            layers.push(next);
        }

        Self { layers }
    }

    pub fn root(&self) -> [u8; 32] {
        self.layers
            .last()
            .and_then(|l| l.first())
            .copied()
            .unwrap_or([0u8; 32])
    }

    pub fn verify_block(
        &self,
        block_index: usize,
        block_data: &[u8],
        proof: &[[u8; 32]],
    ) -> Result<bool, TorrentError> {
        let leaf_hash = Self::hash_leaf(block_data);
        self.verify_proof(block_index, &leaf_hash, proof)
    }

    pub fn verify_proof(
        &self,
        mut index: usize,
        leaf_hash: &[u8; 32],
        proof: &[[u8; 32]],
    ) -> Result<bool, TorrentError> {
        let mut current = *leaf_hash;

        for sibling in proof {
            current = if index.is_multiple_of(2) {
                Self::hash_pair(&current, sibling)
            } else {
                Self::hash_pair(sibling, &current)
            };
            index /= 2;
        }

        Ok(current == self.root())
    }

    pub fn generate_proof(&self, mut index: usize) -> Vec<[u8; 32]> {
        let mut proof = Vec::new();

        for layer in &self.layers[..self.layers.len().saturating_sub(1)] {
            let sibling_index = if index.is_multiple_of(2) {
                index + 1
            } else {
                index - 1
            };

            if sibling_index < layer.len() {
                proof.push(layer[sibling_index]);
            } else {
                proof.push([0u8; 32]);
            }

            index /= 2;
        }

        proof
    }

    pub fn depth(&self) -> usize {
        self.layers.len()
    }

    fn hash_leaf(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update([0x00]);
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update([0x01]);
        hasher.update(left);
        hasher.update(right);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
}
