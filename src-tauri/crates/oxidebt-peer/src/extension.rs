use bytes::{BufMut, Bytes, BytesMut};
use oxidebt_bencode::{decode, encode, Value};
pub use oxidebt_constants::{
    EXTENSION_HANDSHAKE_ID, METADATA_PIECE_SIZE, PEX_FLAG_PREFERS_ENCRYPTION, PEX_FLAG_REACHABLE,
    PEX_FLAG_SUPPORTS_HOLEPUNCH, PEX_FLAG_SUPPORTS_UTP, PEX_FLAG_UPLOAD_ONLY, UT_METADATA_ID,
    UT_PEX_ID,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataMessageType {
    Request = 0,
    Data = 1,
    Reject = 2,
}

impl TryFrom<u8> for MetadataMessageType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MetadataMessageType::Request),
            1 => Ok(MetadataMessageType::Data),
            2 => Ok(MetadataMessageType::Reject),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtensionHandshake {
    pub metadata_size: Option<u32>,
    pub ut_metadata: Option<u8>,
    pub ut_pex: Option<u8>,
    pub client: Option<String>,
    pub yourip: Option<Vec<u8>>,
    pub ipv6: Option<Vec<u8>>,
    pub ipv4: Option<Vec<u8>>,
    pub reqq: Option<u32>,
    pub listen_port: Option<u16>,
}

impl ExtensionHandshake {
    pub fn new() -> Self {
        Self {
            metadata_size: None,
            ut_metadata: Some(UT_METADATA_ID),
            ut_pex: Some(UT_PEX_ID),
            client: Some("oxidebt/0.1.0".to_string()),
            yourip: None,
            ipv6: None,
            ipv4: None,
            reqq: Some(250),
            listen_port: None,
        }
    }

    pub fn with_metadata_size(mut self, size: u32) -> Self {
        self.metadata_size = Some(size);
        self
    }

    pub fn with_listen_port(mut self, port: u16) -> Self {
        self.listen_port = Some(port);
        self
    }

    pub fn encode(&self) -> Bytes {
        let mut dict = BTreeMap::new();

        let mut m = BTreeMap::new();
        if let Some(id) = self.ut_metadata {
            m.insert(
                Bytes::from_static(b"ut_metadata"),
                Value::Integer(id as i64),
            );
        }
        if let Some(id) = self.ut_pex {
            m.insert(Bytes::from_static(b"ut_pex"), Value::Integer(id as i64));
        }
        dict.insert(Bytes::from_static(b"m"), Value::Dict(m));

        if let Some(size) = self.metadata_size {
            dict.insert(
                Bytes::from_static(b"metadata_size"),
                Value::Integer(size as i64),
            );
        }

        if let Some(ref client) = self.client {
            dict.insert(Bytes::from_static(b"v"), Value::string(client));
        }

        if let Some(ref ip) = self.yourip {
            dict.insert(Bytes::from_static(b"yourip"), Value::bytes(ip.clone()));
        }

        if let Some(reqq) = self.reqq {
            dict.insert(Bytes::from_static(b"reqq"), Value::Integer(reqq as i64));
        }

        if let Some(port) = self.listen_port {
            dict.insert(Bytes::from_static(b"p"), Value::Integer(port as i64));
        }

        let encoded = encode(&Value::Dict(dict)).unwrap_or_default();
        Bytes::from(encoded)
    }

    pub fn parse(data: &[u8]) -> Option<Self> {
        let value = decode(data).ok()?;
        let dict = value.as_dict()?;

        let m = dict.get(b"m".as_slice()).and_then(|v| v.as_dict());

        let ut_metadata = m
            .and_then(|m| m.get(b"ut_metadata".as_slice()))
            .and_then(|v| v.as_integer())
            .map(|v| v as u8)
            .filter(|&id| id != 0);

        let ut_pex = m
            .and_then(|m| m.get(b"ut_pex".as_slice()))
            .and_then(|v| v.as_integer())
            .map(|v| v as u8)
            .filter(|&id| id != 0);

        let metadata_size = dict
            .get(b"metadata_size".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| v as u32);

        let client = dict
            .get(b"v".as_slice())
            .and_then(|v| v.as_str())
            .map(String::from);

        let yourip = dict
            .get(b"yourip".as_slice())
            .and_then(|v| v.as_bytes())
            .map(|b| b.to_vec());

        let ipv6 = dict
            .get(b"ipv6".as_slice())
            .and_then(|v| v.as_bytes())
            .map(|b| b.to_vec());

        let ipv4 = dict
            .get(b"ipv4".as_slice())
            .and_then(|v| v.as_bytes())
            .map(|b| b.to_vec());

        let reqq = dict
            .get(b"reqq".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| v as u32);

        let listen_port = dict
            .get(b"p".as_slice())
            .and_then(|v| v.as_integer())
            .map(|v| v as u16);

        Some(Self {
            metadata_size,
            ut_metadata,
            ut_pex,
            client,
            yourip,
            ipv6,
            ipv4,
            reqq,
            listen_port,
        })
    }
}

impl Default for ExtensionHandshake {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum MetadataMessage {
    Request {
        piece: u32,
    },
    Data {
        piece: u32,
        total_size: u32,
        data: Bytes,
    },
    Reject {
        piece: u32,
    },
}

impl MetadataMessage {
    pub fn encode(&self) -> Bytes {
        let mut dict = BTreeMap::new();

        match self {
            MetadataMessage::Request { piece } => {
                dict.insert(
                    Bytes::from_static(b"msg_type"),
                    Value::Integer(MetadataMessageType::Request as i64),
                );
                dict.insert(Bytes::from_static(b"piece"), Value::Integer(*piece as i64));

                let encoded = encode(&Value::Dict(dict)).unwrap_or_default();
                Bytes::from(encoded)
            }
            MetadataMessage::Data {
                piece,
                total_size,
                data,
            } => {
                dict.insert(
                    Bytes::from_static(b"msg_type"),
                    Value::Integer(MetadataMessageType::Data as i64),
                );
                dict.insert(Bytes::from_static(b"piece"), Value::Integer(*piece as i64));
                dict.insert(
                    Bytes::from_static(b"total_size"),
                    Value::Integer(*total_size as i64),
                );

                let encoded = encode(&Value::Dict(dict)).unwrap_or_default();
                let mut buf = BytesMut::with_capacity(encoded.len() + data.len());
                buf.put_slice(&encoded);
                buf.put_slice(data);
                buf.freeze()
            }
            MetadataMessage::Reject { piece } => {
                dict.insert(
                    Bytes::from_static(b"msg_type"),
                    Value::Integer(MetadataMessageType::Reject as i64),
                );
                dict.insert(Bytes::from_static(b"piece"), Value::Integer(*piece as i64));

                let encoded = encode(&Value::Dict(dict)).unwrap_or_default();
                Bytes::from(encoded)
            }
        }
    }

    pub fn parse(data: &[u8]) -> Option<Self> {
        let dict_end = find_dict_end(data)?;
        let dict_data = &data[..dict_end];
        let payload = &data[dict_end..];

        let value = decode(dict_data).ok()?;
        let dict = value.as_dict()?;

        let msg_type = dict
            .get(b"msg_type".as_slice())
            .and_then(|v| v.as_integer())
            .and_then(|v| MetadataMessageType::try_from(v as u8).ok())?;

        let piece = dict.get(b"piece".as_slice()).and_then(|v| v.as_integer())? as u32;

        match msg_type {
            MetadataMessageType::Request => Some(MetadataMessage::Request { piece }),
            MetadataMessageType::Data => {
                let total_size = dict
                    .get(b"total_size".as_slice())
                    .and_then(|v| v.as_integer())? as u32;
                Some(MetadataMessage::Data {
                    piece,
                    total_size,
                    data: Bytes::copy_from_slice(payload),
                })
            }
            MetadataMessageType::Reject => Some(MetadataMessage::Reject { piece }),
        }
    }
}

fn find_dict_end(data: &[u8]) -> Option<usize> {
    if data.is_empty() || data[0] != b'd' {
        return None;
    }

    let mut depth = 0;
    let mut i = 0;

    while i < data.len() {
        match data[i] {
            b'd' | b'l' => depth += 1,
            b'e' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            b'i' => {
                i += 1;
                while i < data.len() && data[i] != b'e' {
                    i += 1;
                }
            }
            b'0'..=b'9' => {
                let start = i;
                while i < data.len() && data[i].is_ascii_digit() {
                    i += 1;
                }
                if i >= data.len() || data[i] != b':' {
                    return None;
                }
                let len_str = std::str::from_utf8(&data[start..i]).ok()?;
                let len: usize = len_str.parse().ok()?;
                i += 1 + len;
                continue;
            }
            _ => {}
        }
        i += 1;
    }

    None
}

#[derive(Debug, Clone)]
pub struct PexMessage {
    pub added: Vec<PexPeer>,
    pub added_flags: Vec<u8>,
    pub dropped: Vec<PexPeer>,
    pub added6: Vec<PexPeer>,
    pub added6_flags: Vec<u8>,
    pub dropped6: Vec<PexPeer>,
}

#[derive(Debug, Clone)]
pub struct PexPeer {
    pub ip: std::net::IpAddr,
    pub port: u16,
}

impl PexMessage {
    pub fn new() -> Self {
        Self {
            added: Vec::new(),
            added_flags: Vec::new(),
            dropped: Vec::new(),
            added6: Vec::new(),
            added6_flags: Vec::new(),
            dropped6: Vec::new(),
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut dict = BTreeMap::new();

        let mut added_bytes = Vec::new();
        for peer in &self.added {
            if let std::net::IpAddr::V4(ip) = peer.ip {
                added_bytes.extend_from_slice(&ip.octets());
                added_bytes.extend_from_slice(&peer.port.to_be_bytes());
            }
        }
        dict.insert(Bytes::from_static(b"added"), Value::bytes(added_bytes));

        if !self.added_flags.is_empty() {
            dict.insert(
                Bytes::from_static(b"added.f"),
                Value::bytes(self.added_flags.clone()),
            );
        }

        let mut dropped_bytes = Vec::new();
        for peer in &self.dropped {
            if let std::net::IpAddr::V4(ip) = peer.ip {
                dropped_bytes.extend_from_slice(&ip.octets());
                dropped_bytes.extend_from_slice(&peer.port.to_be_bytes());
            }
        }
        dict.insert(Bytes::from_static(b"dropped"), Value::bytes(dropped_bytes));

        let mut added6_bytes = Vec::new();
        for peer in &self.added6 {
            if let std::net::IpAddr::V6(ip) = peer.ip {
                added6_bytes.extend_from_slice(&ip.octets());
                added6_bytes.extend_from_slice(&peer.port.to_be_bytes());
            }
        }
        dict.insert(Bytes::from_static(b"added6"), Value::bytes(added6_bytes));

        if !self.added6_flags.is_empty() {
            dict.insert(
                Bytes::from_static(b"added6.f"),
                Value::bytes(self.added6_flags.clone()),
            );
        }

        let mut dropped6_bytes = Vec::new();
        for peer in &self.dropped6 {
            if let std::net::IpAddr::V6(ip) = peer.ip {
                dropped6_bytes.extend_from_slice(&ip.octets());
                dropped6_bytes.extend_from_slice(&peer.port.to_be_bytes());
            }
        }
        dict.insert(
            Bytes::from_static(b"dropped6"),
            Value::bytes(dropped6_bytes),
        );

        let encoded = encode(&Value::Dict(dict)).unwrap_or_default();
        Bytes::from(encoded)
    }

    pub fn parse(data: &[u8]) -> Option<Self> {
        let value = decode(data).ok()?;
        let dict = value.as_dict()?;

        let added = parse_compact_peers(
            dict.get(b"added".as_slice())
                .and_then(|v| v.as_bytes())
                .map(|b| b.as_ref())
                .unwrap_or(&[]),
            false,
        );

        let added_flags = dict
            .get(b"added.f".as_slice())
            .and_then(|v| v.as_bytes())
            .map(|b| b.to_vec())
            .unwrap_or_default();

        let dropped = parse_compact_peers(
            dict.get(b"dropped".as_slice())
                .and_then(|v| v.as_bytes())
                .map(|b| b.as_ref())
                .unwrap_or(&[]),
            false,
        );

        let added6 = parse_compact_peers(
            dict.get(b"added6".as_slice())
                .and_then(|v| v.as_bytes())
                .map(|b| b.as_ref())
                .unwrap_or(&[]),
            true,
        );

        let added6_flags = dict
            .get(b"added6.f".as_slice())
            .and_then(|v| v.as_bytes())
            .map(|b| b.to_vec())
            .unwrap_or_default();

        let dropped6 = parse_compact_peers(
            dict.get(b"dropped6".as_slice())
                .and_then(|v| v.as_bytes())
                .map(|b| b.as_ref())
                .unwrap_or(&[]),
            true,
        );

        Some(Self {
            added,
            added_flags,
            dropped,
            added6,
            added6_flags,
            dropped6,
        })
    }
}

impl Default for PexMessage {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_compact_peers(data: &[u8], ipv6: bool) -> Vec<PexPeer> {
    let mut peers = Vec::new();
    let peer_size = if ipv6 { 18 } else { 6 };

    for chunk in data.chunks_exact(peer_size) {
        let ip = if ipv6 {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&chunk[..16]);
            std::net::IpAddr::V6(std::net::Ipv6Addr::from(octets))
        } else {
            let mut octets = [0u8; 4];
            octets.copy_from_slice(&chunk[..4]);
            std::net::IpAddr::V4(std::net::Ipv4Addr::from(octets))
        };

        let port_offset = if ipv6 { 16 } else { 4 };
        let port = u16::from_be_bytes([chunk[port_offset], chunk[port_offset + 1]]);

        peers.push(PexPeer { ip, port });
    }

    peers
}
