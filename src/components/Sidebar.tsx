import type { ComponentType } from "react";
import { Icons } from "./Icons";
import {
  isDownloading,
  isUploading,
  isCompleted,
  isPaused,
  isQueued,
  isChecking,
  isStalled,
  isActive,
} from "../utils";
import type { TorrentStatus, FilterType } from "../types";

interface SidebarProps {
  torrents: TorrentStatus[];
  filter: FilterType;
  onFilterChange: (filter: FilterType) => void;
  onRssClick?: () => void;
  onSearchClick?: () => void;
}

interface FilterEntry {
  key: FilterType;
  label: string;
  icon: ComponentType;
  count: (torrents: TorrentStatus[]) => number;
}

const FILTERS: FilterEntry[] = [
  {
    key: "all",
    label: "All",
    icon: Icons.All,
    count: (t) => t.length,
  },
  {
    key: "downloading",
    label: "Downloading",
    icon: Icons.Downloading,
    count: (t) => t.filter((x) => isDownloading(x.state)).length,
  },
  {
    key: "seeding",
    label: "Seeding",
    icon: Icons.Seeding,
    count: (t) =>
      t.filter((x) => isUploading(x.state) && !isCompleted(x.state)).length,
  },
  {
    key: "completed",
    label: "Completed",
    icon: Icons.Completed,
    count: (t) => t.filter((x) => isCompleted(x.state)).length,
  },
  {
    key: "active",
    label: "Active",
    icon: Icons.Download,
    count: (t) => t.filter((x) => isActive(x.state)).length,
  },
  {
    key: "paused",
    label: "Paused",
    icon: Icons.Paused,
    count: (t) =>
      t.filter((x) => isPaused(x.state) && !isCompleted(x.state)).length,
  },
  {
    key: "queued",
    label: "Queued",
    icon: Icons.Queued,
    count: (t) => t.filter((x) => isQueued(x.state)).length,
  },
  {
    key: "stalled",
    label: "Stalled",
    icon: Icons.Stopped,
    count: (t) => t.filter((x) => isStalled(x.state)).length,
  },
  {
    key: "checking",
    label: "Checking",
    icon: Icons.Checking,
    count: (t) => t.filter((x) => isChecking(x.state)).length,
  },
];

export function Sidebar({
  torrents,
  filter,
  onFilterChange,
  onRssClick,
  onSearchClick,
}: SidebarProps) {
  return (
    <div className="sidebar">
      <div className="sidebar-section">
        <div className="sidebar-header">Status</div>
        {FILTERS.map((entry) => {
          const Icon = entry.icon;
          const count = entry.count(torrents);
          return (
            <button
              key={entry.key}
              className={`sidebar-item ${filter === entry.key ? "active" : ""}`}
              onClick={() => onFilterChange(entry.key)}
            >
              <Icon />
              <span>{entry.label}</span>
              <span className="sidebar-count">{count > 0 ? count : ""}</span>
            </button>
          );
        })}
      </div>

      <div className="sidebar-section">
        <div className="sidebar-header">Discover</div>
        <button className="sidebar-item" onClick={onSearchClick}>
          <Icons.Search />
          <span>Search</span>
        </button>
        <button className="sidebar-item" onClick={onRssClick}>
          <Icons.Rss />
          <span>RSS Feeds</span>
        </button>
      </div>
    </div>
  );
}

export default Sidebar;
