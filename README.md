# RBitt - A Modern BitTorrent Client

RBitt (pronounced "rabbit") is a modern, cross-platform BitTorrent client built with Tauri and React. The backend is implemented entirely in safe Rust using the OxideBT library.

## Features

### Protocol Support
- BitTorrent v1 (BEP 3)
- Magnet links (BEP 9)
- Extension protocol (BEP 10) with `ut_metadata`
- Fast Extension (BEP 6)

> BitTorrent v2 (BEP 52) and hybrid v1/v2 torrents (BEP 47) are planned but not yet implemented.

### Peer Discovery
- HTTP(S) trackers with announce and scrape
- UDP trackers (BEP 15)
- DHT (Kademlia, BEP 5)
- Local Service Discovery (LSD, BEP 14)
- Peer Exchange (PEX, BEP 11)

### Networking
- UPnP and NAT-PMP port mapping
- IPv4 and IPv6 support
- Per-torrent and global bandwidth limits

### Storage
- Sparse file allocation
- Full preallocation mode
- SHA-1 piece verification
- Multi-file torrent support

## Architecture

RBitt is built as a Rust workspace with the following crates:

| Crate | Description |
|-------|-------------|
| `oxidebt-bencode` | Bencode encoding/decoding |
| `oxidebt-torrent` | Torrent metainfo parsing, info hash computation |
| `oxidebt-peer` | Peer wire protocol, piece management |
| `oxidebt-tracker` | HTTP and UDP tracker clients |
| `oxidebt-dht` | Kademlia DHT implementation |
| `oxidebt-disk` | Disk I/O with verification |
| `oxidebt-cache` | Block/piece caching layer |
| `oxidebt-net` | PEX, LSD, UPnP, bandwidth limiting |
| `oxidebt-constants` | Shared protocol constants and tuning parameters |
| `rbitt` | Tauri application integrating all components |

## Building

### Prerequisites

- Rust 1.95+
- Node.js 20.19+ or 22.12+ (required by Vite 8)
- [Bun](https://bun.sh/) (this project uses `bun.lock`)
- Platform-specific Tauri dependencies (see [Tauri Prerequisites](https://tauri.app/start/prerequisites/))

### Development

```bash
# Install frontend dependencies
bun install

# Run in development mode
bun run tauri dev
```

### Production Build

```bash
# Build for production
bun run tauri build
```

## Usage

1. Launch RBitt
2. Add a torrent via:
   - File > Open Torrent File
   - File > Open Magnet Link
   - Drag and drop a .torrent file
3. Select download location
4. Monitor progress in the main window

## Configuration

Settings are stored in the platform-specific config directory:
- Linux: `~/.config/rbitt/`
- macOS: `~/Library/Application Support/com.rbitt.dev/`
- Windows: `%APPDATA%\rbitt\`

## Development

### Running Tests

```bash
cd src-tauri
cargo test --workspace
```

### Code Quality

```bash
# Check for warnings
cargo clippy --workspace -- -D warnings

# Format code
cargo fmt --all
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- Built with [Tauri](https://tauri.app/)
- Protocol specifications from [bittorrent.org](https://www.bittorrent.org/beps/bep_0000.html)
