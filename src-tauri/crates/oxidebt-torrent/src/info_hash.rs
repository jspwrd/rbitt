
use crate::error::TorrentError;
use sha1::{Digest as _, Sha1};
use sha2::Sha256;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct InfoHashV1(pub [u8; 20]);

impl InfoHashV1 {
    pub fn from_info_bytes(info_bytes: &[u8]) -> Self {
        let mut hasher = Sha1::new();
        hasher.update(info_bytes);
        let result = hasher.finalize();
        let mut hash = [0u8; 20];
        hash.copy_from_slice(&result);
        Self(hash)
    }

    pub fn from_hex(s: &str) -> Result<Self, TorrentError> {
        let bytes = hex::decode(s).map_err(|_| TorrentError::InvalidInfoHashLength(s.len()))?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TorrentError> {
        if bytes.len() != 20 {
            return Err(TorrentError::InvalidInfoHashLength(bytes.len()));
        }
        let mut hash = [0u8; 20];
        hash.copy_from_slice(bytes);
        Ok(Self(hash))
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn url_encode(&self) -> String {
        let mut result = String::with_capacity(60);
        for byte in &self.0 {
            if byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'_' || *byte == b'.' {
                result.push(*byte as char);
            } else {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
        result
    }
}

impl fmt::Debug for InfoHashV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InfoHashV1({})", self.to_hex())
    }
}

impl fmt::Display for InfoHashV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct InfoHashV2(pub [u8; 32]);

impl InfoHashV2 {
    pub fn from_info_bytes(info_bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(info_bytes);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Self(hash)
    }

    pub fn from_hex(s: &str) -> Result<Self, TorrentError> {
        let bytes = hex::decode(s).map_err(|_| TorrentError::InvalidInfoHashLength(s.len()))?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TorrentError> {
        if bytes.len() != 32 {
            return Err(TorrentError::InvalidInfoHashLength(bytes.len()));
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(bytes);
        Ok(Self(hash))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn truncated(&self) -> [u8; 20] {
        let mut result = [0u8; 20];
        result.copy_from_slice(&self.0[..20]);
        result
    }
}

impl fmt::Debug for InfoHashV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InfoHashV2({})", self.to_hex())
    }
}

impl fmt::Display for InfoHashV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfoHash {
    V1(InfoHashV1),
    V2(InfoHashV2),
    Hybrid { v1: InfoHashV1, v2: InfoHashV2 },
}

impl InfoHash {
    pub fn v1(&self) -> Option<&InfoHashV1> {
        match self {
            InfoHash::V1(h) => Some(h),
            InfoHash::Hybrid { v1, .. } => Some(v1),
            InfoHash::V2(_) => None,
        }
    }

    pub fn v2(&self) -> Option<&InfoHashV2> {
        match self {
            InfoHash::V2(h) => Some(h),
            InfoHash::Hybrid { v2, .. } => Some(v2),
            InfoHash::V1(_) => None,
        }
    }

    pub fn primary_bytes(&self) -> &[u8] {
        match self {
            InfoHash::V1(h) => &h.0,
            InfoHash::V2(h) => &h.0,
            InfoHash::Hybrid { v1, .. } => &v1.0,
        }
    }
}
