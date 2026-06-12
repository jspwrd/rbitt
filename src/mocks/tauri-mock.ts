// Browser-only mock of the Tauri IPC layer.
//
// When the frontend runs in a plain browser (vite dev server opened directly,
// e.g. for UI work or screenshot tooling) there is no Tauri runtime, so every
// invoke would throw. This module installs a canned-data implementation of
// `window.__TAURI_INTERNALS__` instead, letting the full UI render with
// realistic torrents in every state.
//
// It is a no-op inside the real app (the Tauri runtime injects the genuine
// internals before any module runs) and compiles away in production builds.
//
// Scenarios: append `?mock=empty` to the URL for the empty/first-run state.

/* eslint-disable @typescript-eslint/no-explicit-any */

function installMock() {
  const params = new URLSearchParams(window.location.search);
  const scenario = params.get("mock") ?? "default";

  const KiB = 1024;
  const MiB = 1024 * KiB;
  const GiB = 1024 * MiB;

  interface MockTorrent {
    info_hash: string;
    name: string;
    state: string;
    progress: number;
    download_rate: number;
    upload_rate: number;
    downloaded: number;
    uploaded: number;
    total_size: number;
    peers: number;
    seeds: number;
  }

  let torrents: MockTorrent[] =
    scenario === "empty"
      ? []
      : [
          {
            info_hash: "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
            name: "ubuntu-24.04.2-desktop-amd64.iso",
            state: "downloading",
            progress: 47.3,
            download_rate: 8.4 * MiB,
            upload_rate: 412 * KiB,
            downloaded: 2.79 * GiB,
            uploaded: 301 * MiB,
            total_size: 5.9 * GiB,
            peers: 42,
            seeds: 18,
          },
          {
            info_hash: "b674016f31fc54286b8db4ec8a8d3bb0b3a5e9c1",
            name: "debian-12.10.0-amd64-DVD-1.iso",
            state: "uploading",
            progress: 100,
            download_rate: 0,
            upload_rate: 1.2 * MiB,
            downloaded: 3.7 * GiB,
            uploaded: 5.4 * GiB,
            total_size: 3.7 * GiB,
            peers: 7,
            seeds: 0,
          },
          {
            info_hash: "c0ffee254729296a45a3885639ac7e10419b2f0a",
            name: "Fedora-Workstation-Live-42-1.1.x86_64.iso",
            state: "stalledDL",
            progress: 12.8,
            download_rate: 0,
            upload_rate: 0,
            downloaded: 296 * MiB,
            uploaded: 12 * MiB,
            total_size: 2.3 * GiB,
            peers: 2,
            seeds: 0,
          },
          {
            info_hash: "deadbeef2f1924f24e2c4d1a9bdfe425d28c2a51",
            name: "archlinux-2026.06.01-x86_64.iso",
            state: "completed",
            progress: 100,
            download_rate: 0,
            upload_rate: 0,
            downloaded: 1.2 * GiB,
            uploaded: 840 * MiB,
            total_size: 1.2 * GiB,
            peers: 0,
            seeds: 0,
          },
          {
            info_hash: "0badc0de7d4a9b1ff0a3c1b2d4e5f60718293a4b",
            name: "LibreOffice_25.2.4_Linux_x86-64_rpm.tar.gz",
            state: "pausedDL",
            progress: 23.1,
            download_rate: 0,
            upload_rate: 0,
            downloaded: 78 * MiB,
            uploaded: 4 * MiB,
            total_size: 338 * MiB,
            peers: 0,
            seeds: 0,
          },
          {
            info_hash: "facade00aa11bb22cc33dd44ee55ff6607182930",
            name: "Big.Buck.Bunny.2008.2160p.collection",
            state: "queuedDL",
            progress: 0,
            download_rate: 0,
            upload_rate: 0,
            downloaded: 0,
            uploaded: 0,
            total_size: 12.6 * GiB,
            peers: 0,
            seeds: 0,
          },
          {
            info_hash: "5eed5eed1f2e3d4c5b6a79880716253443526170",
            name: "nixos-minimal-25.05-x86_64-linux.iso",
            state: "checkingDL",
            progress: 64.0,
            download_rate: 0,
            upload_rate: 0,
            downloaded: 745 * MiB,
            uploaded: 0,
            total_size: 1.1 * GiB,
            peers: 0,
            seeds: 0,
          },
          {
            info_hash: "tails-pending-magnet-hash-0000000000000000",
            name: "tails-amd64-6.5.img",
            state: "metaDL",
            progress: 0,
            download_rate: 0,
            upload_rate: 0,
            downloaded: 0,
            uploaded: 0,
            total_size: 0,
            peers: 0,
            seeds: 0,
          },
          {
            info_hash: "badbadbad1a2b3c4d5e6f708192a3b4c5d6e7f80",
            name: "magnet:?xt=urn:btih:badbadbad…",
            state: "error",
            progress: 0,
            download_rate: 0,
            upload_rate: 0,
            downloaded: 0,
            uploaded: 0,
            total_size: 0,
            peers: 0,
            seeds: 0,
          },
        ];

  const trackers = (hash: string) => [
    {
      url: "https://torrent.ubuntu.com/announce",
      status: "Working",
      peers: 60,
      seeds: 1284,
      leechers: 96,
      last_announce: 312,
      next_announce: 1488,
      message: null,
    },
    {
      url: "udp://tracker.opentrackr.org:1337/announce",
      status: "Working",
      peers: 38,
      seeds: 902,
      leechers: 51,
      last_announce: 290,
      next_announce: 1510,
      message: null,
    },
    {
      url: `udp://dead.example.org:6969/${hash.slice(0, 4)}`,
      status: "Error",
      peers: 0,
      seeds: 0,
      leechers: 0,
      last_announce: 3120,
      next_announce: null,
      message: "timed out",
    },
  ];

  const peers = [
    ["82.64.112.7:51413", 412 * MiB, 18 * MiB, false, true, 92.4],
    ["176.9.34.201:6881", 226 * MiB, 36 * MiB, false, false, 100],
    ["91.121.8.42:50000", 88 * MiB, 2 * MiB, true, true, 31.7],
    ["[2001:db8::8a2e:370]:6881", 64 * MiB, 51 * MiB, false, true, 76.0],
    ["203.0.113.88:28960", 12 * MiB, 0, true, false, 8.2],
    ["198.51.100.14:6889", 2 * MiB, 9 * MiB, false, true, 54.9],
  ].map(([address, dl, ul, choke, int, prog]) => ({
    address,
    download_bytes: dl,
    upload_bytes: ul,
    is_choking_us: choke,
    is_interested: int,
    progress: prog,
  }));

  const files = [
    { path: "ubuntu-24.04.2/ubuntu-24.04.2-desktop-amd64.iso", size: 5.9 * GiB, progress: 47.3, downloaded: 2.79 * GiB },
    { path: "ubuntu-24.04.2/SHA256SUMS", size: 198, progress: 100, downloaded: 198 },
    { path: "ubuntu-24.04.2/SHA256SUMS.gpg", size: 833, progress: 100, downloaded: 833 },
  ];

  const categories = [
    { name: "linux-isos", save_path: "/Users/jasper/Downloads/isos" },
    { name: "media", save_path: "/Users/jasper/Downloads/media" },
  ];

  const rssFeeds = [
    {
      id: "feed-1",
      url: "https://fedoramagazine.org/feed/torrents",
      name: "Fedora Releases",
      enabled: true,
      refresh_interval: 1800,
      last_refresh: Math.floor(Date.now() / 1000) - 600,
      last_error: null,
    },
    {
      id: "feed-2",
      url: "https://distrowatch.com/news/torrents.xml",
      name: "DistroWatch",
      enabled: true,
      refresh_interval: 3600,
      last_refresh: Math.floor(Date.now() / 1000) - 3000,
      last_error: null,
    },
  ];

  const rssItems = [
    { title: "Fedora-Workstation-42-1.1 (x86_64)", torrent_url: "https://example.org/f42.torrent", link: "https://example.org/f42", pub_date: Math.floor(Date.now() / 1000) - 86400, description: "Official release torrent", is_downloaded: true },
    { title: "Fedora-KDE-42-1.1 (x86_64)", torrent_url: "https://example.org/f42-kde.torrent", link: null, pub_date: Math.floor(Date.now() / 1000) - 90000, description: null, is_downloaded: false },
    { title: "Fedora-Server-42-1.1 (aarch64)", torrent_url: "https://example.org/f42-server.torrent", link: null, pub_date: Math.floor(Date.now() / 1000) - 95000, description: null, is_downloaded: false },
  ];

  const rssRules = [
    {
      id: "rule-1",
      name: "Workstation x86_64",
      enabled: true,
      must_contain: "Workstation",
      must_not_contain: "beta",
      use_regex: false,
      episode_filter: null,
      affected_feeds: ["feed-1"],
      category: "linux-isos",
      tags: ["fedora"],
      save_path: null,
      add_paused: false,
      last_match: Math.floor(Date.now() / 1000) - 86400,
    },
  ];

  const searchPlugins = [
    { name: "linuxtracker", display_name: "LinuxTracker", version: "1.2", enabled: true, categories: ["software"], url: "https://linuxtracker.org" },
    { name: "internetarchive", display_name: "Internet Archive", version: "3.0", enabled: true, categories: ["all"], url: "https://archive.org" },
  ];

  const searchResults = [
    { name: "ubuntu-24.04.2-desktop-amd64.iso", download_link: "magnet:?xt=urn:btih:aaa", size: 5.9 * GiB, seeders: 1430, leechers: 102, plugin: "linuxtracker", description_link: "https://example.org/u", pub_date: null },
    { name: "ubuntu-24.04.2-live-server-amd64.iso", download_link: "magnet:?xt=urn:btih:bbb", size: 2.6 * GiB, seeders: 880, leechers: 64, plugin: "linuxtracker", description_link: null, pub_date: null },
    { name: "Ubuntu Studio 24.04 collection", download_link: "magnet:?xt=urn:btih:ccc", size: 4.1 * GiB, seeders: 96, leechers: 12, plugin: "internetarchive", description_link: null, pub_date: null },
  ];

  let searchStatus = "running";

  const handlers: Record<string, (args: any) => any> = {
    // Core lifecycle
    get_stored_download_dir: () => "/Users/jasper/Downloads",
    get_default_download_dir: () => "/Users/jasper/Downloads",
    init_engine: () => null,

    // Torrent list + stats
    get_torrents: () => torrents,
    get_global_stats: () => ({
      download_rate: torrents.reduce((a, t) => a + t.download_rate, 0),
      upload_rate: torrents.reduce((a, t) => a + t.upload_rate, 0),
      total_downloaded: 14.2 * GiB,
      total_uploaded: 9.8 * GiB,
      active_torrents: torrents.filter((t) => t.download_rate + t.upload_rate > 0).length,
      total_peers: torrents.reduce((a, t) => a + t.peers, 0),
      global_connections: 51,
    }),

    // Torrent actions (mutate the mock list so the harness feels live)
    pause_torrent: ({ infoHash }: any) => {
      const t = torrents.find((t) => t.info_hash === infoHash);
      if (t) {
        t.state = t.progress >= 100 ? "pausedUP" : "pausedDL";
        t.download_rate = 0;
        t.upload_rate = 0;
      }
      return null;
    },
    resume_torrent: ({ infoHash }: any) => {
      const t = torrents.find((t) => t.info_hash === infoHash);
      if (t) t.state = t.progress >= 100 ? "uploading" : "downloading";
      return null;
    },
    remove_torrent: ({ infoHash }: any) => {
      torrents = torrents.filter((t) => t.info_hash !== infoHash);
      return null;
    },
    add_magnet: () => "new-magnet-hash-000000000000000000000000",
    add_torrent_bytes: () => "new-torrent-hash-00000000000000000000000",

    // Detail panel
    get_torrent_trackers: ({ infoHash }: any) => trackers(infoHash),
    get_torrent_peers: () => peers,
    get_torrent_files: () => files,
    get_torrent_tags: () => ["iso", "lts"],
    get_torrent_share_limits: () => ({ max_ratio: 2.0, max_seeding_time: null, limit_action: "pause" }),
    get_torrent_ratio: () => 1.45,
    get_torrent_seeding_time: () => 2 * 86400 + 3600 * 5,
    get_file_priorities: () => [4, 4, 4],
    get_sequential_download: () => false,
    set_sequential_download: () => true,
    set_file_priority: () => true,
    add_torrent_tag: () => true,
    remove_torrent_tag: () => true,
    set_torrent_category: () => true,
    set_torrent_share_limits: () => true,

    // Settings
    get_queue_settings: () => [5, 5],
    set_queue_settings: () => null,
    get_bandwidth_limits: () => [4 * MiB, 1 * MiB],
    set_bandwidth_limits: () => null,
    get_no_seed_mode: () => false,
    set_no_seed_mode: () => null,
    get_disconnect_on_complete: () => false,
    set_disconnect_on_complete: () => null,
    get_categories: () => categories,
    add_category: () => null,
    remove_category: () => true,
    get_watch_folders: () => [
      { id: "wf-1", path: "/Users/jasper/Downloads/watch", category: "linux-isos", tags: [], process_existing: false, enabled: true },
    ],
    add_watch_folder: () => "wf-new",
    remove_watch_folder: () => true,
    get_auto_tracker_settings: () => ({ enabled: false, trackers: [] }),
    set_auto_tracker_settings: () => null,
    get_move_on_complete_settings: () => ({ enabled: false, target_path: null, use_category_path: false }),
    set_move_on_complete_settings: () => null,
    get_external_program_settings: () => ({ on_completion_enabled: false, on_completion_command: null }),
    set_external_program_settings: () => null,

    // Add-torrent previews
    parse_magnet: ({ uri }: any) => ({
      info_hash: "a94a8fe5ccb19ba61c4c0873d391e987982fbbd3",
      display_name: decodeURIComponent((uri.match(/dn=([^&]+)/) || [])[1] ?? "ubuntu-24.04.2-desktop-amd64.iso"),
      trackers: ["https://torrent.ubuntu.com/announce"],
    }),

    // RSS
    get_rss_feeds: () => rssFeeds,
    add_rss_feed: () => "feed-new",
    remove_rss_feed: () => true,
    refresh_rss_feed: () => null,
    get_rss_feed_items: () => rssItems,
    get_rss_rules: () => rssRules,
    add_rss_rule: () => "rule-new",
    remove_rss_rule: () => true,

    // Search
    load_search_plugins: () => searchPlugins.length,
    get_search_plugins: () => searchPlugins,
    install_search_plugin: () => searchPlugins[0],
    remove_search_plugin: () => null,
    set_search_plugin_enabled: () => true,
    start_search: () => {
      searchStatus = "running";
      setTimeout(() => (searchStatus = "completed"), 2500);
      return "search-1";
    },
    stop_search: () => true,
    delete_search: () => true,
    get_search_status: () => ({
      id: "search-1",
      query: "ubuntu",
      plugins: searchPlugins.map((p) => p.name),
      category: null,
      status: searchStatus,
      results_count: searchResults.length,
      error: null,
    }),
    get_search_results: () => searchResults,

    // Tauri plugin surface the app touches
    "plugin:window|set_theme": () => null,
    "plugin:window|theme": () => "dark",
    "plugin:dialog|open": () => null, // behave like a cancelled picker
    "plugin:updater|check": () => {
      throw new Error("updater unavailable in browser mock");
    },
  };

  let callbackId = 0;
  (window as any).__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
    },
    transformCallback: (cb?: (r: any) => void) => {
      const id = ++callbackId;
      (window as any)[`_${id}`] = cb;
      return id;
    },
    invoke: async (cmd: string, args?: any) => {
      const handler = handlers[cmd];
      if (!handler) {
        console.warn(`[tauri-mock] unhandled command: ${cmd}`, args);
        throw new Error(`mock: unhandled command ${cmd}`);
      }
      return handler(args ?? {});
    },
  };

  console.info(`[tauri-mock] installed (scenario: ${scenario})`);
}

if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  installMock();
}
