use oxidebt_constants::{
    CONNECTION_TIMEOUT, METADATA_FETCH_TIMEOUT, METADATA_PIECE_SIZE, METADATA_READ_TIMEOUT,
};
use oxidebt_peer::{
    ExtensionHandshake, ExtensionMessage, Handshake, Message, MetadataMessage, PeerId,
};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Fetches metadata from a list of peers for a magnet link.
/// Tries up to 10 peers before giving up.
pub async fn fetch_metadata_from_peers(
    peers: Vec<SocketAddr>,
    info_hash: [u8; 20],
    our_peer_id: PeerId,
) -> Option<Vec<u8>> {
    for peer_addr in peers.into_iter().take(10) {
        if let Some(metadata) =
            try_fetch_metadata_from_peer(peer_addr, info_hash, our_peer_id).await
        {
            return Some(metadata);
        }
    }
    None
}

async fn try_fetch_metadata_from_peer(
    peer_addr: SocketAddr,
    info_hash: [u8; 20],
    our_peer_id: PeerId,
) -> Option<Vec<u8>> {
    let stream = timeout(CONNECTION_TIMEOUT, TcpStream::connect(peer_addr))
        .await
        .ok()?
        .ok()?;

    fetch_metadata_from_stream(stream, peer_addr, info_hash, our_peer_id).await
}

async fn fetch_metadata_from_stream(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    info_hash: [u8; 20],
    our_peer_id: PeerId,
) -> Option<Vec<u8>> {
    let handshake = Handshake::new(info_hash, *our_peer_id.as_bytes());
    let handshake_bytes = handshake.encode();

    timeout(CONNECTION_TIMEOUT, stream.write_all(&handshake_bytes))
        .await
        .ok()?
        .ok()?;

    let mut buf = [0u8; 68];
    timeout(CONNECTION_TIMEOUT, stream.read_exact(&mut buf))
        .await
        .ok()?
        .ok()?;

    let remote_handshake = Handshake::parse(&buf).ok()?;

    if remote_handshake.info_hash != info_hash {
        return None;
    }

    if !remote_handshake.supports_extensions() {
        tracing::debug!("Peer {} does not support extensions", peer_addr);
        return None;
    }

    let ext_handshake = ExtensionHandshake::new();
    let ext_handshake_bytes = ext_handshake.encode();

    let ext_msg = Message::Extended(ExtensionMessage::Handshake(ext_handshake_bytes));
    let msg_bytes = ext_msg.encode_with_length();

    timeout(CONNECTION_TIMEOUT, stream.write_all(&msg_bytes))
        .await
        .ok()?
        .ok()?;

    let remote_ext = read_extension_handshake(&mut stream).await?;

    let metadata_size = remote_ext.metadata_size?;
    let ut_metadata_id = remote_ext.ut_metadata?;

    if metadata_size == 0 {
        return None;
    }

    let num_pieces = (metadata_size as usize).div_ceil(METADATA_PIECE_SIZE);
    let mut metadata = vec![0u8; metadata_size as usize];
    let mut received_pieces = vec![false; num_pieces];

    for piece in 0..num_pieces as u32 {
        let request = MetadataMessage::Request { piece };
        let request_bytes = request.encode();

        let mut msg_payload = bytes::BytesMut::new();
        msg_payload.extend_from_slice(&[ut_metadata_id]);
        msg_payload.extend_from_slice(&request_bytes);

        let length = msg_payload.len() as u32 + 1;
        let mut full_msg = bytes::BytesMut::new();
        full_msg.extend_from_slice(&length.to_be_bytes());
        full_msg.extend_from_slice(&[20]);
        full_msg.extend_from_slice(&msg_payload);

        timeout(CONNECTION_TIMEOUT, stream.write_all(&full_msg))
            .await
            .ok()?
            .ok()?;
    }

    let start = Instant::now();
    while !received_pieces.iter().all(|&r| r) {
        if start.elapsed() > METADATA_FETCH_TIMEOUT {
            tracing::debug!("Metadata fetch timeout for {}", peer_addr);
            return None;
        }

        let mut len_buf = [0u8; 4];
        if timeout(METADATA_READ_TIMEOUT, stream.read_exact(&mut len_buf))
            .await
            .ok()?
            .is_err()
        {
            return None;
        }

        let length = u32::from_be_bytes(len_buf) as usize;
        if length == 0 {
            continue;
        }

        if length > 1024 * 1024 {
            return None;
        }

        let mut msg_buf = vec![0u8; length];
        if timeout(METADATA_READ_TIMEOUT, stream.read_exact(&mut msg_buf))
            .await
            .ok()?
            .is_err()
        {
            return None;
        }

        if msg_buf.is_empty() {
            continue;
        }

        let msg_id = msg_buf[0];
        if msg_id != 20 {
            continue;
        }

        if msg_buf.len() < 2 {
            continue;
        }

        let ext_id = msg_buf[1];
        if ext_id == 0 {
            continue;
        }

        let payload = &msg_buf[2..];
        if let Some(MetadataMessage::Data {
            piece,
            total_size: _,
            data,
        }) = MetadataMessage::parse(payload)
        {
            let piece_idx = piece as usize;
            if piece_idx < num_pieces && !received_pieces[piece_idx] {
                let start_offset = piece_idx * METADATA_PIECE_SIZE;
                let end_offset = (start_offset + data.len()).min(metadata_size as usize);
                let copy_len = end_offset - start_offset;

                if start_offset + copy_len <= metadata.len() && copy_len <= data.len() {
                    metadata[start_offset..start_offset + copy_len]
                        .copy_from_slice(&data[..copy_len]);
                    received_pieces[piece_idx] = true;
                    tracing::debug!(
                        "Received metadata piece {}/{} from {}",
                        piece_idx + 1,
                        num_pieces,
                        peer_addr
                    );
                }
            }
        }
    }

    Some(metadata)
}

async fn read_extension_handshake(stream: &mut TcpStream) -> Option<ExtensionHandshake> {
    let timeout_duration = Duration::from_secs(10);
    const MAX_MESSAGES: usize = 100;
    let mut message_count = 0;

    loop {
        message_count += 1;
        if message_count > MAX_MESSAGES {
            tracing::debug!(
                "read_extension_handshake: exceeded max message count without handshake"
            );
            return None;
        }

        let mut len_buf = [0u8; 4];
        if timeout(timeout_duration, stream.read_exact(&mut len_buf))
            .await
            .ok()?
            .is_err()
        {
            return None;
        }

        let length = u32::from_be_bytes(len_buf) as usize;
        if length == 0 {
            continue;
        }

        if length > 1024 * 1024 {
            return None;
        }

        let mut msg_buf = vec![0u8; length];
        if timeout(timeout_duration, stream.read_exact(&mut msg_buf))
            .await
            .ok()?
            .is_err()
        {
            return None;
        }

        if msg_buf.is_empty() {
            continue;
        }

        let msg_id = msg_buf[0];
        if msg_id != 20 {
            continue;
        }

        if msg_buf.len() < 2 {
            continue;
        }

        let ext_id = msg_buf[1];
        if ext_id == 0 {
            let payload = &msg_buf[2..];
            return ExtensionHandshake::parse(payload);
        }
    }
}
