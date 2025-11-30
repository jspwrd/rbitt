use crate::bitfield::Bitfield;
use crate::error::PeerError;
use crate::fast::FastExtensionState;
use crate::message::{Handshake, Message};
use crate::peer_id::PeerId;
use crate::transport::TransportStream;
use bytes::{Buf, BytesMut};
use oxidebt_constants::{
    HANDSHAKE_TIMEOUT, KEEPALIVE_INTERVAL, MAX_MESSAGE_SIZE, PEER_READ_TIMEOUT, READ_BUFFER_SIZE,
    SOCKET_RECV_BUFFER_SIZE, SOCKET_SEND_BUFFER_SIZE,
};
use socket2::{Socket, TcpKeepalive};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

fn configure_socket_for_performance(stream: &TcpStream) -> Result<(), PeerError> {
    stream.set_nodelay(true).map_err(PeerError::Io)?;

    #[cfg(unix)]
    {
        use std::os::unix::io::{AsRawFd, FromRawFd};
        let fd = stream.as_raw_fd();
        let socket = unsafe { Socket::from_raw_fd(fd) };

        let _ = socket.set_recv_buffer_size(SOCKET_RECV_BUFFER_SIZE);
        let _ = socket.set_send_buffer_size(SOCKET_SEND_BUFFER_SIZE);

        let keepalive = TcpKeepalive::new().with_time(Duration::from_secs(60));
        let _ = socket.set_tcp_keepalive(&keepalive);

        std::mem::forget(socket);
    }

    #[cfg(windows)]
    {
        use std::os::windows::io::{AsRawSocket, FromRawSocket};
        let raw_socket = stream.as_raw_socket();
        let socket = unsafe { Socket::from_raw_socket(raw_socket) };

        let _ = socket.set_recv_buffer_size(SOCKET_RECV_BUFFER_SIZE);
        let _ = socket.set_send_buffer_size(SOCKET_SEND_BUFFER_SIZE);

        let keepalive = TcpKeepalive::new().with_time(Duration::from_secs(60));
        let _ = socket.set_tcp_keepalive(&keepalive);

        std::mem::forget(socket);
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    Connecting,
    Handshaking,
    Connected,
    Disconnected,
}

pub struct PeerConnection {
    stream: TransportStream,
    addr: SocketAddr,
    state: PeerState,
    our_peer_id: PeerId,
    remote_peer_id: Option<PeerId>,
    info_hash: [u8; 20],
    am_choking: bool,
    am_interested: bool,
    peer_choking: bool,
    peer_interested: bool,
    remote_bitfield: Option<Bitfield>,
    piece_count: usize,
    last_message_sent: Instant,
    last_message_received: Instant,
    read_buffer: BytesMut,
    supports_extensions: bool,
    supports_dht: bool,
    supports_fast: bool,
    download_bytes: u64,
    upload_bytes: u64,
    peer_pex_id: Option<u8>,
    peer_metadata_id: Option<u8>,
    fast_state: FastExtensionState,
}

impl PeerConnection {
    pub async fn connect(
        addr: SocketAddr,
        info_hash: [u8; 20],
        our_peer_id: PeerId,
        piece_count: usize,
    ) -> Result<Self, PeerError> {
        let tcp = timeout(HANDSHAKE_TIMEOUT, TcpStream::connect(addr))
            .await
            .map_err(|_| PeerError::Timeout)?
            .map_err(PeerError::Io)?;

        if let Err(e) = configure_socket_for_performance(&tcp) {
            tracing::debug!("Failed to configure socket performance for {}: {}", addr, e);
        }

        let stream = TransportStream::from(tcp);
        Ok(Self::new(stream, addr, info_hash, our_peer_id, piece_count))
    }

    pub fn from_accepted(
        stream: TcpStream,
        addr: SocketAddr,
        info_hash: [u8; 20],
        our_peer_id: PeerId,
        piece_count: usize,
    ) -> Self {
        if let Err(e) = configure_socket_for_performance(&stream) {
            tracing::debug!("Failed to configure socket performance for {}: {}", addr, e);
        }

        Self::new(
            TransportStream::from(stream),
            addr,
            info_hash,
            our_peer_id,
            piece_count,
        )
    }

    fn new(
        stream: TransportStream,
        addr: SocketAddr,
        info_hash: [u8; 20],
        our_peer_id: PeerId,
        piece_count: usize,
    ) -> Self {
        Self {
            stream,
            addr,
            state: PeerState::Connecting,
            our_peer_id,
            remote_peer_id: None,
            info_hash,
            am_choking: true,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
            remote_bitfield: None,
            piece_count,
            last_message_sent: Instant::now(),
            last_message_received: Instant::now(),
            read_buffer: BytesMut::with_capacity(READ_BUFFER_SIZE),
            supports_extensions: false,
            supports_dht: false,
            supports_fast: false,
            download_bytes: 0,
            upload_bytes: 0,
            peer_pex_id: None,
            peer_metadata_id: None,
            fast_state: FastExtensionState::new(),
        }
    }

    pub async fn handshake(&mut self) -> Result<(), PeerError> {
        self.state = PeerState::Handshaking;

        let our_handshake = Handshake::new(self.info_hash, *self.our_peer_id.as_bytes());
        let handshake_bytes = our_handshake.encode();

        timeout(HANDSHAKE_TIMEOUT, self.stream.write_all(&handshake_bytes))
            .await
            .map_err(|_| PeerError::Timeout)?
            .map_err(PeerError::Io)?;

        let mut buf = [0u8; 68];
        timeout(HANDSHAKE_TIMEOUT, self.stream.read_exact(&mut buf))
            .await
            .map_err(|_| PeerError::Timeout)?
            .map_err(PeerError::Io)?;

        let remote_handshake = Handshake::parse(&buf)?;

        if remote_handshake.info_hash != self.info_hash {
            return Err(PeerError::InfoHashMismatch);
        }

        self.remote_peer_id = PeerId::from_bytes(&remote_handshake.peer_id);
        self.supports_extensions = remote_handshake.supports_extensions();
        self.supports_dht = remote_handshake.supports_dht();
        self.supports_fast = remote_handshake.supports_fast();

        if self.supports_fast {
            let peer_ip = self.addr.ip();
            self.fast_state
                .init_for_peer(peer_ip, &self.info_hash, self.piece_count);
        }

        self.state = PeerState::Connected;
        self.last_message_received = Instant::now();

        Ok(())
    }

    pub async fn send(&mut self, message: Message) -> Result<(), PeerError> {
        let bytes = message.encode_with_length();
        self.stream.write_all(&bytes).await.map_err(PeerError::Io)?;
        self.last_message_sent = Instant::now();
        self.upload_bytes += bytes.len() as u64;

        match &message {
            Message::Choke => self.am_choking = true,
            Message::Unchoke => self.am_choking = false,
            Message::Interested => self.am_interested = true,
            Message::NotInterested => self.am_interested = false,
            _ => {}
        }

        Ok(())
    }

    /// Sends multiple messages in a single batch, flushing only once at the end.
    /// This is much more efficient than calling send() for each message individually
    /// as it avoids the overhead of multiple syscalls and TCP packet fragmentation.
    pub async fn send_batch(&mut self, messages: &[Message]) -> Result<(), PeerError> {
        if messages.is_empty() {
            return Ok(());
        }

        // Pre-calculate total size for efficient allocation
        let total_size: usize = messages.iter().map(|m| m.encode_with_length().len()).sum();
        let mut buffer = Vec::with_capacity(total_size);

        for message in messages {
            let bytes = message.encode_with_length();
            self.upload_bytes += bytes.len() as u64;

            match message {
                Message::Choke => self.am_choking = true,
                Message::Unchoke => self.am_choking = false,
                Message::Interested => self.am_interested = true,
                Message::NotInterested => self.am_interested = false,
                _ => {}
            }

            buffer.extend_from_slice(&bytes);
        }

        self.stream
            .write_all(&buffer)
            .await
            .map_err(PeerError::Io)?;
        self.last_message_sent = Instant::now();

        Ok(())
    }

    pub async fn receive(&mut self) -> Result<Message, PeerError> {
        loop {
            if let Some(message) = self.try_parse_message()? {
                self.last_message_received = Instant::now();

                match &message {
                    Message::Choke => self.peer_choking = true,
                    Message::Unchoke => self.peer_choking = false,
                    Message::Interested => self.peer_interested = true,
                    Message::NotInterested => self.peer_interested = false,
                    Message::Bitfield(bits) => {
                        self.remote_bitfield = Some(Bitfield::from_bytes(bits, self.piece_count)?);
                    }
                    Message::Have { piece_index } => {
                        if let Some(ref mut bf) = self.remote_bitfield {
                            bf.set_piece(*piece_index as usize);
                        } else {
                            let mut bf = Bitfield::new(self.piece_count);
                            bf.set_piece(*piece_index as usize);
                            self.remote_bitfield = Some(bf);
                        }
                    }
                    Message::HaveAll => {
                        let mut bf = Bitfield::new(self.piece_count);
                        for i in 0..self.piece_count {
                            bf.set_piece(i);
                        }
                        self.remote_bitfield = Some(bf);
                    }
                    Message::HaveNone => {
                        self.remote_bitfield = Some(Bitfield::new(self.piece_count));
                    }
                    Message::Piece { data, .. } => {
                        self.download_bytes += data.len() as u64;
                    }
                    _ => {}
                }

                return Ok(message);
            }

            // Use a larger read buffer to reduce syscall overhead
            let mut buf = vec![0u8; 65536];
            let n = timeout(PEER_READ_TIMEOUT, self.stream.read(&mut buf))
                .await
                .map_err(|_| PeerError::Timeout)?
                .map_err(PeerError::Io)?;

            if n == 0 {
                return Err(PeerError::ConnectionClosed);
            }

            self.read_buffer.extend_from_slice(&buf[..n]);
        }
    }

    fn try_parse_message(&mut self) -> Result<Option<Message>, PeerError> {
        if self.read_buffer.len() < 4 {
            return Ok(None);
        }

        let length = u32::from_be_bytes([
            self.read_buffer[0],
            self.read_buffer[1],
            self.read_buffer[2],
            self.read_buffer[3],
        ]);

        if length > MAX_MESSAGE_SIZE as u32 {
            return Err(PeerError::MessageTooLarge(length));
        }

        let total_len = 4 + length as usize;
        if self.read_buffer.len() < total_len {
            return Ok(None);
        }

        self.read_buffer.advance(4);
        let message_bytes: Vec<u8> = self.read_buffer[..length as usize].to_vec();
        self.read_buffer.advance(length as usize);

        let message = Message::parse(&message_bytes)?;
        Ok(Some(message))
    }

    pub async fn maybe_send_keepalive(&mut self) -> Result<(), PeerError> {
        if self.last_message_sent.elapsed() >= KEEPALIVE_INTERVAL {
            self.send(Message::KeepAlive).await?;
        }
        Ok(())
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn peer_id(&self) -> Option<&PeerId> {
        self.remote_peer_id.as_ref()
    }

    pub fn am_choking(&self) -> bool {
        self.am_choking
    }

    pub fn am_interested(&self) -> bool {
        self.am_interested
    }

    pub fn peer_choking(&self) -> bool {
        self.peer_choking
    }

    pub fn peer_interested(&self) -> bool {
        self.peer_interested
    }

    pub fn bitfield(&self) -> Option<&Bitfield> {
        self.remote_bitfield.as_ref()
    }

    pub fn supports_extensions(&self) -> bool {
        self.supports_extensions
    }

    pub fn supports_dht(&self) -> bool {
        self.supports_dht
    }

    pub fn piece_count(&self) -> usize {
        self.piece_count
    }

    pub fn state(&self) -> PeerState {
        self.state
    }

    pub fn download_bytes(&self) -> u64 {
        self.download_bytes
    }

    pub fn upload_bytes(&self) -> u64 {
        self.upload_bytes
    }

    pub fn idle_time(&self) -> Duration {
        self.last_message_received.elapsed()
    }

    pub fn set_peer_pex_id(&mut self, id: Option<u8>) {
        self.peer_pex_id = id;
    }

    pub fn peer_pex_id(&self) -> Option<u8> {
        self.peer_pex_id
    }

    pub fn set_peer_metadata_id(&mut self, id: Option<u8>) {
        self.peer_metadata_id = id;
    }

    pub fn peer_metadata_id(&self) -> Option<u8> {
        self.peer_metadata_id
    }

    pub async fn close(&mut self) {
        self.state = PeerState::Disconnected;
        let _ = self.stream.shutdown().await;
    }

    pub fn supports_fast(&self) -> bool {
        self.supports_fast
    }

    pub fn get_allowed_fast_set(&self) -> &HashSet<u32> {
        self.fast_state.get_outgoing_allowed_fast()
    }

    pub fn add_allowed_fast(&mut self, piece_index: u32) {
        self.fast_state.add_incoming_allowed_fast(piece_index);
    }

    pub fn add_suggestion(&mut self, piece_index: u32) {
        self.fast_state.add_suggestion(piece_index);
    }

    pub fn suggested_pieces(&self) -> &[u32] {
        &self.fast_state.suggested_pieces
    }

    pub fn can_request_while_choked(&self, piece_index: u32) -> bool {
        self.fast_state.can_request_while_choked(piece_index)
    }

    pub async fn send_allowed_fast_set(&mut self) -> Result<(), PeerError> {
        if !self.supports_fast {
            return Ok(());
        }
        let pieces: Vec<u32> = self
            .fast_state
            .allowed_fast_set_outgoing
            .iter()
            .copied()
            .collect();
        for piece_index in pieces {
            self.send(Message::AllowedFast { piece_index }).await?;
        }
        Ok(())
    }

    pub fn info_hash(&self) -> &[u8; 20] {
        &self.info_hash
    }
}
