import { Icons } from "./Icons";
import {
  isDownloading,
  isUploading,
  isCompleted,
  isPaused,
  isQueued,
  isChecking,
  isStalledDownloading,
  isStalledUploading,
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

export function Sidebar({ torrents, filter, onFilterChange, onRssClick, onSearchClick }: SidebarProps) {
  return (
    <div className="sidebar">
      <div className="sidebar-section">
        <div className="sidebar-header">Status</div>
        <button
          className={`sidebar-item ${filter === "all" ? "active" : ""}`}
          onClick={() => onFilterChange("all")}
        >
          <Icons.All />
          <span>All</span>
          <span className="sidebar-count">{torrents.length}</span>
        </button>
        <button
          className={`sidebar-item ${filter === "downloading" ? "active" : ""}`}
          onClick={() => onFilterChange("downloading")}
        >
          <Icons.Downloading />
          <span>Downloading</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isDownloading(t.state)).length}
          </span>
        </button>
        <button
          className={`sidebar-item ${filter === "seeding" ? "active" : ""}`}
          onClick={() => onFilterChange("seeding")}
        >
          <Icons.Seeding />
          <span>Seeding</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isUploading(t.state) && !isCompleted(t.state)).length}
          </span>
        </button>
        <button
          className={`sidebar-item ${filter === "completed" ? "active" : ""}`}
          onClick={() => onFilterChange("completed")}
        >
          <Icons.Completed />
          <span>Completed</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isCompleted(t.state)).length}
          </span>
        </button>
        <button
          className={`sidebar-item ${filter === "paused" ? "active" : ""}`}
          onClick={() => onFilterChange("paused")}
        >
          <Icons.Paused />
          <span>Paused</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isPaused(t.state) && !isCompleted(t.state)).length}
          </span>
        </button>
        <button
          className={`sidebar-item ${filter === "queued" ? "active" : ""}`}
          onClick={() => onFilterChange("queued")}
        >
          <Icons.Queued />
          <span>Queued</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isQueued(t.state)).length}
          </span>
        </button>
        <button
          className={`sidebar-item ${filter === "checking" ? "active" : ""}`}
          onClick={() => onFilterChange("checking")}
        >
          <Icons.Checking />
          <span>Checking</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isChecking(t.state)).length}
          </span>
        </button>
        <button
          className={`sidebar-item ${filter === "stalledDL" ? "active" : ""}`}
          onClick={() => onFilterChange("stalledDL")}
        >
          <Icons.Downloading />
          <span>Stalled DL</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isStalledDownloading(t.state)).length}
          </span>
        </button>
        <button
          className={`sidebar-item ${filter === "stalledUP" ? "active" : ""}`}
          onClick={() => onFilterChange("stalledUP")}
        >
          <Icons.Seeding />
          <span>Stalled UP</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isStalledUploading(t.state)).length}
          </span>
        </button>
        <button
          className={`sidebar-item ${filter === "active" ? "active" : ""}`}
          onClick={() => onFilterChange("active")}
        >
          <Icons.Downloading />
          <span>Active</span>
          <span className="sidebar-count">
            {torrents.filter((t) => isActive(t.state)).length}
          </span>
        </button>
      </div>

      <div className="sidebar-section">
        <div className="sidebar-header">Features</div>
        <button className="sidebar-item" onClick={onSearchClick}>
          <Icons.Search />
          <span>Search</span>
        </button>
        <button className="sidebar-item" onClick={onRssClick}>
          <Icons.Rss />
          <span>RSS</span>
        </button>
      </div>
    </div>
  );
}

export default Sidebar;
