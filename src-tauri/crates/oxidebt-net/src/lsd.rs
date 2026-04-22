use crate::error::NetError;
use oxidebt_constants::{
    LSD_ANNOUNCE_INTERVAL, LSD_CHANNEL_CAPACITY, LSD_COOKIE_SIZE, LSD_MULTICAST_V4,
    LSD_MULTICAST_V6, LSD_PORT,
};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::time::interval;

fn lsd_multicast_v4() -> Ipv4Addr {
    LSD_MULTICAST_V4.parse().expect("invalid LSD_MULTICAST_V4")
}

fn lsd_multicast_v6() -> Ipv6Addr {
    LSD_MULTICAST_V6.parse().expect("invalid LSD_MULTICAST_V6")
}

#[derive(Debug, Clone)]
pub struct LsdAnnounce {
    pub info_hash: [u8; 20],
    pub port: u16,
    pub source: SocketAddr,
}

pub struct LsdService {
    socket_v4: Option<Arc<UdpSocket>>,
    socket_v6: Option<Arc<UdpSocket>>,
    port: u16,
    cookie: String,
    announce_tx: broadcast::Sender<LsdAnnounce>,
}

impl LsdService {
    pub async fn new(port: u16) -> Result<Self, NetError> {
        let mut cookie_bytes = [0u8; LSD_COOKIE_SIZE];
        rand::RngExt::fill(&mut rand::rng(), &mut cookie_bytes);
        let cookie = hex::encode(&cookie_bytes);

        let socket_v4 = Self::bind_v4().await.ok();
        let socket_v6 = Self::bind_v6().await.ok();

        if socket_v4.is_none() && socket_v6.is_none() {
            return Err(NetError::Lsd("failed to bind any socket".into()));
        }

        let (announce_tx, _) = broadcast::channel(LSD_CHANNEL_CAPACITY);

        Ok(Self {
            socket_v4,
            socket_v6,
            port,
            cookie,
            announce_tx,
        })
    }

    async fn bind_v4() -> Result<Arc<UdpSocket>, NetError> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, LSD_PORT)).await?;
        socket.set_multicast_loop_v4(false)?;
        socket.join_multicast_v4(lsd_multicast_v4(), Ipv4Addr::UNSPECIFIED)?;
        Ok(Arc::new(socket))
    }

    async fn bind_v6() -> Result<Arc<UdpSocket>, NetError> {
        let socket =
            UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, LSD_PORT, 0, 0)).await?;
        socket.set_multicast_loop_v6(false)?;
        socket.join_multicast_v6(&lsd_multicast_v6(), 0)?;
        Ok(Arc::new(socket))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LsdAnnounce> {
        self.announce_tx.subscribe()
    }

    pub fn start(self: Arc<Self>, info_hashes: Vec<[u8; 20]>) {
        let service = self.clone();
        tokio::spawn(async move {
            service.run(info_hashes).await;
        });
    }

    async fn run(&self, info_hashes: Vec<[u8; 20]>) {
        let mut announce_interval = interval(LSD_ANNOUNCE_INTERVAL);

        loop {
            tokio::select! {
                _ = announce_interval.tick() => {
                    for hash in &info_hashes {
                        let _ = self.announce(hash).await;
                    }
                }
                result = self.receive() => {
                    if let Ok(announce) = result {
                        let _ = self.announce_tx.send(announce);
                    }
                }
            }
        }
    }

    pub async fn announce(&self, info_hash: &[u8; 20]) -> Result<(), NetError> {
        let message = self.format_announce(info_hash);

        if let Some(ref socket) = self.socket_v4 {
            let dest = SocketAddrV4::new(lsd_multicast_v4(), LSD_PORT);
            let _ = socket.send_to(message.as_bytes(), dest).await;
        }

        if let Some(ref socket) = self.socket_v6 {
            let dest = SocketAddrV6::new(lsd_multicast_v6(), LSD_PORT, 0, 0);
            let _ = socket.send_to(message.as_bytes(), dest).await;
        }

        Ok(())
    }

    fn format_announce(&self, info_hash: &[u8; 20]) -> String {
        let hash_hex = hex::encode(info_hash);
        // BEP-14: Message must end with \r\n\r\n (double CRLF)
        format!(
            "BT-SEARCH * HTTP/1.1\r\n\
             Host: {host}:{port}\r\n\
             Port: {listen_port}\r\n\
             Infohash: {hash}\r\n\
             cookie: {cookie}\r\n\
             \r\n",
            host = lsd_multicast_v4(),
            port = LSD_PORT,
            listen_port = self.port,
            hash = hash_hex,
            cookie = self.cookie
        )
    }

    async fn receive(&self) -> Result<LsdAnnounce, NetError> {
        let mut buf_v4 = vec![0u8; 1024];
        let mut buf_v6 = vec![0u8; 1024];

        match (&self.socket_v4, &self.socket_v6) {
            (Some(v4), Some(v6)) => {
                tokio::select! {
                    result = v4.recv_from(&mut buf_v4) => {
                        let (n, source) = result?;
                        self.parse_announce(&buf_v4[..n], source)
                    }
                    result = v6.recv_from(&mut buf_v6) => {
                        let (n, source) = result?;
                        self.parse_announce(&buf_v6[..n], source)
                    }
                }
            }
            (Some(v4), None) => {
                let (n, source) = v4.recv_from(&mut buf_v4).await?;
                self.parse_announce(&buf_v4[..n], source)
            }
            (None, Some(v6)) => {
                let (n, source) = v6.recv_from(&mut buf_v6).await?;
                self.parse_announce(&buf_v6[..n], source)
            }
            (None, None) => Err(NetError::Lsd("no socket available".into())),
        }
    }

    fn parse_announce(&self, data: &[u8], source: SocketAddr) -> Result<LsdAnnounce, NetError> {
        let text = std::str::from_utf8(data)
            .map_err(|_| NetError::InvalidResponse("invalid utf8".into()))?;

        if !text.starts_with("BT-SEARCH") {
            return Err(NetError::InvalidResponse("not a BT-SEARCH message".into()));
        }

        let mut port = None;
        let mut info_hash = None;
        let mut cookie = None;

        for line in text.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("Port:") {
                port = value.trim().parse().ok();
            } else if let Some(value) = line.strip_prefix("Infohash:") {
                let hash_hex = value.trim();
                if hash_hex.len() == 40
                    && let Ok(bytes) = hex::decode(hash_hex)
                    && bytes.len() == 20
                {
                    let mut hash = [0u8; 20];
                    hash.copy_from_slice(&bytes);
                    info_hash = Some(hash);
                }
            } else if let Some(value) = line.strip_prefix("cookie:") {
                cookie = Some(value.trim().to_string());
            }
        }

        if cookie.as_deref() == Some(&self.cookie) {
            return Err(NetError::InvalidResponse("own announce".into()));
        }

        let port = port.ok_or_else(|| NetError::InvalidResponse("missing port".into()))?;
        let info_hash =
            info_hash.ok_or_else(|| NetError::InvalidResponse("missing info hash".into()))?;

        Ok(LsdAnnounce {
            info_hash,
            port,
            source,
        })
    }
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes
            .iter()
            .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{:02x}", b);
                s
            })
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, ()> {
        if !s.len().is_multiple_of(2) {
            return Err(());
        }

        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
            .collect()
    }
}
