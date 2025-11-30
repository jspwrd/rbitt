import { Icons } from "./Icons";
import {
  formatBytes,
  formatSpeed,
  formatEta,
  formatState,
  getStateColor,
  isDownloading,
  isPaused,
  isChecking,
  isCompleted,
  isQueued,
  isError,
} from "../utils";
import type { TorrentStatus } from "../types";

interface TorrentListProps {
  torrents: TorrentStatus[];
  selectedTorrent: string | null;
  onSelect: (infoHash: string) => void;
  onDoubleClick: (torrent: TorrentStatus) => void;
  onAddClick: () => void;
}

function getStateIcon(state: string) {
  if (isError(state)) {
    return <Icons.Error />;
  }
  if (isChecking(state)) {
    return <Icons.Checking />;
  }
  if (isQueued(state)) {
    return <Icons.Queued />;
  }
  if (isCompleted(state)) {
    return <Icons.Completed />;
  }
  if (isPaused(state)) {
    return <Icons.Stopped />;
  }
  if (state === "stalledDL") {
    return <Icons.Downloading />;
  }
  if (state === "stalledUP") {
    return <Icons.Seeding />;
  }
  if (state === "metaDL" || state === "forcedMetaDL") {
    return <Icons.Downloading />;
  }
  if (state === "uploading" || state === "forcedUP") {
    return <Icons.Seeding />;
  }
  if (state === "downloading" || state === "forcedDL") {
    return <Icons.Downloading />;
  }
  return <Icons.Downloading />;
}

export function TorrentList({
  torrents,
  selectedTorrent,
  onSelect,
  onDoubleClick,
  onAddClick,
}: TorrentListProps) {
  return (
    <div className="torrent-list-container">
      <table className="torrent-table">
        <thead>
          <tr>
            <th className="col-name">Name</th>
            <th className="col-size">Size</th>
            <th className="col-progress">Progress</th>
            <th className="col-status">Status</th>
            <th className="col-seeds">Seeds</th>
            <th className="col-peers">Peers</th>
            <th className="col-down">Down Speed</th>
            <th className="col-up">Up Speed</th>
            <th className="col-eta">ETA</th>
          </tr>
        </thead>
        <tbody>
          {torrents.length === 0 ? (
            <tr className="empty-row">
              <td colSpan={9}>
                <div className="empty-state">
                  <p>No torrents</p>
                  <button className="btn-primary" onClick={onAddClick}>
                    Add your first torrent
                  </button>
                </div>
              </td>
            </tr>
          ) : (
            torrents.map((torrent) => (
              <tr
                key={torrent.info_hash}
                className={selectedTorrent === torrent.info_hash ? "selected" : ""}
                onClick={() => onSelect(torrent.info_hash)}
                onDoubleClick={() => onDoubleClick(torrent)}
              >
                <td className="col-name">
                  <div className="torrent-name-cell">
                    <span
                      className={`state-icon ${isChecking(torrent.state) ? "spinning" : ""}`}
                      style={{ color: getStateColor(torrent.state) }}
                    >
                      {getStateIcon(torrent.state)}
                    </span>
                    <span className="torrent-name" title={torrent.name}>
                      {torrent.name}
                    </span>
                  </div>
                </td>
                <td className="col-size">{formatBytes(torrent.total_size)}</td>
                <td className="col-progress">
                  <div className="progress-cell">
                    <div className="progress-bar-container">
                      <div
                        className="progress-bar"
                        style={{
                          width: `${torrent.progress}%`,
                          backgroundColor: getStateColor(torrent.state),
                        }}
                      />
                    </div>
                    <span className="progress-text">{torrent.progress.toFixed(1)}%</span>
                  </div>
                </td>
                <td className="col-status" style={{ color: getStateColor(torrent.state) }}>
                  {formatState(torrent.state)}
                </td>
                <td className="col-seeds">{torrent.seeds}</td>
                <td className="col-peers">{torrent.peers}</td>
                <td className="col-down">
                  {torrent.download_rate > 0 ? formatSpeed(torrent.download_rate) : "-"}
                </td>
                <td className="col-up">
                  {torrent.upload_rate > 0 ? formatSpeed(torrent.upload_rate) : "-"}
                </td>
                <td className="col-eta">
                  {isDownloading(torrent.state) && torrent.download_rate > 0 && torrent.total_size > 0
                    ? formatEta(torrent.downloaded, torrent.total_size, torrent.download_rate)
                    : "-"}
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}

export default TorrentList;
