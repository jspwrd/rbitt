use bytes::Bytes;
use oxidebt_bencode::{decode, Value};
use std::net::SocketAddr;

/// Checks if a peer address is suitable for sharing via PEX.
/// Per BEP-11, we should only share globally routable addresses.
pub fn is_shareable_pex_addr(addr: &SocketAddr, listen_port: u16) -> bool {
    // Don't share loopback addresses
    if addr.ip().is_loopback() {
        return false;
    }
    // Don't share addresses using our listen port (likely ourselves)
    if addr.port() == listen_port {
        return false;
    }
    // Don't share unspecified addresses (0.0.0.0, ::)
    if addr.ip().is_unspecified() {
        return false;
    }
    // Don't share multicast addresses
    if addr.ip().is_multicast() {
        return false;
    }

    match addr.ip() {
        std::net::IpAddr::V4(ipv4) => {
            // Don't share private IPv4 ranges (RFC 1918)
            // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
            if ipv4.is_private() {
                return false;
            }
            // Don't share link-local addresses (169.254.0.0/16)
            if ipv4.is_link_local() {
                return false;
            }
            // Don't share broadcast addresses
            if ipv4.is_broadcast() {
                return false;
            }
            // Don't share documentation addresses (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24)
            if ipv4.is_documentation() {
                return false;
            }
        }
        std::net::IpAddr::V6(ipv6) => {
            // Don't share link-local addresses (fe80::/10)
            // These are not routable outside the local network segment
            let segments = ipv6.segments();
            if (segments[0] & 0xffc0) == 0xfe80 {
                return false;
            }
            // Don't share site-local (deprecated) addresses (fec0::/10)
            if (segments[0] & 0xffc0) == 0xfec0 {
                return false;
            }
            // Don't share unique local addresses (fc00::/7)
            // These are the IPv6 equivalent of private addresses
            if (segments[0] & 0xfe00) == 0xfc00 {
                return false;
            }
        }
    }

    true
}

/// Parses PEX message data into a list of peer addresses.
pub fn parse_pex_peers(data: &[u8]) -> Option<Vec<SocketAddr>> {
    let value = decode(data).ok()?;
    let dict = match value {
        Value::Dict(d) => d,
        _ => return None,
    };

    let mut peers = Vec::new();

    if let Some(Value::Bytes(bytes)) = dict.get(&Bytes::from_static(b"added")) {
        for chunk in bytes.chunks_exact(6) {
            let ip = std::net::Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
            peers.push(SocketAddr::from((ip, port)));
        }
    }

    if let Some(Value::Bytes(bytes)) = dict.get(&Bytes::from_static(b"added6")) {
        for chunk in bytes.chunks_exact(18) {
            let ip_bytes: [u8; 16] = chunk[0..16].try_into().ok()?;
            let ip = std::net::Ipv6Addr::from(ip_bytes);
            let port = u16::from_be_bytes([chunk[16], chunk[17]]);
            peers.push(SocketAddr::from((ip, port)));
        }
    }

    if peers.is_empty() {
        None
    } else {
        Some(peers)
    }
}
