import { Icons } from "./Icons";
import { formatSpeed, isDownloading, isUploading, isActive } from "../utils";
import type { TorrentStatus, GlobalStats } from "../types";

interface StatusBarProps {
  torrents: TorrentStatus[];
  globalStats: GlobalStats | null;
}

export function StatusBar({ torrents, globalStats }: StatusBarProps) {
  const localStats = {
    totalDownload: torrents.reduce((sum, t) => sum + t.download_rate, 0),
    totalUpload: torrents.reduce((sum, t) => sum + t.upload_rate, 0),
    downloading: torrents.filter((t) => isDownloading(t.state)).length,
    seeding: torrents.filter((t) => isUploading(t.state)).length,
    active: torrents.filter((t) => isActive(t.state)).length,
    total: torrents.length,
  };

  return (
    <div className="status-bar">
      <div className="status-section">
        <span className="status-item">
          <Icons.Download />
          <span>{formatSpeed(globalStats?.download_rate ?? localStats.totalDownload)}</span>
        </span>
        <span className="status-item">
          <Icons.Upload />
          <span>{formatSpeed(globalStats?.upload_rate ?? localStats.totalUpload)}</span>
        </span>
      </div>
      <div className="status-section">
        <span className="status-item">Downloading: {localStats.downloading}</span>
        <span className="status-item">Seeding: {localStats.seeding}</span>
        <span className="status-item">Total: {localStats.total}</span>
      </div>
      <div className="status-section">
        <span className="status-item">
          <Icons.Peers />
          <span>Peers: {globalStats?.total_peers ?? 0}</span>
        </span>
        <span className="status-item connection">
          <span className="connection-dot connected" />
          DHT: Ready
        </span>
      </div>
    </div>
  );
}

export default StatusBar;
