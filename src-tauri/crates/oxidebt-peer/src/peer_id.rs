use oxidebt_constants::CLIENT_PREFIX;
use rand::RngExt;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerId(pub [u8; 20]);

impl PeerId {
    pub fn generate() -> Self {
        let mut id = [0u8; 20];
        id[..8].copy_from_slice(CLIENT_PREFIX.as_bytes());

        let mut rng = rand::rng();
        for byte in &mut id[8..] {
            *byte = rng.random();
        }

        Self(id)
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 20 {
            return None;
        }
        let mut id = [0u8; 20];
        id.copy_from_slice(bytes);
        Some(Self(id))
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    pub fn client_name(&self) -> Option<String> {
        if self.0[0] == b'-' && self.0[7] == b'-' {
            let client_code = &self.0[1..3];
            let version = &self.0[3..7];

            let client = match client_code {
                b"OX" => "OxideBT",
                b"qB" => "qBittorrent",
                b"TR" => "Transmission",
                b"UT" => "uTorrent",
                b"lt" => "libtorrent",
                b"DE" => "Deluge",
                b"AZ" => "Vuze",
                b"BC" => "BitComet",
                _ => return None,
            };

            Some(format!(
                "{} {}.{}.{}.{}",
                client,
                version[0] as char,
                version[1] as char,
                version[2] as char,
                version[3] as char
            ))
        } else {
            None
        }
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.client_name() {
            write!(f, "PeerId({}, {:?})", name, hex::encode(self.0))
        } else {
            write!(f, "PeerId({:?})", hex::encode(self.0))
        }
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}
