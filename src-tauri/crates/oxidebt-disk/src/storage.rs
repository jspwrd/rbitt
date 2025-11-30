use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AllocationMode {
    #[default]
    Sparse,
    Full,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub length: u64,
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub struct PieceInfo {
    pub index: u32,
    pub hash: Vec<u8>,
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug)]
pub struct PieceFileSpan {
    pub file_index: usize,
    pub file_offset: u64,
    pub length: u64,
}

impl FileEntry {
    pub fn new(path: PathBuf, length: u64, offset: u64) -> Self {
        Self {
            path,
            length,
            offset,
        }
    }

    pub fn byte_range(&self) -> std::ops::Range<u64> {
        self.offset..self.offset + self.length
    }

    pub fn contains_offset(&self, offset: u64) -> bool {
        offset >= self.offset && offset < self.offset + self.length
    }
}

impl PieceInfo {
    pub fn v1(index: u32, hash: [u8; 20], offset: u64, length: u64) -> Self {
        Self {
            index,
            hash: hash.to_vec(),
            offset,
            length,
        }
    }

    pub fn v2(index: u32, hash: [u8; 32], offset: u64, length: u64) -> Self {
        Self {
            index,
            hash: hash.to_vec(),
            offset,
            length,
        }
    }

    pub fn byte_range(&self) -> std::ops::Range<u64> {
        self.offset..self.offset + self.length
    }
}
