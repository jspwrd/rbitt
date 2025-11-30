use crate::error::PeerError;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use oxidebt_constants::{DHT_BIT, EXTENSION_BIT, FAST_EXTENSION_BIT, RESERVED_BYTES};

pub const PROTOCOL_STRING: &[u8; 19] = b"BitTorrent protocol";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageId {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8,
    Port = 9,
    SuggestPiece = 13,
    HaveAll = 14,
    HaveNone = 15,
    RejectRequest = 16,
    AllowedFast = 17,
    Extended = 20,
    HashRequest = 21,
    Hashes = 22,
    HashReject = 23,
}

impl TryFrom<u8> for MessageId {
    type Error = PeerError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MessageId::Choke),
            1 => Ok(MessageId::Unchoke),
            2 => Ok(MessageId::Interested),
            3 => Ok(MessageId::NotInterested),
            4 => Ok(MessageId::Have),
            5 => Ok(MessageId::Bitfield),
            6 => Ok(MessageId::Request),
            7 => Ok(MessageId::Piece),
            8 => Ok(MessageId::Cancel),
            9 => Ok(MessageId::Port),
            13 => Ok(MessageId::SuggestPiece),
            14 => Ok(MessageId::HaveAll),
            15 => Ok(MessageId::HaveNone),
            16 => Ok(MessageId::RejectRequest),
            17 => Ok(MessageId::AllowedFast),
            20 => Ok(MessageId::Extended),
            21 => Ok(MessageId::HashRequest),
            22 => Ok(MessageId::Hashes),
            23 => Ok(MessageId::HashReject),
            _ => Err(PeerError::InvalidMessage(format!(
                "unknown message id: {}",
                value
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have { piece_index: u32 },
    Bitfield(Bytes),
    Request { index: u32, begin: u32, length: u32 },
    Piece { index: u32, begin: u32, data: Bytes },
    Cancel { index: u32, begin: u32, length: u32 },
    Port(u16),
    SuggestPiece { piece_index: u32 },
    HaveAll,
    HaveNone,
    RejectRequest { index: u32, begin: u32, length: u32 },
    AllowedFast { piece_index: u32 },
    Extended(ExtensionMessage),
}

impl Message {
    pub fn parse(data: &[u8]) -> Result<Self, PeerError> {
        if data.is_empty() {
            return Ok(Message::KeepAlive);
        }

        let id = MessageId::try_from(data[0])?;
        let payload = &data[1..];

        match id {
            MessageId::Choke => Ok(Message::Choke),
            MessageId::Unchoke => Ok(Message::Unchoke),
            MessageId::Interested => Ok(Message::Interested),
            MessageId::NotInterested => Ok(Message::NotInterested),
            MessageId::Have => {
                if payload.len() != 4 {
                    return Err(PeerError::InvalidMessage("have message wrong size".into()));
                }
                let piece_index =
                    u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                Ok(Message::Have { piece_index })
            }
            MessageId::Bitfield => Ok(Message::Bitfield(Bytes::copy_from_slice(payload))),
            MessageId::Request => {
                if payload.len() != 12 {
                    return Err(PeerError::InvalidMessage(
                        "request message wrong size".into(),
                    ));
                }
                let mut buf = payload;
                let index = buf.get_u32();
                let begin = buf.get_u32();
                let length = buf.get_u32();
                Ok(Message::Request {
                    index,
                    begin,
                    length,
                })
            }
            MessageId::Piece => {
                if payload.len() < 8 {
                    return Err(PeerError::InvalidMessage("piece message too small".into()));
                }
                let mut buf = &payload[..8];
                let index = buf.get_u32();
                let begin = buf.get_u32();
                let data = Bytes::copy_from_slice(&payload[8..]);
                Ok(Message::Piece { index, begin, data })
            }
            MessageId::Cancel => {
                if payload.len() != 12 {
                    return Err(PeerError::InvalidMessage(
                        "cancel message wrong size".into(),
                    ));
                }
                let mut buf = payload;
                let index = buf.get_u32();
                let begin = buf.get_u32();
                let length = buf.get_u32();
                Ok(Message::Cancel {
                    index,
                    begin,
                    length,
                })
            }
            MessageId::Port => {
                if payload.len() != 2 {
                    return Err(PeerError::InvalidMessage("port message wrong size".into()));
                }
                let port = u16::from_be_bytes([payload[0], payload[1]]);
                Ok(Message::Port(port))
            }
            MessageId::SuggestPiece => {
                if payload.len() != 4 {
                    return Err(PeerError::InvalidMessage(
                        "suggest piece message wrong size".into(),
                    ));
                }
                let piece_index =
                    u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                Ok(Message::SuggestPiece { piece_index })
            }
            MessageId::HaveAll => Ok(Message::HaveAll),
            MessageId::HaveNone => Ok(Message::HaveNone),
            MessageId::RejectRequest => {
                if payload.len() != 12 {
                    return Err(PeerError::InvalidMessage(
                        "reject request message wrong size".into(),
                    ));
                }
                let mut buf = payload;
                let index = buf.get_u32();
                let begin = buf.get_u32();
                let length = buf.get_u32();
                Ok(Message::RejectRequest {
                    index,
                    begin,
                    length,
                })
            }
            MessageId::AllowedFast => {
                if payload.len() != 4 {
                    return Err(PeerError::InvalidMessage(
                        "allowed fast message wrong size".into(),
                    ));
                }
                let piece_index =
                    u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                Ok(Message::AllowedFast { piece_index })
            }
            MessageId::Extended => {
                let ext = ExtensionMessage::parse(payload)?;
                Ok(Message::Extended(ext))
            }
            _ => Err(PeerError::InvalidMessage(format!(
                "unhandled message type: {:?}",
                id
            ))),
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();

        match self {
            Message::KeepAlive => {}
            Message::Choke => buf.put_u8(MessageId::Choke as u8),
            Message::Unchoke => buf.put_u8(MessageId::Unchoke as u8),
            Message::Interested => buf.put_u8(MessageId::Interested as u8),
            Message::NotInterested => buf.put_u8(MessageId::NotInterested as u8),
            Message::Have { piece_index } => {
                buf.put_u8(MessageId::Have as u8);
                buf.put_u32(*piece_index);
            }
            Message::Bitfield(bits) => {
                buf.put_u8(MessageId::Bitfield as u8);
                buf.put_slice(bits);
            }
            Message::Request {
                index,
                begin,
                length,
            } => {
                buf.put_u8(MessageId::Request as u8);
                buf.put_u32(*index);
                buf.put_u32(*begin);
                buf.put_u32(*length);
            }
            Message::Piece { index, begin, data } => {
                buf.put_u8(MessageId::Piece as u8);
                buf.put_u32(*index);
                buf.put_u32(*begin);
                buf.put_slice(data);
            }
            Message::Cancel {
                index,
                begin,
                length,
            } => {
                buf.put_u8(MessageId::Cancel as u8);
                buf.put_u32(*index);
                buf.put_u32(*begin);
                buf.put_u32(*length);
            }
            Message::Port(port) => {
                buf.put_u8(MessageId::Port as u8);
                buf.put_u16(*port);
            }
            Message::SuggestPiece { piece_index } => {
                buf.put_u8(MessageId::SuggestPiece as u8);
                buf.put_u32(*piece_index);
            }
            Message::HaveAll => buf.put_u8(MessageId::HaveAll as u8),
            Message::HaveNone => buf.put_u8(MessageId::HaveNone as u8),
            Message::RejectRequest {
                index,
                begin,
                length,
            } => {
                buf.put_u8(MessageId::RejectRequest as u8);
                buf.put_u32(*index);
                buf.put_u32(*begin);
                buf.put_u32(*length);
            }
            Message::AllowedFast { piece_index } => {
                buf.put_u8(MessageId::AllowedFast as u8);
                buf.put_u32(*piece_index);
            }
            Message::Extended(ext) => {
                buf.put_u8(MessageId::Extended as u8);
                buf.put_slice(&ext.encode());
            }
        }

        buf.freeze()
    }

    pub fn encode_with_length(&self) -> Bytes {
        let payload = self.encode();
        let mut buf = BytesMut::with_capacity(4 + payload.len());
        buf.put_u32(payload.len() as u32);
        buf.put_slice(&payload);
        buf.freeze()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionMessage {
    Handshake(Bytes),
    PeerExchange(Bytes),
    Metadata {
        msg_type: u8,
        piece: u32,
        data: Bytes,
    },
    Unknown {
        id: u8,
        data: Bytes,
    },
}

impl ExtensionMessage {
    fn parse(data: &[u8]) -> Result<Self, PeerError> {
        if data.is_empty() {
            return Err(PeerError::InvalidMessage("empty extension message".into()));
        }

        let ext_id = data[0];
        let payload = Bytes::copy_from_slice(&data[1..]);

        match ext_id {
            0 => Ok(ExtensionMessage::Handshake(payload)),
            _ => Ok(ExtensionMessage::Unknown {
                id: ext_id,
                data: payload,
            }),
        }
    }

    fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();

        match self {
            ExtensionMessage::Handshake(data) => {
                buf.put_u8(0);
                buf.put_slice(data);
            }
            ExtensionMessage::PeerExchange(data) => {
                buf.put_u8(1);
                buf.put_slice(data);
            }
            ExtensionMessage::Metadata {
                msg_type,
                piece,
                data,
            } => {
                buf.put_u8(2);
                buf.put_u8(*msg_type);
                buf.put_u32(*piece);
                buf.put_slice(data);
            }
            ExtensionMessage::Unknown { id, data } => {
                buf.put_u8(*id);
                buf.put_slice(data);
            }
        }

        buf.freeze()
    }
}

#[derive(Debug, Clone)]
pub struct Handshake {
    pub reserved: [u8; 8],
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
}

impl Handshake {
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Self {
        let mut reserved = RESERVED_BYTES;
        reserved[5] |= EXTENSION_BIT;
        reserved[7] |= DHT_BIT;
        reserved[7] |= FAST_EXTENSION_BIT;

        Self {
            reserved,
            info_hash,
            peer_id,
        }
    }

    pub fn parse(data: &[u8]) -> Result<Self, PeerError> {
        if data.len() != 68 {
            return Err(PeerError::InvalidHandshake(format!(
                "wrong length: {}",
                data.len()
            )));
        }

        let pstrlen = data[0];
        if pstrlen != 19 {
            return Err(PeerError::InvalidHandshake(format!(
                "wrong pstrlen: {}",
                pstrlen
            )));
        }

        if &data[1..20] != PROTOCOL_STRING {
            return Err(PeerError::ProtocolMismatch);
        }

        let mut reserved = [0u8; 8];
        reserved.copy_from_slice(&data[20..28]);

        let mut info_hash = [0u8; 20];
        info_hash.copy_from_slice(&data[28..48]);

        let mut peer_id = [0u8; 20];
        peer_id.copy_from_slice(&data[48..68]);

        Ok(Self {
            reserved,
            info_hash,
            peer_id,
        })
    }

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(68);
        buf.put_u8(19);
        buf.put_slice(PROTOCOL_STRING);
        buf.put_slice(&self.reserved);
        buf.put_slice(&self.info_hash);
        buf.put_slice(&self.peer_id);
        buf.freeze()
    }

    pub fn supports_extensions(&self) -> bool {
        self.reserved[5] & EXTENSION_BIT != 0
    }

    pub fn supports_dht(&self) -> bool {
        self.reserved[7] & DHT_BIT != 0
    }

    #[allow(dead_code)]
    pub fn supports_fast(&self) -> bool {
        self.reserved[7] & FAST_EXTENSION_BIT != 0
    }
}
