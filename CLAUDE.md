# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

This project uses **bun** as its package manager (see `bun.lock`). MSRV is **Rust 1.95**; frontend requires **Node 20.19+ or 22.12+** (Vite 8).

```bash
# Install frontend dependencies
bun install

# Run in development mode (starts both Vite dev server and Tauri)
bun run tauri dev

# Build for production
bun run tauri build

# Type-check + bundle the frontend only
bun run build

# Run Rust tests (from src-tauri directory)
cd src-tauri && cargo test --workspace

# Run a single test
cd src-tauri && cargo test --package <crate-name> <test_name>

# Check for warnings and lint
cd src-tauri && cargo clippy --workspace -- -D warnings

# Format code
cd src-tauri && cargo fmt --all

# Type check Rust code
cd src-tauri && cargo check --workspace
```

## Architecture Overview

RBitt is a BitTorrent client built with Tauri (Rust backend + React frontend). The Rust backend is organized as a workspace with multiple crates under `src-tauri/crates/`.

### Crate Dependency Hierarchy

```
rbitt (Tauri app)
├── oxidebt-torrent    # Metainfo parsing, magnet links, info hashes
│   └── oxidebt-bencode    # Bencode encoding/decoding
├── oxidebt-peer       # Peer wire protocol, piece management, connections
├── oxidebt-tracker    # HTTP and UDP tracker clients
├── oxidebt-dht        # Kademlia DHT implementation
├── oxidebt-disk       # Disk I/O, file storage, piece verification
│   └── oxidebt-cache      # Block/piece caching layer
├── oxidebt-net        # PEX, LSD, UPnP, bandwidth limiting
└── oxidebt-constants  # Shared protocol constants and tuning parameters
```

### Core Engine (`src-tauri/src/engine/`)

The `TorrentEngine` in `engine/mod.rs` orchestrates all BitTorrent operations:

- **Peer Discovery**: Combines DHT, trackers, LSD, and PEX for finding peers
- **Connection Management**: Rate-limited peer connections with retry logic
- **Piece Management**: Tracks piece availability, downloads, and verification
- **Event System**: Uses `mpsc` channels to communicate peer events (connected, disconnected, piece completed, etc.)

Key background tasks started by `TorrentEngine::start_background_tasks()`:
- `start_listener()` - Accepts incoming peer connections
- `start_event_processor()` - Processes peer events from all connections
- `start_lsd_announcer/receiver()` - Local peer discovery
- `start_dht_discovery_task()` - Periodic DHT queries
- `start_reannounce_task()` - Tracker re-announcements

### Protocol Support

- BitTorrent v1 (BEP 3), v2 (BEP 52), and Hybrid (BEP 47)
- Extension protocol (BEP 10) with ut_metadata (BEP 9) and ut_pex (BEP 11)
- Fast Extension (BEP 6)
- DHT (BEP 5), UDP trackers (BEP 15)

### Frontend (`src/`)

React + TypeScript frontend using Tauri's IPC for communication with the Rust backend. Tauri commands are defined in `src-tauri/src/lib.rs`.

## Key Constants

Protocol tuning parameters are centralized in `oxidebt-constants/src/lib.rs`. Important ones:
- `MAX_PEERS_PER_TORRENT`: 200
- `MAX_REQUESTS_PER_PEER`: 500 (request pipelining)
- `BLOCK_SIZE`: 16384 (16KB)
- `MAX_UNCHOKED_PEERS`: 4
