# RBitt Codebase Architectural Issues

## 1. Dead/Underutilized Code

### UTP Protocol - REMOVED
The UTP (Micro Transport Protocol, BEP 29) stub implementation has been removed. The ~1,586 lines of dead code in `oxidebt-net/utp/` have been deleted as they were never functional (always returned `Poll::Pending`).

### WebSeed - REMOVED
The WebSeed module has been removed as it was never used anywhere in the codebase.

### Session API (oxidebt-engine/session.rs) - IN USE
The Session API is used by the CLI tool (`oxidebt-cli`). While there are two parallel engine implementations (Session API and TorrentEngine in the Tauri app), both serve different purposes:
- `TorrentEngine` (src-tauri/src/engine.rs) - Powers the Tauri GUI application
- `Session` (oxidebt-engine/session.rs) - Powers the CLI tool

## 2. Monolithic Engine File

`src-tauri/src/engine.rs` is ~3,400 lines containing:
- Rate calculation helpers (RateCalculator, TorrentStats)
- Peer tracking (PeerInfo, FailedPeer)
- Torrent state machine (TorrentState, ManagedTorrent)
- Main TorrentEngine struct with 49+ functions
- Massive match statements for message handling

This could be split into logical modules for maintainability (future improvement).

## 3. Known Bugs - Status

| # | Bug | Status | Notes |
|---|-----|--------|-------|
| 1 | HAVE message doesn't update peer's stored bitfield | **FIXED** | PeerEvent::PeerHave handler updates bitfield |
| 2 | HaveAll/HaveNone don't call update_availability() | **FIXED** | PeerEvent::PeerBitfield handler calls update_availability() |
| 3 | Bitfield doesn't call update_availability() | **FIXED** | PeerEvent::PeerBitfield handler calls update_availability() |
| 4 | Rate calculation stale timestamps | **NOT A BUG** | Code uses single `now` timestamp for all operations |
| 5 | Endgame mode doesn't send Cancel messages | **FIXED** | cancel_tx sends notifications when pieces complete |
| 6 | Upload counter includes protocol prefix | **BY DESIGN** | Counts all bytes sent on wire, valid metric |
| 7 | piece_size() underflow when piece_count is 0 | **FIXED** | Added check for piece_count == 0 |
| 8 | DHT announce_peer doesn't store peers | **FIXED** | peer_store.add_peer() is called |
| 9 | read_extension_handshake infinite loop | **FIXED** | MAX_MESSAGES limit of 100 prevents infinite loop |
| 10 | PEX doesn't filter own address properly | **FIXED** | Added is_shareable_pex_addr() helper |
| 11 | LSD only listens on IPv4 | **FIXED** | Code handles both IPv4 and IPv6 with tokio::select! |
| 12 | DHT parse_response logic error | **FIXED** | Logic correctly returns FindNode when no token |
| 13 | UPnP missing NewInternalClient | **FIXED** | local_ip is passed to the template |
| 14 | NAT-PMP remove_mapping ignores port | **FIXED** | external_port parameter is used correctly |

## 4. Well-Implemented Modules

These modules are clean and well-tested:
- **oxidebt-bencode** - Minimal, 519 lines of tests
- **oxidebt-torrent** - Handles v1, v2, hybrid torrents correctly
- **oxidebt-constants** - Excellent centralization of config values
- **oxidebt-disk** - Solid async disk I/O with 474 lines of tests
- **oxidebt-peer** - Comprehensive peer wire protocol (781 lines of tests)
- **oxidebt-tracker** - Good HTTP/UDP tracker support
- **oxidebt-dht** - Well-implemented Kademlia DHT

## 5. Test Coverage

| Module | Test Lines | Assessment |
|--------|-----------|------------|
| oxidebt-peer | 781 | Excellent |
| oxidebt-bencode | 519 | Comprehensive |
| oxidebt-disk | 474 | Good |
| oxidebt-torrent | 217 | Good |
| oxidebt-engine | 223 | Basic |
| oxidebt-dht | 170 | Basic |
| oxidebt-net | 104 | Minimal |
| oxidebt-tracker | 98 | Minimal |

## 6. Completed Improvements

1. **Removed UTP stub** - Deleted 1,586 lines of non-functional code
2. **Removed WebSeed** - Deleted unused module
3. **Fixed piece_size() underflow** - Added bounds check
4. **Improved PEX filtering** - Added is_shareable_pex_addr() to filter loopback and unspecified addresses

## 7. Future Recommendations

1. **Split engine.rs** - Break into logical modules (peer_handling, state_machine, messages)
2. **Add more tests** - Especially for oxidebt-net and oxidebt-tracker modules
3. **Consider merging engines** - Evaluate if TorrentEngine and Session can share more code
