# qBittorrent Features Not in RBitt

This document lists features available in qBittorrent that are not currently implemented in RBitt.

## Protocol & Transport

| Feature | Description | Priority |
|---------|-------------|----------|
| µTP (Micro Transport Protocol) | BEP-29 - Congestion-friendly UDP-based transport | High |
| Message Stream Encryption (MSE/PE) | Encryption for peer connections | High |
| Proxy Support | SOCKS4/SOCKS5/HTTP proxy for connections | Medium |
| IP Filtering / Blocklists | Block connections from known bad peers | Medium |
| Holepunch (full) | BEP-55 NAT traversal (only flag support exists) | Low |

## Download Management

| Feature | Description | Priority |
|---------|-------------|----------|
| Sequential Downloading | Download pieces in order for streaming playback | High |
| File Priority / Skip Files | Set priority per file, skip unwanted files | High |
| Torrent Queuing | Limit active downloads/uploads, queue the rest | High |
| Super Seeding Mode | Initial seeding optimization for new torrents | Medium |
| Incomplete File Extension | Use `.!qb` extension until file is complete | Low |
| Pre-allocation | Reserve disk space before downloading | Low |
| Move to Trash | Move deleted files to trash instead of permanent delete | Low |

## Organization & Automation

| Feature | Description | Priority |
|---------|-------------|----------|
| Categories | Organize torrents into categories with default paths | High |
| Tags | Add multiple tags to torrents for filtering | Medium |
| RSS Feed Support | Subscribe to RSS feeds with download filters | Medium |
| Auto-load from Folder | Watch folder for .torrent files | Medium |
| External Program on Completion | Run script/program when torrent completes | Low |
| Email Notifications | Send email on torrent completion | Low |

## User Interface

| Feature | Description | Priority |
|---------|-------------|----------|
| Web UI | Remote control via web browser | High |
| Integrated Search Engine | Search torrent sites from within the app | Medium |
| Torrent Creation Tool | Create .torrent files from local content | Medium |
| Tracker Management UI | Add/remove/edit trackers per torrent | Medium |
| Peer List View | View connected peers with details | Low |
| Speed Graphs | Visual download/upload speed history | Low |

## Advanced Features

| Feature | Description | Priority |
|---------|-------------|----------|
| Scheduled Speed Limits | Different speed limits at different times | Medium |
| Anonymous Mode | Disable DHT/PEX/LSD for privacy | Medium |
| Embedded Tracker | Built-in tracker for sharing torrents | Low |
| Mark-of-the-Web | Windows security feature for downloaded files | Low |
| Network Interface Binding | Bind to specific network interface | Low |

## Multi-Torrent Management (Audit Findings)

Current issues with RBitt's multi-torrent handling:

1. **No Active Torrent Limit Enforcement**: `MAX_ACTIVE_TORRENTS` (5) is defined but not used
2. **No Global Connection Limit**: `MAX_GLOBAL_CONNECTIONS` (500) is defined but not enforced
3. **No Torrent Priority System**: All torrents compete equally for resources
4. **No Queue Management**: Cannot pause lower-priority torrents when at capacity

## Summary by Category

### Already Implemented in RBitt
- BitTorrent v1/v2/Hybrid (BEP 3, 52)
- DHT (BEP 5)
- PEX (BEP 11)
- LSD (BEP 14)
- Fast Extension (BEP 6)
- Extension Protocol (BEP 10)
- ut_metadata (BEP 9)
- UDP Trackers (BEP 15)
- HTTP Trackers
- Multitracker (BEP 12)
- Compact Peer Lists (BEP 23)
- Private Torrents (BEP 27)
- Magnet Links
- UPnP/NAT-PMP Port Forwarding
- Global Bandwidth Limiting
- Rarest-First Piece Selection
- Endgame Mode
- Request Pipelining
- Choking Algorithm

### High Priority Missing Features
1. µTP transport
2. Connection encryption (MSE/PE)
3. Sequential downloading
4. File priority / skip files
5. Torrent queuing
6. Categories
7. Web UI

### Medium Priority Missing Features
1. Proxy support
2. IP filtering
3. Tags
4. RSS feeds
5. Scheduled speed limits
6. Search engine
7. Torrent creation
8. Anonymous mode

### Low Priority Missing Features
1. Super seeding
2. Pre-allocation
3. Incomplete file extension
4. Move to trash
5. Email notifications
6. External program execution
7. Speed graphs
8. Embedded tracker
