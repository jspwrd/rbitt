use crate::error::TrackerError;
use crate::response::{AnnounceResponse, Peer, ScrapeResponse, ScrapeStats};
use crate::AnnounceParams;
use oxidebt_bencode::{decode, Value};
use oxidebt_constants::{HTTP_TRACKER_TIMEOUT, USER_AGENT};
use oxidebt_torrent::InfoHashV1;
use reqwest::Client;
use url::Url;

/// Safely convert an i64 to u32, returning an error if out of bounds
fn i64_to_u32(value: i64, field_name: &str) -> Result<u32, TrackerError> {
    if value < 0 || value > u32::MAX as i64 {
        return Err(TrackerError::InvalidResponse(format!(
            "{} value {} out of u32 bounds",
            field_name, value
        )));
    }
    Ok(value as u32)
}

/// Safely convert an i64 to u16, returning an error if out of bounds
fn i64_to_u16(value: i64, field_name: &str) -> Result<u16, TrackerError> {
    if value < 0 || value > u16::MAX as i64 {
        return Err(TrackerError::InvalidResponse(format!(
            "{} value {} out of u16 bounds",
            field_name, value
        )));
    }
    Ok(value as u16)
}

pub struct HttpTracker {
    client: Client,
}

impl HttpTracker {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TRACKER_TIMEOUT)
            .build()
            .expect("failed to create http client");

        Self { client }
    }

    pub async fn announce(
        &self,
        params: &AnnounceParams<'_>,
    ) -> Result<AnnounceResponse, TrackerError> {
        let parsed_url = Url::parse(params.url)?;

        let mut query_parts = vec![
            format!(
                "info_hash={}",
                url_encode_bytes(params.info_hash.as_bytes())
            ),
            format!("peer_id={}", url_encode_bytes(params.peer_id)),
            format!("port={}", params.port),
            format!("uploaded={}", params.uploaded),
            format!("downloaded={}", params.downloaded),
            format!("left={}", params.left),
            "compact=1".to_string(),
            "numwant=500".to_string(),
        ];

        if let Some(event_str) = params.event.as_str() {
            query_parts.push(format!("event={}", event_str));
        }

        let query_string = query_parts.join("&");
        let full_url = if parsed_url.query().is_some() {
            format!("{}&{}", parsed_url.as_str(), query_string)
        } else {
            format!("{}?{}", parsed_url.as_str(), query_string)
        };

        let response = self.client.get(&full_url).send().await?;
        let body = response.bytes().await?;

        self.parse_announce_response(&body)
    }

    fn parse_announce_response(&self, data: &[u8]) -> Result<AnnounceResponse, TrackerError> {
        let value = decode(data)?;

        let dict = value
            .as_dict()
            .ok_or_else(|| TrackerError::InvalidResponse("expected dict".into()))?;

        if let Some(failure) = dict.get(b"failure reason".as_slice()) {
            let msg = failure.as_str().unwrap_or("unknown failure").to_string();
            return Err(TrackerError::TrackerFailure(msg));
        }

        let interval_raw = dict
            .get(b"interval".as_slice())
            .and_then(|v| v.as_integer())
            .ok_or_else(|| TrackerError::InvalidResponse("missing interval".into()))?;
        let interval = i64_to_u32(interval_raw, "interval")?;

        let min_interval = dict
            .get(b"min interval".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| i64_to_u32(v, "min_interval"))
            .transpose()?;

        let complete = dict
            .get(b"complete".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| i64_to_u32(v, "complete"))
            .transpose()?;

        let incomplete = dict
            .get(b"incomplete".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| i64_to_u32(v, "incomplete"))
            .transpose()?;

        let warning_message = dict
            .get(b"warning message".as_slice())
            .and_then(|v| v.as_str())
            .map(String::from);

        let tracker_id = dict
            .get(b"tracker id".as_slice())
            .and_then(|v| v.as_str())
            .map(String::from);

        let peers = self.parse_peers(dict.get(b"peers".as_slice()))?;
        let peers6 = self.parse_peers6(dict.get(b"peers6".as_slice()))?;

        Ok(AnnounceResponse {
            interval,
            min_interval,
            complete,
            incomplete,
            peers,
            peers6,
            warning_message,
            tracker_id,
        })
    }

    fn parse_peers(&self, value: Option<&Value>) -> Result<Vec<Peer>, TrackerError> {
        let Some(value) = value else {
            return Ok(Vec::new());
        };

        match value {
            Value::Bytes(data) => Ok(Peer::from_compact_v4(data)),
            Value::List(list) => {
                let mut peers = Vec::new();
                for item in list {
                    if let Some(dict) = item.as_dict() {
                        let ip_str = dict
                            .get(b"ip".as_slice())
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                TrackerError::InvalidResponse("peer missing ip".into())
                            })?;

                        let port_raw = dict
                            .get(b"port".as_slice())
                            .and_then(|v| v.as_integer())
                            .ok_or_else(|| {
                                TrackerError::InvalidResponse("peer missing port".into())
                            })?;
                        let port = i64_to_u16(port_raw, "port")?;

                        let peer_id = dict.get(b"peer id".as_slice()).and_then(|v| {
                            v.as_bytes().and_then(|b| {
                                if b.len() == 20 {
                                    let mut id = [0u8; 20];
                                    id.copy_from_slice(b);
                                    Some(id)
                                } else {
                                    None
                                }
                            })
                        });

                        if let Ok(ip) = ip_str.parse() {
                            peers.push(Peer {
                                addr: std::net::SocketAddr::new(ip, port),
                                peer_id,
                            });
                        }
                    }
                }
                Ok(peers)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn parse_peers6(&self, value: Option<&Value>) -> Result<Vec<Peer>, TrackerError> {
        let Some(Value::Bytes(data)) = value else {
            return Ok(Vec::new());
        };

        Ok(Peer::from_compact_v6(data))
    }

    pub async fn scrape(
        &self,
        url: &str,
        info_hashes: &[InfoHashV1],
    ) -> Result<ScrapeResponse, TrackerError> {
        let scrape_url = self.announce_to_scrape_url(url)?;
        let parsed_url = Url::parse(&scrape_url)?;

        let query_parts: Vec<String> = info_hashes
            .iter()
            .map(|hash| format!("info_hash={}", url_encode_bytes(hash.as_bytes())))
            .collect();

        let query_string = query_parts.join("&");
        let full_url = if parsed_url.query().is_some() {
            format!("{}&{}", parsed_url.as_str(), query_string)
        } else {
            format!("{}?{}", parsed_url.as_str(), query_string)
        };

        let response = self.client.get(&full_url).send().await?;
        let body = response.bytes().await?;

        self.parse_scrape_response(&body)
    }

    fn announce_to_scrape_url(&self, announce_url: &str) -> Result<String, TrackerError> {
        if let Some(pos) = announce_url.rfind("/announce") {
            let mut url = announce_url.to_string();
            url.replace_range(pos..pos + 9, "/scrape");
            Ok(url)
        } else {
            Err(TrackerError::InvalidResponse(
                "cannot convert announce URL to scrape URL".into(),
            ))
        }
    }

    fn parse_scrape_response(&self, data: &[u8]) -> Result<ScrapeResponse, TrackerError> {
        let value = decode(data)?;

        let dict = value
            .as_dict()
            .ok_or_else(|| TrackerError::InvalidResponse("expected dict".into()))?;

        if let Some(failure) = dict.get(b"failure reason".as_slice()) {
            let msg = failure.as_str().unwrap_or("unknown failure").to_string();
            return Err(TrackerError::TrackerFailure(msg));
        }

        let files_dict = dict
            .get(b"files".as_slice())
            .and_then(|v| v.as_dict())
            .ok_or_else(|| TrackerError::InvalidResponse("missing files dict".into()))?;

        let mut files = Vec::new();

        for (hash_bytes, stats_value) in files_dict.iter() {
            if hash_bytes.len() != 20 {
                continue;
            }

            let mut hash = [0u8; 20];
            hash.copy_from_slice(hash_bytes);

            let stats_dict = stats_value
                .as_dict()
                .ok_or_else(|| TrackerError::InvalidResponse("invalid stats".into()))?;

            let complete = stats_dict
                .get(b"complete".as_slice())
                .and_then(|v| v.as_integer())
                .map(|v| i64_to_u32(v, "complete"))
                .transpose()?
                .unwrap_or(0);

            let incomplete = stats_dict
                .get(b"incomplete".as_slice())
                .and_then(|v| v.as_integer())
                .map(|v| i64_to_u32(v, "incomplete"))
                .transpose()?
                .unwrap_or(0);

            let downloaded = stats_dict
                .get(b"downloaded".as_slice())
                .and_then(|v| v.as_integer())
                .map(|v| i64_to_u32(v, "downloaded"))
                .transpose()?
                .unwrap_or(0);

            files.push((
                hash,
                ScrapeStats {
                    complete,
                    incomplete,
                    downloaded,
                },
            ));
        }

        Ok(ScrapeResponse { files })
    }
}

impl Default for HttpTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn url_encode_bytes(bytes: &[u8]) -> String {
    let mut result = String::new();
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{:02X}", byte));
        }
    }
    result
}
