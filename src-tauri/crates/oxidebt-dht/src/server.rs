use crate::error::DhtError;
use crate::message::{DhtMessage, DhtResponse};
use crate::node::{Node, NodeId};
use crate::routing::RoutingTable;
use bytes::Bytes;
use oxidebt_constants::{
    DHT_ALPHA, DHT_BOOTSTRAP_NODES, DHT_MAX_ITERATIONS, DHT_QUERY_TIMEOUT, MAX_PENDING_DHT_QUERIES,
};
use parking_lot::RwLock;
use rand::Rng;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, info, warn};

const MAX_PEERS_PER_TORRENT: usize = 1000;
const PEER_ANNOUNCE_LIFETIME: Duration = Duration::from_secs(30 * 60);

struct AnnouncedPeer {
    addr: SocketAddr,
    announced_at: Instant,
}

struct PeerStore {
    peers: HashMap<[u8; 20], Vec<AnnouncedPeer>>,
}

impl PeerStore {
    fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    fn add_peer(&mut self, info_hash: [u8; 20], addr: SocketAddr) {
        let peers = self.peers.entry(info_hash).or_default();

        let now = Instant::now();
        peers.retain(|p| now.duration_since(p.announced_at) < PEER_ANNOUNCE_LIFETIME);

        peers.retain(|p| p.addr != addr);

        if peers.len() < MAX_PEERS_PER_TORRENT {
            peers.push(AnnouncedPeer {
                addr,
                announced_at: now,
            });
        }
    }

    fn get_peers(&mut self, info_hash: &[u8; 20]) -> Vec<SocketAddr> {
        let now = Instant::now();
        if let Some(peers) = self.peers.get_mut(info_hash) {
            peers.retain(|p| now.duration_since(p.announced_at) < PEER_ANNOUNCE_LIFETIME);
            peers.iter().map(|p| p.addr).collect()
        } else {
            Vec::new()
        }
    }
}

struct PendingQuery {
    sender: mpsc::Sender<DhtResponse>,
}

struct TokenSecrets {
    current: [u8; 16],
    previous: [u8; 16],
}

impl TokenSecrets {
    fn new() -> Self {
        Self {
            current: rand::random(),
            previous: rand::random(),
        }
    }

    fn rotate(&mut self) {
        self.previous = self.current;
        self.current = rand::random();
    }
}

pub struct DhtServer {
    socket: Arc<UdpSocket>,
    our_id: NodeId,
    routing_table: Arc<RoutingTable>,
    pending_queries: Arc<RwLock<HashMap<Bytes, PendingQuery>>>,
    port: u16,
    token_secrets: RwLock<TokenSecrets>,
    peer_store: RwLock<PeerStore>,
}

impl DhtServer {
    pub async fn bind(port: u16) -> Result<Self, DhtError> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port)).await?;
        let local_addr = socket.local_addr()?;
        let our_id = NodeId::generate();

        info!("DHT server bound to {} with id {}", local_addr, our_id);

        Ok(Self {
            socket: Arc::new(socket),
            our_id,
            routing_table: Arc::new(RoutingTable::new(our_id)),
            pending_queries: Arc::new(RwLock::new(HashMap::new())),
            port: local_addr.port(),
            token_secrets: RwLock::new(TokenSecrets::new()),
            peer_store: RwLock::new(PeerStore::new()),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn our_id(&self) -> &NodeId {
        &self.our_id
    }

    pub fn routing_table(&self) -> &RoutingTable {
        &self.routing_table
    }

    pub async fn bootstrap(&self) -> Result<(), DhtError> {
        info!("Starting DHT bootstrap");

        for addr_str in DHT_BOOTSTRAP_NODES {
            match tokio::net::lookup_host(addr_str).await {
                Ok(mut addrs) => {
                    if let Some(addr) = addrs.next() {
                        debug!("Pinging bootstrap node {}", addr);
                        if let Ok(response) = self.ping(addr).await {
                            info!("Got response from bootstrap node: {:?}", response);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to resolve bootstrap node {}: {}", addr_str, e);
                }
            }
        }

        self.find_node(self.our_id).await?;

        info!(
            "DHT bootstrap complete, {} nodes in routing table",
            self.routing_table.node_count()
        );
        Ok(())
    }

    pub async fn ping(&self, addr: SocketAddr) -> Result<DhtResponse, DhtError> {
        let tid = self.generate_transaction_id();
        let msg = DhtMessage::ping(tid.clone(), &self.our_id);

        self.send_query(addr, msg, tid).await
    }

    pub async fn find_node(&self, target: NodeId) -> Result<Vec<Node>, DhtError> {
        let closest = self.routing_table.find_closest(&target, 8);

        if closest.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_nodes = Vec::new();

        // Query up to DHT_ALPHA nodes in parallel for faster discovery
        let nodes_to_query: Vec<_> = closest.iter().take(DHT_ALPHA).cloned().collect();
        let mut queries = Vec::with_capacity(nodes_to_query.len());

        for node in &nodes_to_query {
            let tid = self.generate_transaction_id();
            let msg = DhtMessage::find_node(tid.clone(), &self.our_id, target);
            queries.push((node.id, self.send_query(node.addr, msg, tid)));
        }

        let results = futures::future::join_all(
            queries
                .into_iter()
                .map(|(id, fut)| async move { (id, fut.await) }),
        )
        .await;

        for (node_id, result) in results {
            match result {
                Ok(DhtResponse::FindNode { nodes, .. }) => {
                    for n in nodes {
                        self.routing_table.add_node(n.clone());
                        all_nodes.push(n);
                    }
                }
                Ok(DhtResponse::GetPeers {
                    nodes: Some(nodes), ..
                }) => {
                    for n in nodes {
                        self.routing_table.add_node(n.clone());
                        all_nodes.push(n);
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    debug!("find_node query failed: {}", e);
                    self.routing_table.mark_failed(&node_id);
                }
            }
        }

        Ok(all_nodes)
    }

    pub async fn get_peers(&self, info_hash: [u8; 20]) -> Result<Vec<SocketAddr>, DhtError> {
        let target = NodeId::from_bytes(&info_hash)?;

        let mut peers = Vec::new();
        let mut queried = std::collections::HashSet::new();
        let mut to_query: Vec<Node> = self.routing_table.find_closest(&target, 8);

        for _ in 0..DHT_MAX_ITERATIONS {
            if to_query.is_empty() {
                break;
            }

            to_query.sort_by(|a, b| {
                let dist_a = a.id.distance(&target);
                let dist_b = b.id.distance(&target);
                dist_a.cmp(&dist_b)
            });

            let mut queries = Vec::new();
            let mut nodes_to_query = Vec::new();

            for node in to_query.iter() {
                if !queried.contains(&node.id) && queries.len() < DHT_ALPHA {
                    queried.insert(node.id);
                    nodes_to_query.push(node.clone());

                    let tid = self.generate_transaction_id();
                    let msg = DhtMessage::get_peers(tid.clone(), &self.our_id, info_hash);
                    queries.push(self.send_query(node.addr, msg, tid));
                }
            }

            if queries.is_empty() {
                break;
            }

            let results = futures::future::join_all(queries).await;

            let mut new_nodes = Vec::new();

            for (i, result) in results.into_iter().enumerate() {
                match result {
                    Ok(DhtResponse::GetPeers {
                        peers: Some(p),
                        nodes,
                        ..
                    }) => {
                        peers.extend(p);
                        if let Some(nodes) = nodes {
                            for n in nodes {
                                if !queried.contains(&n.id) {
                                    self.routing_table.add_node(n.clone());
                                    new_nodes.push(n);
                                }
                            }
                        }
                    }
                    Ok(DhtResponse::GetPeers {
                        nodes: Some(nodes), ..
                    }) => {
                        for n in nodes {
                            if !queried.contains(&n.id) {
                                self.routing_table.add_node(n.clone());
                                new_nodes.push(n);
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        debug!("get_peers query failed: {}", e);
                        if i < nodes_to_query.len() {
                            self.routing_table.mark_failed(&nodes_to_query[i].id);
                        }
                    }
                }
            }

            to_query = new_nodes;

            if peers.len() >= 50 {
                break;
            }
        }

        info!(
            "DHT get_peers found {} peers after querying {} nodes",
            peers.len(),
            queried.len()
        );
        Ok(peers)
    }

    pub async fn announce(
        &self,
        info_hash: [u8; 20],
        port: u16,
        token: Bytes,
        node_addr: SocketAddr,
    ) -> Result<(), DhtError> {
        let tid = self.generate_transaction_id();
        let msg = DhtMessage::announce_peer(tid.clone(), &self.our_id, info_hash, port, token);

        self.send_query(node_addr, msg, tid).await?;
        Ok(())
    }

    pub async fn get_peers_with_tokens(
        &self,
        info_hash: [u8; 20],
    ) -> Result<(Vec<SocketAddr>, Vec<(SocketAddr, Bytes)>), DhtError> {
        let target = NodeId::from_bytes(&info_hash)?;

        let mut peers = Vec::new();
        let mut tokens: Vec<(SocketAddr, Bytes)> = Vec::new();
        let mut queried = std::collections::HashSet::new();
        let mut to_query: Vec<Node> = self.routing_table.find_closest(&target, 8);

        for _ in 0..DHT_MAX_ITERATIONS {
            if to_query.is_empty() {
                break;
            }

            to_query.sort_by(|a, b| {
                let dist_a = a.id.distance(&target);
                let dist_b = b.id.distance(&target);
                dist_a.cmp(&dist_b)
            });

            let mut queries = Vec::new();
            let mut nodes_to_query = Vec::new();

            for node in to_query.iter() {
                if !queried.contains(&node.id) && queries.len() < DHT_ALPHA {
                    queried.insert(node.id);
                    nodes_to_query.push(node.clone());

                    let tid = self.generate_transaction_id();
                    let msg = DhtMessage::get_peers(tid.clone(), &self.our_id, info_hash);
                    queries.push(self.send_query(node.addr, msg, tid));
                }
            }

            if queries.is_empty() {
                break;
            }

            let results = futures::future::join_all(queries).await;

            let mut new_nodes = Vec::new();

            for (i, result) in results.into_iter().enumerate() {
                match result {
                    Ok(DhtResponse::GetPeers {
                        peers: Some(p),
                        nodes,
                        token,
                        ..
                    }) => {
                        peers.extend(p);
                        if i < nodes_to_query.len() && !token.is_empty() {
                            tokens.push((nodes_to_query[i].addr, token));
                        }
                        if let Some(nodes) = nodes {
                            for n in nodes {
                                if !queried.contains(&n.id) {
                                    self.routing_table.add_node(n.clone());
                                    new_nodes.push(n);
                                }
                            }
                        }
                    }
                    Ok(DhtResponse::GetPeers {
                        nodes: Some(nodes),
                        token,
                        ..
                    }) => {
                        if i < nodes_to_query.len() && !token.is_empty() {
                            tokens.push((nodes_to_query[i].addr, token));
                        }
                        for n in nodes {
                            if !queried.contains(&n.id) {
                                self.routing_table.add_node(n.clone());
                                new_nodes.push(n);
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        debug!("get_peers query failed: {}", e);
                        if i < nodes_to_query.len() {
                            self.routing_table.mark_failed(&nodes_to_query[i].id);
                        }
                    }
                }
            }

            to_query = new_nodes;

            if peers.len() >= 50 {
                break;
            }
        }

        info!(
            "DHT get_peers_with_tokens found {} peers and {} tokens",
            peers.len(),
            tokens.len()
        );
        Ok((peers, tokens))
    }

    pub async fn announce_to_nodes(
        &self,
        info_hash: [u8; 20],
        port: u16,
        nodes_with_tokens: Vec<(SocketAddr, Bytes)>,
    ) {
        for (addr, token) in nodes_with_tokens.into_iter().take(8) {
            if let Err(e) = self.announce(info_hash, port, token, addr).await {
                debug!("Failed to announce to {}: {}", addr, e);
            }
        }
    }

    async fn send_query(
        &self,
        addr: SocketAddr,
        msg: DhtMessage,
        tid: Bytes,
    ) -> Result<DhtResponse, DhtError> {
        let (tx, mut rx) = mpsc::channel(1);

        {
            let mut pending = self.pending_queries.write();
            if pending.len() >= MAX_PENDING_DHT_QUERIES {
                return Err(DhtError::RateLimited);
            }

            pending.insert(tid.clone(), PendingQuery { sender: tx });
        }

        let data = msg.encode()?;
        self.socket.send_to(&data, addr).await?;

        let result = timeout(DHT_QUERY_TIMEOUT, rx.recv()).await;

        {
            self.pending_queries.write().remove(&tid);
        }

        match result {
            Ok(Some(response)) => Ok(response),
            Ok(None) => Err(DhtError::Timeout),
            Err(_) => Err(DhtError::Timeout),
        }
    }

    pub async fn run(&self) -> Result<(), DhtError> {
        let mut buf = vec![0u8; 65535];
        let mut refresh_interval = tokio::time::interval(Duration::from_secs(15 * 60));
        refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut token_rotation_interval = tokio::time::interval(Duration::from_secs(5 * 60));
        token_rotation_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        refresh_interval.tick().await;
        token_rotation_interval.tick().await;

        loop {
            tokio::select! {
                result = self.socket.recv_from(&mut buf) => {
                    let (n, addr) = result?;
                    match DhtMessage::parse(&buf[..n]) {
                        Ok(msg) => {
                            self.handle_message(msg, addr).await;
                        }
                        Err(e) => {
                            debug!("Failed to parse DHT message from {}: {}", addr, e);
                        }
                    }
                }
                _ = refresh_interval.tick() => {
                    self.refresh_stale_buckets().await;
                }
                _ = token_rotation_interval.tick() => {
                    self.rotate_token_secret();
                }
            }
        }
    }

    async fn refresh_stale_buckets(&self) {
        let stale = self.routing_table.stale_buckets();
        if stale.is_empty() {
            return;
        }

        debug!("Refreshing {} stale DHT buckets", stale.len());

        for bucket_idx in stale {
            let target = self.generate_id_for_bucket(bucket_idx);
            let _ = self.find_node(target).await;
        }
    }

    fn generate_id_for_bucket(&self, bucket_idx: usize) -> NodeId {
        if bucket_idx >= 160 {
            return NodeId::generate();
        }

        let mut id = self.our_id.0;
        let byte_idx = bucket_idx / 8;
        let bit_idx = 7 - (bucket_idx % 8);

        id[byte_idx] ^= 1 << bit_idx;

        if bit_idx > 0 {
            let random_byte: u8 = rand::random();
            let keep_mask = !((1u8 << bit_idx) - 1);
            let random_mask = (1u8 << bit_idx) - 1;
            id[byte_idx] = (id[byte_idx] & keep_mask) | (random_byte & random_mask);
        }

        for i in (byte_idx + 1)..20 {
            id[i] = rand::random();
        }

        NodeId(id)
    }

    async fn handle_message(&self, msg: DhtMessage, addr: SocketAddr) {
        if let Some(id) = msg.sender_id {
            self.routing_table.add_node(Node::new(id, addr));
        }

        if let Some(response) = msg.response {
            let pending = self.pending_queries.read();
            if let Some(query) = pending.get(&msg.transaction_id) {
                let _ = query.sender.try_send(response);
            }
            return;
        }

        if let Some((name, query)) = msg.query {
            self.handle_query(msg.transaction_id, addr, &name, query, msg.sender_id)
                .await;
        }
    }

    async fn handle_query(
        &self,
        tid: Bytes,
        addr: SocketAddr,
        name: &str,
        query: crate::message::DhtQuery,
        _sender_id: Option<NodeId>,
    ) {
        let response = match (name, query) {
            ("ping", _) => DhtMessage {
                transaction_id: tid,
                sender_id: None,
                query: None,
                response: Some(DhtResponse::Ping { id: self.our_id }),
            },
            ("find_node", crate::message::DhtQuery::FindNode { target }) => {
                let nodes = self.routing_table.find_closest(&target, 8);
                DhtMessage {
                    transaction_id: tid,
                    sender_id: None,
                    query: None,
                    response: Some(DhtResponse::FindNode {
                        id: self.our_id,
                        nodes,
                    }),
                }
            }
            ("get_peers", crate::message::DhtQuery::GetPeers { info_hash }) => {
                let target = NodeId::from_bytes(&info_hash).unwrap_or(self.our_id);
                let nodes = self.routing_table.find_closest(&target, 8);
                let token = self.generate_token(&addr);

                let stored_peers = self.peer_store.write().get_peers(&info_hash);
                let peers = if stored_peers.is_empty() {
                    None
                } else {
                    Some(stored_peers)
                };

                DhtMessage {
                    transaction_id: tid,
                    sender_id: None,
                    query: None,
                    response: Some(DhtResponse::GetPeers {
                        id: self.our_id,
                        token,
                        peers,
                        nodes: Some(nodes),
                    }),
                }
            }
            (
                "announce_peer",
                crate::message::DhtQuery::AnnouncePeer {
                    info_hash,
                    port,
                    implied_port,
                    token,
                },
            ) => {
                if !self.validate_token(&addr, &token) {
                    debug!("Rejecting announce_peer from {} - invalid token", addr);
                    DhtMessage {
                        transaction_id: tid,
                        sender_id: None,
                        query: None,
                        response: Some(DhtResponse::Error {
                            code: 203,
                            message: "Invalid token".to_string(),
                        }),
                    }
                } else {
                    let peer_port = if implied_port { addr.port() } else { port };
                    let peer_addr = SocketAddr::new(addr.ip(), peer_port);
                    self.peer_store.write().add_peer(info_hash, peer_addr);
                    debug!(
                        "Stored announced peer {} for info_hash {:?}",
                        peer_addr,
                        &info_hash[..8]
                    );

                    DhtMessage {
                        transaction_id: tid,
                        sender_id: None,
                        query: None,
                        response: Some(DhtResponse::AnnouncePeer { id: self.our_id }),
                    }
                }
            }
            _ => return,
        };

        if let Ok(data) = response.encode() {
            let _ = self.socket.send_to(&data, addr).await;
        }
    }

    fn generate_transaction_id(&self) -> Bytes {
        let id: [u8; 2] = rand::thread_rng().gen();
        Bytes::copy_from_slice(&id)
    }

    fn generate_token(&self, addr: &SocketAddr) -> Bytes {
        self.generate_token_with_secret(addr, &self.token_secrets.read().current)
    }

    fn generate_token_with_secret(&self, addr: &SocketAddr, secret: &[u8; 16]) -> Bytes {
        use sha1::{Digest, Sha1};

        let mut hasher = Sha1::new();
        hasher.update(secret);
        hasher.update(addr.ip().to_string().as_bytes());

        let result = hasher.finalize();
        Bytes::copy_from_slice(&result[..8])
    }

    fn validate_token(&self, addr: &SocketAddr, token: &Bytes) -> bool {
        let secrets = self.token_secrets.read();

        let current_token = self.generate_token_with_secret(addr, &secrets.current);
        if &current_token == token {
            return true;
        }

        let previous_token = self.generate_token_with_secret(addr, &secrets.previous);
        &previous_token == token
    }

    pub fn rotate_token_secret(&self) {
        self.token_secrets.write().rotate();
        debug!("DHT token secret rotated");
    }
}
