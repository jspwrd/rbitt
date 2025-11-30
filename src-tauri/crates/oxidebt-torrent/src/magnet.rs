
use crate::error::TorrentError;
use crate::info_hash::{InfoHash, InfoHashV1, InfoHashV2};
use url::Url;

#[derive(Debug, Clone)]
pub struct MagnetLink {
    pub info_hash: InfoHash,
    pub display_name: Option<String>,
    pub trackers: Vec<String>,
    pub web_seeds: Vec<String>,
    pub peer_addresses: Vec<String>,
}

impl MagnetLink {
    pub fn parse(uri: &str) -> Result<Self, TorrentError> {
        let url = Url::parse(uri)
            .map_err(|e| TorrentError::InvalidMagnetLink(format!("invalid URL: {}", e)))?;

        if url.scheme() != "magnet" {
            return Err(TorrentError::InvalidMagnetLink(format!(
                "expected magnet scheme, got {}",
                url.scheme()
            )));
        }

        let mut info_hash_v1: Option<InfoHashV1> = None;
        let mut info_hash_v2: Option<InfoHashV2> = None;
        let mut display_name = None;
        let mut trackers = Vec::new();
        let mut web_seeds = Vec::new();
        let mut peer_addresses = Vec::new();

        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "xt" => {
                    if let Some(hash) = value.strip_prefix("urn:btih:") {
                        let hash = if hash.len() == 40 {
                            InfoHashV1::from_hex(hash)?
                        } else if hash.len() == 32 {
                            decode_base32_v1(hash)?
                        } else {
                            return Err(TorrentError::InvalidMagnetLink(format!(
                                "invalid btih length: {}",
                                hash.len()
                            )));
                        };
                        info_hash_v1 = Some(hash);
                    } else if let Some(hash) = value.strip_prefix("urn:btmh:1220") {
                        let hash = InfoHashV2::from_hex(hash)?;
                        info_hash_v2 = Some(hash);
                    }
                }
                "dn" => {
                    display_name = Some(value.to_string());
                }
                "tr" => {
                    trackers.push(value.to_string());
                }
                "ws" => {
                    web_seeds.push(value.to_string());
                }
                "x.pe" => {
                    peer_addresses.push(value.to_string());
                }
                _ => {}
            }
        }

        let info_hash = match (info_hash_v1, info_hash_v2) {
            (Some(v1), Some(v2)) => InfoHash::Hybrid { v1, v2 },
            (Some(v1), None) => InfoHash::V1(v1),
            (None, Some(v2)) => InfoHash::V2(v2),
            (None, None) => {
                return Err(TorrentError::InvalidMagnetLink(
                    "no info hash found".to_string(),
                ))
            }
        };

        Ok(MagnetLink {
            info_hash,
            display_name,
            trackers,
            web_seeds,
            peer_addresses,
        })
    }

    pub fn to_uri(&self) -> String {
        let mut uri = String::from("magnet:?");

        match &self.info_hash {
            InfoHash::V1(h) => {
                uri.push_str("xt=urn:btih:");
                uri.push_str(&h.to_hex());
            }
            InfoHash::V2(h) => {
                uri.push_str("xt=urn:btmh:1220");
                uri.push_str(&h.to_hex());
            }
            InfoHash::Hybrid { v1, v2 } => {
                uri.push_str("xt=urn:btih:");
                uri.push_str(&v1.to_hex());
                uri.push_str("&xt=urn:btmh:1220");
                uri.push_str(&v2.to_hex());
            }
        }

        if let Some(ref name) = self.display_name {
            uri.push_str("&dn=");
            uri.push_str(&url_encode(name));
        }

        for tracker in &self.trackers {
            uri.push_str("&tr=");
            uri.push_str(&url_encode(tracker));
        }

        for ws in &self.web_seeds {
            uri.push_str("&ws=");
            uri.push_str(&url_encode(ws));
        }

        for peer in &self.peer_addresses {
            uri.push_str("&x.pe=");
            uri.push_str(&url_encode(peer));
        }

        uri
    }
}

fn decode_base32_v1(s: &str) -> Result<InfoHashV1, TorrentError> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    let s = s.to_uppercase();
    let mut result = Vec::with_capacity(20);
    let mut buffer: u64 = 0;
    let mut bits = 0;

    for c in s.bytes() {
        let value = ALPHABET
            .iter()
            .position(|&x| x == c)
            .ok_or_else(|| TorrentError::InvalidMagnetLink("invalid base32 character".into()))?
            as u64;

        buffer = (buffer << 5) | value;
        bits += 5;

        if bits >= 8 {
            bits -= 8;
            result.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }

    InfoHashV1::from_bytes(&result)
}

fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{:02X}", byte));
        }
    }
    result
}
