use bytes::Bytes;
use oxidebt_constants::{
    KEEPALIVE_INTERVAL, MAX_PARALLEL_PIECES, MAX_REQUESTS_PER_PEER, PEX_INITIAL_DELAY,
    PEX_MAX_IPV4_PEERS, PEX_SEND_INTERVAL,
};
use oxidebt_disk::DiskManager;
use oxidebt_net::BandwidthLimiter;
use oxidebt_peer::{
    Bitfield, Block, ExtensionHandshake, ExtensionMessage, Handshake, Message, MetadataMessage,
    PeerConnection, PeerId, PexMessage, PexPeer,
};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::error::EngineError;
use super::events::PeerEvent;
use super::pex::{is_shareable_pex_addr, parse_pex_peers};
use super::torrent::ManagedTorrent;
use oxidebt_constants::HANDSHAKE_TIMEOUT;

/// Handles an incoming TCP connection from a peer.
#[allow(clippy::too_many_arguments)]
pub async fn handle_incoming_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    our_peer_id: PeerId,
    torrents: Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    disk_manager: Arc<DiskManager>,
    bandwidth_limiter: Arc<RwLock<BandwidthLimiter>>,
    event_tx: mpsc::UnboundedSender<PeerEvent>,
    listen_port: u16,
) -> Result<(), EngineError> {
    let mut buf = [0u8; 68];
    timeout(HANDSHAKE_TIMEOUT, stream.read_exact(&mut buf))
        .await
        .map_err(|_| {
            EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Handshake timeout",
            ))
        })?
        .map_err(EngineError::Io)?;

    let remote_handshake = Handshake::parse(&buf)?;
    let info_hash = remote_handshake.info_hash;

    let (hash, piece_count) = {
        let torrents_guard = torrents.read();
        let mut found = None;
        for (hash, torrent) in torrents_guard.iter() {
            if let Some(our_hash) = torrent.info_hash_bytes() {
                if our_hash == info_hash {
                    found = Some((hash.clone(), torrent.meta.piece_count()));
                    break;
                }
            }
        }
        match found {
            Some(f) => f,
            None => {
                tracing::debug!("Incoming connection for unknown torrent");
                return Ok(());
            }
        }
    };

    let our_handshake = Handshake::new(info_hash, *our_peer_id.as_bytes());
    stream
        .write_all(&our_handshake.encode())
        .await
        .map_err(EngineError::Io)?;

    let conn = PeerConnection::from_accepted(stream, addr, info_hash, our_peer_id, piece_count);

    handle_peer_connection(
        hash,
        addr,
        conn,
        torrents,
        disk_manager,
        bandwidth_limiter,
        event_tx,
        listen_port,
    )
    .await
}

/// Handles a peer connection after handshake is complete.
#[allow(clippy::too_many_arguments)]
pub async fn handle_peer_connection(
    hash: String,
    peer_addr: SocketAddr,
    mut conn: PeerConnection,
    torrents: Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    disk_manager: Arc<DiskManager>,
    bandwidth_limiter: Arc<RwLock<BandwidthLimiter>>,
    event_tx: mpsc::UnboundedSender<PeerEvent>,
    listen_port: u16,
) -> Result<(), EngineError> {
    if conn.state() != oxidebt_peer::PeerState::Connected {
        conn.handshake().await?;
    }

    if conn.supports_extensions() {
        let ext_handshake = ExtensionHandshake::new().with_listen_port(listen_port);
        let ext_msg = Message::Extended(ExtensionMessage::Handshake(ext_handshake.encode()));
        conn.send(ext_msg).await?;
    }

    let _ = event_tx.send(PeerEvent::Connected {
        torrent_hash: hash.clone(),
        peer_addr,
    });

    let (bitfield, piece_count) = {
        let torrents = torrents.read();
        torrents
            .get(&hash)
            .map(|t| (t.piece_manager.bitfield(), t.meta.piece_count()))
            .unwrap_or_else(|| (Bitfield::new(0), 0))
    };

    if piece_count > 0 {
        if conn.supports_fast() {
            if bitfield.is_complete() {
                conn.send(Message::HaveAll).await?;
            } else if bitfield.is_empty() {
                conn.send(Message::HaveNone).await?;
            } else {
                conn.send(Message::Bitfield(Bytes::from(bitfield.as_bytes().to_vec())))
                    .await?;
            }
        } else {
            conn.send(Message::Bitfield(Bytes::from(bitfield.as_bytes().to_vec())))
                .await?;
        }
    }

    if conn.supports_fast() {
        conn.send_allowed_fast_set().await?;
    }

    let (cancel_rx, shutdown_rx) = {
        let torrents = torrents.read();
        torrents
            .get(&hash)
            .map(|t| (t.cancel_tx.subscribe(), t.shutdown_tx.subscribe()))
            .unwrap_or_else(|| {
                let (tx1, rx1) = tokio::sync::broadcast::channel::<u32>(1);
                let (tx2, rx2) = tokio::sync::broadcast::channel::<()>(1);
                drop(tx1);
                drop(tx2);
                (rx1, rx2)
            })
    };

    let result = peer_message_loop(
        &hash,
        peer_addr,
        &mut conn,
        &torrents,
        &disk_manager,
        &bandwidth_limiter,
        &event_tx,
        cancel_rx,
        shutdown_rx,
        listen_port,
    )
    .await;

    let _ = event_tx.send(PeerEvent::Disconnected {
        torrent_hash: hash.clone(),
        peer_addr,
    });

    result
}

#[allow(clippy::too_many_arguments)]
async fn peer_message_loop(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    disk_manager: &Arc<DiskManager>,
    bandwidth_limiter: &Arc<RwLock<BandwidthLimiter>>,
    event_tx: &mpsc::UnboundedSender<PeerEvent>,
    mut cancel_rx: tokio::sync::broadcast::Receiver<u32>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    listen_port: u16,
) -> Result<(), EngineError> {
    let mut last_pex_time = Instant::now() - PEX_INITIAL_DELAY;
    let mut pex_sent_peers: HashSet<SocketAddr> = HashSet::new();
    let mut sent_initial_pex = false;
    let mut pending_requests: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                tracing::debug!("Peer {} received shutdown signal", peer_addr);
                return Ok(());
            }
            result = conn.receive() => {
                let message = result?;
                handle_message(
                    message,
                    hash,
                    peer_addr,
                    conn,
                    torrents,
                    disk_manager,
                    bandwidth_limiter,
                    event_tx,
                    &mut pending_requests,
                    &mut pex_sent_peers,
                    &mut sent_initial_pex,
                    &mut last_pex_time,
                    listen_port,
                ).await?;
            }
            _ = tokio::time::sleep(KEEPALIVE_INTERVAL / 4) => {
                conn.maybe_send_keepalive().await?;
                handle_periodic_tasks(
                    hash,
                    peer_addr,
                    conn,
                    torrents,
                    &mut last_pex_time,
                    &mut pex_sent_peers,
                    listen_port,
                ).await?;
            }
            result = cancel_rx.recv() => {
                if let Ok(piece_index) = result {
                    handle_cancel(conn, piece_index, &mut pending_requests, peer_addr).await?;
                }
            }
        }
    }
}

/// Handles a single message from a peer.
#[allow(clippy::too_many_arguments)]
async fn handle_message(
    message: Message,
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    disk_manager: &Arc<DiskManager>,
    bandwidth_limiter: &Arc<RwLock<BandwidthLimiter>>,
    event_tx: &mpsc::UnboundedSender<PeerEvent>,
    pending_requests: &mut HashMap<u32, Vec<(u32, u32)>>,
    pex_sent_peers: &mut HashSet<SocketAddr>,
    sent_initial_pex: &mut bool,
    last_pex_time: &mut Instant,
    listen_port: u16,
) -> Result<(), EngineError> {
    match message {
        Message::Unchoke => {
            let _ = event_tx.send(PeerEvent::PeerState {
                torrent_hash: hash.to_string(),
                peer_addr,
                is_choking_us: false,
                is_interested: conn.peer_interested(),
            });

            let we_need_pieces = {
                let torrents = torrents.read();
                torrents
                    .get(hash)
                    .map(|t| !t.piece_manager.is_complete())
                    .unwrap_or(false)
            };
            if we_need_pieces && !conn.am_interested() {
                conn.send(Message::Interested).await?;
                tracing::debug!(
                    "Sent Interested to peer {} (received Unchoke first)",
                    peer_addr
                );
            }

            request_pieces(hash, peer_addr, conn, torrents, pending_requests).await?;
        }
        Message::Choke => {
            let _ = event_tx.send(PeerEvent::PeerState {
                torrent_hash: hash.to_string(),
                peer_addr,
                is_choking_us: true,
                is_interested: conn.peer_interested(),
            });
        }
        Message::Interested => {
            let _ = event_tx.send(PeerEvent::PeerState {
                torrent_hash: hash.to_string(),
                peer_addr,
                is_choking_us: conn.peer_choking(),
                is_interested: true,
            });

            let should_unchoke = {
                let torrents = torrents.read();
                torrents
                    .get(hash)
                    .map(|t| t.unchoked_peers.contains(&peer_addr))
                    .unwrap_or(false)
            };

            if should_unchoke {
                conn.send(Message::Unchoke).await?;
            }
        }
        Message::NotInterested => {
            let _ = event_tx.send(PeerEvent::PeerState {
                torrent_hash: hash.to_string(),
                peer_addr,
                is_choking_us: conn.peer_choking(),
                is_interested: false,
            });
        }
        Message::Piece { index, begin, data } => {
            handle_piece_message(
                hash, peer_addr, conn, torrents, disk_manager, bandwidth_limiter, event_tx,
                pending_requests, index, begin, data,
            )
            .await?;
        }
        Message::Request { index, begin, length } => {
            handle_request_message(
                hash, peer_addr, conn, torrents, disk_manager, bandwidth_limiter, event_tx,
                index, begin, length,
            )
            .await?;
        }
        Message::Have { piece_index } => {
            {
                let torrents = torrents.read();
                if let Some(torrent) = torrents.get(hash) {
                    torrent
                        .piece_manager
                        .increment_piece_availability(piece_index as usize);
                }
            }

            let _ = event_tx.send(PeerEvent::PeerHave {
                torrent_hash: hash.to_string(),
                peer_addr,
                piece_index,
            });

            if !conn.peer_choking() {
                request_pieces(hash, peer_addr, conn, torrents, pending_requests).await?;
            }
        }
        Message::Bitfield(bits) => {
            handle_bitfield_message(
                hash, peer_addr, conn, torrents, event_tx, pending_requests, bits,
            )
            .await?;
        }
        Message::Extended(ext_msg) => {
            handle_extended_message(
                hash, peer_addr, conn, torrents, event_tx, pex_sent_peers, sent_initial_pex,
                last_pex_time, listen_port, ext_msg,
            )
            .await?;
        }
        Message::KeepAlive => {}
        Message::Cancel { .. } => {}
        Message::Port(port) => {
            tracing::debug!("Peer {} advertised DHT port {}", peer_addr, port);
        }
        Message::SuggestPiece { piece_index } => {
            conn.add_suggestion(piece_index);
            tracing::debug!("Peer {} suggested piece {}", peer_addr, piece_index);
        }
        Message::HaveAll => {
            handle_have_all(hash, peer_addr, conn, torrents, event_tx, pending_requests).await?;
        }
        Message::HaveNone => {
            handle_have_none(hash, peer_addr, torrents, event_tx).await?;
        }
        Message::RejectRequest { index, begin, length } => {
            tracing::debug!(
                "Peer {} rejected request for piece {} offset {} len {}",
                peer_addr,
                index,
                begin,
                length
            );
        }
        Message::AllowedFast { piece_index } => {
            conn.add_allowed_fast(piece_index);
            tracing::debug!(
                "Peer {} allows fast request for piece {}",
                peer_addr,
                piece_index
            );
            if conn.peer_choking() {
                let need_piece = {
                    let torrents = torrents.read();
                    torrents
                        .get(hash)
                        .map(|t| {
                            !t.piece_manager
                                .bitfield()
                                .has_piece(piece_index as usize)
                        })
                        .unwrap_or(false)
                };
                if need_piece {
                    request_pieces(hash, peer_addr, conn, torrents, pending_requests).await?;
                }
            }
        }
    }
    Ok(())
}

/// Handles a Piece message (block data received).
#[allow(clippy::too_many_arguments)]
async fn handle_piece_message(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    disk_manager: &Arc<DiskManager>,
    bandwidth_limiter: &Arc<RwLock<BandwidthLimiter>>,
    event_tx: &mpsc::UnboundedSender<PeerEvent>,
    pending_requests: &mut HashMap<u32, Vec<(u32, u32)>>,
    index: u32,
    begin: u32,
    data: Bytes,
) -> Result<(), EngineError> {
    let block_size = data.len();
    let recv_start = Instant::now();

    let download_limiter = bandwidth_limiter.read().download_limiter();
    let wait = download_limiter.acquire(data.len()).await;
    if !wait.is_zero() {
        tracing::debug!(
            "[BLOCK] {} rate limited for {:?} ({} bytes)",
            peer_addr, wait, block_size
        );
        tokio::time::sleep(wait).await;
    }

    let _ = event_tx.send(PeerEvent::BlockReceived {
        torrent_hash: hash.to_string(),
        size: data.len() as u64,
    });

    disk_manager
        .write_block(hash, index, begin, &data)
        .await?;

    let piece_complete = {
        let torrents = torrents.read();
        if let Some(torrent) = torrents.get(hash) {
            let block = Block {
                piece_index: index,
                offset: begin,
                data: Bytes::copy_from_slice(&data),
            };
            torrent.piece_manager.receive_block(block).unwrap_or(false)
        } else {
            false
        }
    };

    if piece_complete {
        let should_verify = {
            let torrents = torrents.read();
            torrents
                .get(hash)
                .map(|t| t.piece_manager.start_verifying(index))
                .unwrap_or(false)
        };

        if !should_verify {
            tracing::debug!("Piece {} already being verified by another peer", index);
        } else {
            let valid = disk_manager.verify_piece(hash, index).await?;
            if valid {
                {
                    let torrents = torrents.read();
                    if let Some(torrent) = torrents.get(hash) {
                        torrent.piece_manager.mark_piece_complete(index);
                        let _ = torrent.cancel_tx.send(index);
                    }
                }

                pending_requests.remove(&index);

                let _ = event_tx.send(PeerEvent::PieceCompleted {
                    torrent_hash: hash.to_string(),
                    piece_index: index,
                });

                conn.send(Message::Have { piece_index: index }).await?;

                let elapsed = recv_start.elapsed();
                tracing::info!(
                    "[PIECE] #{} complete ({} bytes in {:?}) from {}",
                    index, block_size, elapsed, peer_addr
                );
            } else {
                tracing::warn!("Piece {} failed verification, will re-download", index);
                let torrents = torrents.read();
                if let Some(torrent) = torrents.get(hash) {
                    torrent.piece_manager.mark_piece_failed(index);
                }
            }
        }
    }

    if !conn.peer_choking() {
        request_pieces(hash, peer_addr, conn, torrents, pending_requests).await?;
    }

    Ok(())
}

/// Handles a Request message (peer wants a block).
#[allow(clippy::too_many_arguments)]
async fn handle_request_message(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    disk_manager: &Arc<DiskManager>,
    bandwidth_limiter: &Arc<RwLock<BandwidthLimiter>>,
    event_tx: &mpsc::UnboundedSender<PeerEvent>,
    index: u32,
    begin: u32,
    length: u32,
) -> Result<(), EngineError> {
    let (have_piece, should_serve) = {
        let torrents = torrents.read();
        if let Some(torrent) = torrents.get(hash) {
            let have = torrent.piece_manager.bitfield().has_piece(index as usize);
            let unchoked = torrent.unchoked_peers.contains(&peer_addr);
            (have, have && unchoked)
        } else {
            (false, false)
        }
    };

    if should_serve {
        let upload_limiter = bandwidth_limiter.read().upload_limiter();
        let wait = upload_limiter.acquire(length as usize).await;
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }

        let data = disk_manager.read_block(hash, index, begin, length).await?;
        conn.send(Message::Piece { index, begin, data }).await?;

        tracing::debug!(
            "Served block to {}: piece {} offset {} len {} (total served this session)",
            peer_addr,
            index,
            begin,
            length
        );

        let _ = event_tx.send(PeerEvent::BlockSent {
            torrent_hash: hash.to_string(),
            size: length as u64,
        });
    } else if have_piece && !conn.am_choking() {
        tracing::debug!(
            "Rejecting request from {}: piece {} (have={}, in_unchoked={})",
            peer_addr,
            index,
            have_piece,
            should_serve
        );
        conn.send(Message::RejectRequest { index, begin, length })
            .await?;
    } else if !should_serve {
        tracing::debug!(
            "Not serving request from {}: piece {} (have={}, in_unchoked_peers={})",
            peer_addr,
            index,
            have_piece,
            {
                let torrents = torrents.read();
                torrents.get(hash).map(|t| t.unchoked_peers.contains(&peer_addr)).unwrap_or(false)
            }
        );
    }

    Ok(())
}

/// Handles a Bitfield message.
async fn handle_bitfield_message(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    event_tx: &mpsc::UnboundedSender<PeerEvent>,
    pending_requests: &mut HashMap<u32, Vec<(u32, u32)>>,
    bits: Bytes,
) -> Result<(), EngineError> {
    let piece_count = {
        let torrents = torrents.read();
        torrents.get(hash).map(|t| t.meta.piece_count()).unwrap_or(0)
    };

    match Bitfield::from_bytes(&bits, piece_count) {
        Ok(bf) => {
            let we_need_pieces = {
                let torrents = torrents.read();
                if let Some(torrent) = torrents.get(hash) {
                    let our_bf = torrent.piece_manager.bitfield();
                    (0..piece_count).any(|i| bf.has_piece(i) && !our_bf.has_piece(i))
                } else {
                    false
                }
            };

            let _ = event_tx.send(PeerEvent::PeerBitfield {
                torrent_hash: hash.to_string(),
                peer_addr,
                bitfield: bf,
            });

            if we_need_pieces && !conn.am_interested() {
                conn.send(Message::Interested).await?;
                tracing::debug!(
                    "Sent Interested to peer {} (they have pieces we need)",
                    peer_addr
                );
            }

            if !conn.peer_choking() {
                request_pieces(hash, peer_addr, conn, torrents, pending_requests).await?;
            }
        }
        Err(e) => {
            tracing::warn!("Failed to parse bitfield from peer {}: {}", peer_addr, e);
        }
    }

    Ok(())
}

/// Handles an Extended message.
#[allow(clippy::too_many_arguments)]
async fn handle_extended_message(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    event_tx: &mpsc::UnboundedSender<PeerEvent>,
    pex_sent_peers: &mut HashSet<SocketAddr>,
    sent_initial_pex: &mut bool,
    last_pex_time: &mut Instant,
    listen_port: u16,
    ext_msg: ExtensionMessage,
) -> Result<(), EngineError> {
    match ext_msg {
        ExtensionMessage::Handshake(data) => {
            if let Some(ext_hs) = ExtensionHandshake::parse(&data) {
                conn.set_peer_pex_id(ext_hs.ut_pex);
                conn.set_peer_metadata_id(ext_hs.ut_metadata);
                tracing::debug!(
                    "Peer {} supports PEX={:?}, ut_metadata={:?}",
                    peer_addr,
                    ext_hs.ut_pex,
                    ext_hs.ut_metadata
                );

                if !*sent_initial_pex {
                    if let Some(pex_id) = ext_hs.ut_pex {
                        send_initial_pex(
                            hash, peer_addr, conn, torrents, pex_sent_peers, listen_port, pex_id,
                        )
                        .await?;
                        *last_pex_time = Instant::now();
                    }
                    *sent_initial_pex = true;
                }
            }
        }
        ExtensionMessage::Unknown { id, data } => {
            if Some(id) == conn.peer_pex_id() {
                if let Some(peers) = parse_pex_peers(&data) {
                    tracing::debug!(
                        "Received PEX from {} with {} new peers",
                        peer_addr,
                        peers.len()
                    );
                    let _ = event_tx.send(PeerEvent::NewPeers {
                        torrent_hash: hash.to_string(),
                        peers,
                    });
                }
            }
        }
        ExtensionMessage::PeerExchange(pex_data) => {
            if let Some(peers) = parse_pex_peers(&pex_data) {
                let _ = event_tx.send(PeerEvent::NewPeers {
                    torrent_hash: hash.to_string(),
                    peers,
                });
            }
        }
        ExtensionMessage::Metadata { msg_type, piece, data: _ } => {
            // BEP-9: ut_metadata handling
            // msg_type 0 = request, 1 = data, 2 = reject
            if msg_type == 0 {
                // Peer is requesting metadata from us
                handle_metadata_request(hash, peer_addr, conn, torrents, piece).await?;
            }
            // msg_type 1 (data) and 2 (reject) are handled by the metadata fetcher
            // when we're downloading metadata for magnet links
        }
    }

    Ok(())
}

/// Handles a metadata request from a peer (BEP-9 ut_metadata).
/// Sends back the requested metadata piece or a reject message.
async fn handle_metadata_request(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    piece: u32,
) -> Result<(), EngineError> {
    const METADATA_PIECE_SIZE: usize = 16384;

    // Get the raw metadata and our ut_metadata ID
    let (raw_info, peer_metadata_id) = {
        let torrents = torrents.read();
        let torrent = match torrents.get(hash) {
            Some(t) => t,
            None => return Ok(()),
        };
        let raw = bytes::Bytes::copy_from_slice(torrent.meta.raw_info());
        (raw, conn.peer_metadata_id())
    };

    let Some(peer_ut_metadata_id) = peer_metadata_id else {
        tracing::debug!("Peer {} requested metadata but hasn't advertised ut_metadata support", peer_addr);
        return Ok(());
    };

    let metadata_size = raw_info.len();
    let num_pieces = metadata_size.div_ceil(METADATA_PIECE_SIZE);
    let piece_idx = piece as usize;

    if piece_idx >= num_pieces {
        // Invalid piece index - send reject
        tracing::debug!(
            "Peer {} requested invalid metadata piece {} (max {})",
            peer_addr,
            piece,
            num_pieces.saturating_sub(1)
        );
        let reject = MetadataMessage::Reject { piece };
        let reject_bytes = reject.encode();

        let mut msg_payload = bytes::BytesMut::new();
        msg_payload.extend_from_slice(&[peer_ut_metadata_id]);
        msg_payload.extend_from_slice(&reject_bytes);

        let ext_msg = ExtensionMessage::Unknown {
            id: peer_ut_metadata_id,
            data: msg_payload.freeze(),
        };
        conn.send(Message::Extended(ext_msg)).await?;
        return Ok(());
    }

    // Calculate piece bounds
    let start = piece_idx * METADATA_PIECE_SIZE;
    let end = (start + METADATA_PIECE_SIZE).min(metadata_size);
    let piece_data = bytes::Bytes::copy_from_slice(&raw_info[start..end]);

    // Send metadata piece
    let data_msg = MetadataMessage::Data {
        piece,
        total_size: metadata_size as u32,
        data: piece_data,
    };
    let data_bytes = data_msg.encode();

    let mut msg_payload = bytes::BytesMut::new();
    msg_payload.extend_from_slice(&[peer_ut_metadata_id]);
    msg_payload.extend_from_slice(&data_bytes);

    let ext_msg = ExtensionMessage::Unknown {
        id: peer_ut_metadata_id,
        data: msg_payload.freeze(),
    };
    conn.send(Message::Extended(ext_msg)).await?;

    tracing::debug!(
        "Served metadata piece {}/{} ({} bytes) to {}",
        piece_idx + 1,
        num_pieces,
        end - start,
        peer_addr
    );

    Ok(())
}

/// Handles a HaveAll message.
async fn handle_have_all(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    event_tx: &mpsc::UnboundedSender<PeerEvent>,
    pending_requests: &mut HashMap<u32, Vec<(u32, u32)>>,
) -> Result<(), EngineError> {
    let (piece_count, we_need_pieces) = {
        let torrents = torrents.read();
        if let Some(torrent) = torrents.get(hash) {
            let pc = torrent.meta.piece_count();
            let need = !torrent.piece_manager.is_complete();
            (pc, need)
        } else {
            (0, false)
        }
    };
    let mut bf = Bitfield::new(piece_count);
    for i in 0..piece_count {
        bf.set_piece(i);
    }
    let _ = event_tx.send(PeerEvent::PeerBitfield {
        torrent_hash: hash.to_string(),
        peer_addr,
        bitfield: bf,
    });

    if we_need_pieces && !conn.am_interested() {
        conn.send(Message::Interested).await?;
        tracing::debug!(
            "Sent Interested to peer {} (HaveAll - they're a seed)",
            peer_addr
        );
    }

    if !conn.peer_choking() {
        request_pieces(hash, peer_addr, conn, torrents, pending_requests).await?;
    }

    Ok(())
}

/// Handles a HaveNone message.
async fn handle_have_none(
    hash: &str,
    peer_addr: SocketAddr,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    event_tx: &mpsc::UnboundedSender<PeerEvent>,
) -> Result<(), EngineError> {
    let piece_count = {
        let torrents = torrents.read();
        torrents.get(hash).map(|t| t.meta.piece_count()).unwrap_or(0)
    };
    let bf = Bitfield::new(piece_count);
    let _ = event_tx.send(PeerEvent::PeerBitfield {
        torrent_hash: hash.to_string(),
        peer_addr,
        bitfield: bf,
    });

    Ok(())
}

/// Handles periodic tasks during the keepalive interval.
async fn handle_periodic_tasks(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    last_pex_time: &mut Instant,
    pex_sent_peers: &mut HashSet<SocketAddr>,
    listen_port: u16,
) -> Result<(), EngineError> {
    let should_unchoke = {
        let torrents = torrents.read();
        torrents
            .get(hash)
            .map(|t| t.unchoked_peers.contains(&peer_addr))
            .unwrap_or(false)
    };

    if should_unchoke && conn.am_choking() {
        conn.send(Message::Unchoke).await?;
    } else if !should_unchoke && !conn.am_choking() {
        conn.send(Message::Choke).await?;
    }

    if last_pex_time.elapsed() >= PEX_SEND_INTERVAL {
        if let Some(pex_id) = conn.peer_pex_id() {
            send_pex_update(hash, peer_addr, conn, torrents, pex_sent_peers, listen_port, pex_id)
                .await?;
            *last_pex_time = Instant::now();
        }
    }

    Ok(())
}

/// Handles a cancel message for a completed piece.
async fn handle_cancel(
    conn: &mut PeerConnection,
    piece_index: u32,
    pending_requests: &mut HashMap<u32, Vec<(u32, u32)>>,
    peer_addr: SocketAddr,
) -> Result<(), EngineError> {
    if let Some(blocks) = pending_requests.remove(&piece_index) {
        let num_blocks = blocks.len();
        for (offset, length) in blocks {
            let _ = conn
                .send(Message::Cancel {
                    index: piece_index,
                    begin: offset,
                    length,
                })
                .await;
        }
        tracing::debug!(
            "Sent {} Cancel messages to {} for piece {}",
            num_blocks,
            peer_addr,
            piece_index
        );
    }
    Ok(())
}

/// BEP-11 peer flags
const PEX_FLAG_SEED: u8 = 0x02;       // Upload only (seed)
const PEX_FLAG_REACHABLE: u8 = 0x10;  // Connectable/reachable

/// Get PEX flags for a peer based on their bitfield
fn get_pex_flags(bitfield: Option<&Bitfield>) -> u8 {
    let mut flags = PEX_FLAG_REACHABLE; // Assume connectable since we're connected to them
    if let Some(bf) = bitfield {
        if bf.is_complete() {
            flags |= PEX_FLAG_SEED;
        }
    }
    flags
}

/// Sends initial PEX burst to a peer.
/// Per BEP-11, initial messages are exempt from the 50-peer limit.
/// We only share actually connected peers, not just known peers.
async fn send_initial_pex(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    pex_sent_peers: &mut HashSet<SocketAddr>,
    listen_port: u16,
    pex_id: u8,
) -> Result<(), EngineError> {
    // BEP-11: Only share peers we're actually connected to for better liveness
    // Also collect their bitfields to determine flags
    let connected_peers: Vec<(SocketAddr, u8)> = {
        let torrents = torrents.read();
        if let Some(torrent) = torrents.get(hash) {
            torrent
                .peers
                .iter()
                .filter(|(&p, _)| p != peer_addr && is_shareable_pex_addr(&p, listen_port))
                .map(|(&addr, info)| (addr, get_pex_flags(info.bitfield.as_ref())))
                .collect()
        } else {
            Vec::new()
        }
    };

    if !connected_peers.is_empty() {
        // BEP-11 allows more than 50 peers in initial message
        let peers_to_send: Vec<_> = connected_peers.into_iter().take(100).collect();
        let mut pex_msg = PexMessage::new();

        for (addr, flags) in &peers_to_send {
            match addr {
                SocketAddr::V4(v4) => {
                    pex_msg.added.push(PexPeer {
                        ip: std::net::IpAddr::V4(*v4.ip()),
                        port: v4.port(),
                    });
                    pex_msg.added_flags.push(*flags);
                }
                SocketAddr::V6(v6) => {
                    pex_msg.added6.push(PexPeer {
                        ip: std::net::IpAddr::V6(*v6.ip()),
                        port: v6.port(),
                    });
                    pex_msg.added6_flags.push(*flags);
                }
            }
            pex_sent_peers.insert(*addr);
        }

        let pex_data = pex_msg.encode();
        let ext_msg = ExtensionMessage::Unknown {
            id: pex_id,
            data: pex_data,
        };
        if conn.send(Message::Extended(ext_msg)).await.is_ok() {
            tracing::debug!(
                "Sent initial PEX burst to {} with {} peers",
                peer_addr,
                peers_to_send.len()
            );
        }
    }

    Ok(())
}

/// Sends periodic PEX updates.
/// Per BEP-11: Only share actually connected peers for better liveness information.
/// Combined added contacts should not exceed 50 entries per message.
async fn send_pex_update(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    pex_sent_peers: &mut HashSet<SocketAddr>,
    listen_port: u16,
    pex_id: u8,
) -> Result<(), EngineError> {
    // BEP-11: Only share peers we're actually connected to, with their flags
    let (connected_peers, peer_flags): (HashSet<SocketAddr>, HashMap<SocketAddr, u8>) = {
        let torrents = torrents.read();
        if let Some(torrent) = torrents.get(hash) {
            let addrs: HashSet<_> = torrent.peers.keys().copied().collect();
            let flags: HashMap<_, _> = torrent
                .peers
                .iter()
                .map(|(&addr, info)| (addr, get_pex_flags(info.bitfield.as_ref())))
                .collect();
            (addrs, flags)
        } else {
            (HashSet::new(), HashMap::new())
        }
    };

    let new_peers: Vec<(SocketAddr, u8)> = connected_peers
        .iter()
        .filter(|&&p| {
            p != peer_addr && is_shareable_pex_addr(&p, listen_port) && !pex_sent_peers.contains(&p)
        })
        .map(|&addr| (addr, peer_flags.get(&addr).copied().unwrap_or(PEX_FLAG_REACHABLE)))
        .take(PEX_MAX_IPV4_PEERS)
        .collect();

    // BEP-11: Report peers we previously shared but are no longer connected to
    let all_stale_peers: Vec<SocketAddr> = pex_sent_peers
        .iter()
        .filter(|p| !connected_peers.contains(p))
        .copied()
        .collect();
    let dropped_peers: Vec<SocketAddr> = all_stale_peers
        .iter()
        .copied()
        .take(PEX_MAX_IPV4_PEERS)
        .collect();

    if !new_peers.is_empty() || !dropped_peers.is_empty() {
        let mut pex_msg = PexMessage::new();

        for (addr, flags) in &new_peers {
            match addr {
                SocketAddr::V4(v4) => {
                    pex_msg.added.push(PexPeer {
                        ip: std::net::IpAddr::V4(*v4.ip()),
                        port: v4.port(),
                    });
                    pex_msg.added_flags.push(*flags);
                }
                SocketAddr::V6(v6) => {
                    pex_msg.added6.push(PexPeer {
                        ip: std::net::IpAddr::V6(*v6.ip()),
                        port: v6.port(),
                    });
                    pex_msg.added6_flags.push(*flags);
                }
            }
            pex_sent_peers.insert(*addr);
        }

        for addr in &dropped_peers {
            match addr {
                SocketAddr::V4(v4) => {
                    pex_msg.dropped.push(PexPeer {
                        ip: std::net::IpAddr::V4(*v4.ip()),
                        port: v4.port(),
                    });
                }
                SocketAddr::V6(v6) => {
                    pex_msg.dropped6.push(PexPeer {
                        ip: std::net::IpAddr::V6(*v6.ip()),
                        port: v6.port(),
                    });
                }
            }
        }

        for addr in &all_stale_peers {
            pex_sent_peers.remove(addr);
        }

        let pex_data = pex_msg.encode();
        let ext_msg = ExtensionMessage::Unknown {
            id: pex_id,
            data: pex_data,
        };
        conn.send(Message::Extended(ext_msg)).await?;
        tracing::debug!(
            "Sent PEX to {} with {} added, {} dropped peers",
            peer_addr,
            new_peers.len(),
            dropped_peers.len()
        );
    }

    Ok(())
}


/// Requests pieces from a peer using batched sends for better performance.
///
/// This function collects all block requests first, then sends them in a single
/// batch to avoid the overhead of individual async sends and reduce lock contention.
pub async fn request_pieces(
    hash: &str,
    peer_addr: SocketAddr,
    conn: &mut PeerConnection,
    torrents: &Arc<RwLock<HashMap<String, ManagedTorrent>>>,
    pending_requests: &mut HashMap<u32, Vec<(u32, u32)>>,
) -> Result<(), EngineError> {

    // Check if peer is choking us (don't spam logs for this common case)
    if conn.peer_choking() {
        return Ok(());
    }

    // Calculate current outstanding requests to enforce pipelining limit
    let current_pending: usize = pending_requests.values().map(|v| v.len()).sum();
    if current_pending >= MAX_REQUESTS_PER_PEER {
        // Already have enough outstanding requests, don't add more
        // Don't log - this is expected and very noisy
        return Ok(());
    }
    let requests_budget = MAX_REQUESTS_PER_PEER - current_pending;

    // Collect all the information we need in a single lock acquisition
    let (all_block_requests, endgame_requests) = {
        let torrents = torrents.read();
        let Some(torrent) = torrents.get(hash) else {
            return Ok(());
        };

        let piece_count = torrent.meta.piece_count();
        let peer_bf = conn.bitfield().cloned().unwrap_or_else(|| {
            tracing::debug!("Peer has no bitfield yet, assuming they have all pieces");
            Bitfield::full(piece_count)
        });

        let is_endgame = torrent.piece_manager.is_endgame();

        let mut all_requests = Vec::with_capacity(requests_budget);
        let mut pieces_started = 0usize;

        for _ in 0..MAX_PARALLEL_PIECES {
            if all_requests.len() >= requests_budget {
                break;
            }

            if let Some(piece_idx) = torrent.piece_manager.pick_piece(&peer_bf) {
                torrent.piece_manager.start_piece(piece_idx);

                let block_requests = torrent.piece_manager.get_block_requests(piece_idx);

                let blocks_to_take = (requests_budget - all_requests.len()).min(block_requests.len());
                if blocks_to_take > 0 {
                    pieces_started += 1;
                    for req in block_requests.into_iter().take(blocks_to_take) {
                        torrent.piece_manager.add_pending_block(&req);
                        all_requests.push(req);
                    }
                }
            } else {
                break;
            }
        }

        if !all_requests.is_empty() {
            tracing::debug!(
                "[REQ] {} -> {} blocks across {} pieces (budget={}, pending={})",
                peer_addr,
                all_requests.len(),
                pieces_started,
                requests_budget,
                current_pending
            );
        } else if pieces_started == 0 {
            // Log why we couldn't pick any pieces
            let our_bf = torrent.piece_manager.bitfield();
            let peer_has_count = (0..piece_count).filter(|&i| peer_bf.has_piece(i)).count();
            let we_need_count = (0..piece_count).filter(|&i| !our_bf.has_piece(i)).count();
            let overlap = (0..piece_count).filter(|&i| peer_bf.has_piece(i) && !our_bf.has_piece(i)).count();
            tracing::debug!(
                "[REQ] {} -> 0 blocks: peer_has={}/{}, we_need={}, overlap={}, active={}",
                peer_addr,
                peer_has_count,
                piece_count,
                we_need_count,
                overlap,
                torrent.piece_manager.active_piece_count()
            );
        }

        // Handle endgame mode
        let endgame = if is_endgame && all_requests.is_empty() {
            let requests = torrent.piece_manager.get_endgame_requests();
            let remaining_budget = requests_budget - all_requests.len();
            let filtered: Vec<_> = requests
                .into_iter()
                .filter(|req| peer_bf.has_piece(req.piece_index as usize))
                .take(remaining_budget)
                .collect();
            if !filtered.is_empty() {
                tracing::info!(
                    "Endgame mode: requesting {} blocks from peer",
                    filtered.len()
                );
            }
            filtered
        } else {
            Vec::new()
        };

        (all_requests, endgame)
    };

    // Build messages to send
    let mut messages = Vec::with_capacity(all_block_requests.len() + endgame_requests.len());

    for req in &all_block_requests {
        pending_requests
            .entry(req.piece_index)
            .or_default()
            .push((req.offset, req.length));

        messages.push(Message::Request {
            index: req.piece_index,
            begin: req.offset,
            length: req.length,
        });
    }

    for req in &endgame_requests {
        pending_requests
            .entry(req.piece_index)
            .or_default()
            .push((req.offset, req.length));

        messages.push(Message::Request {
            index: req.piece_index,
            begin: req.offset,
            length: req.length,
        });
    }

    // Send all requests in a single batch - this is the key performance improvement
    if !messages.is_empty() {
        conn.send_batch(&messages).await?;
    }

    Ok(())
}
