import { useMemo, useState } from "react";
import { Icons } from "./Icons";
import {
  formatBytes,
  formatSpeed,
  formatEta,
  formatState,
  isDownloading,
} from "../utils";
import type { TorrentStatus, TorrentState } from "../types";

type SortKey = "active" | "name" | "size" | "progress";

const SORT_STORAGE_KEY = "torrent-sort";

const SORT_LABELS: Record<SortKey, string> = {
  active: "Activity",
  name: "Name",
  size: "Size",
  progress: "Progress",
};

function getStoredSort(): SortKey {
  const stored = localStorage.getItem(SORT_STORAGE_KEY);
  return stored === "name" || stored === "size" || stored === "progress"
    ? stored
    : "active";
}

/** Pill style + whether its dot pulses, per state. */
function pillFor(state: TorrentState): { cls: string; pulse: boolean } {
  switch (state) {
    case "downloading":
    case "forcedDL":
      return { cls: "pill-downloading", pulse: true };
    case "uploading":
    case "forcedUP":
      return { cls: "pill-seeding", pulse: true };
    case "metaDL":
    case "forcedMetaDL":
      return { cls: "pill-metadata", pulse: true };
    case "checkingDL":
    case "checkingUP":
    case "checkingResumeData":
    case "allocating":
      return { cls: "pill-checking", pulse: true };
    case "moving":
      return { cls: "pill-moving", pulse: true };
    case "stalledDL":
    case "stalledUP":
      return { cls: "pill-stalled", pulse: false };
    case "completed":
      return { cls: "pill-completed", pulse: false };
    case "queuedDL":
    case "queuedUP":
      return { cls: "pill-queued", pulse: false };
    case "error":
    case "missingFiles":
      return { cls: "pill-error", pulse: false };
    default:
      return { cls: "pill-paused", pulse: false };
  }
}

function fillFor(state: TorrentState): string {
  switch (state) {
    case "completed":
      return "fill-completed";
    case "uploading":
    case "forcedUP":
    case "stalledUP":
      return "fill-seeding";
    case "stalledDL":
      return "fill-stalled";
    case "error":
    case "missingFiles":
      return "fill-error";
    case "checkingDL":
    case "checkingUP":
    case "checkingResumeData":
      return "fill-checking";
    case "pausedDL":
    case "pausedUP":
    case "stoppedDL":
    case "stoppedUP":
    case "queuedDL":
    case "queuedUP":
      return "fill-paused";
    default:
      return "";
  }
}

function metaLine(torrent: TorrentStatus): string {
  const parts: string[] = [];
  if (torrent.total_size > 0) {
    parts.push(formatBytes(torrent.total_size));
  }
  if (torrent.state === "metaDL" || torrent.state === "forcedMetaDL") {
    parts.push("fetching metadata from swarm");
    return parts.join(" · ");
  }
  parts.push(`${torrent.seeds} seeds`);
  parts.push(`${torrent.peers} peers`);
  if (
    isDownloading(torrent.state) &&
    torrent.download_rate > 0 &&
    torrent.total_size > 0
  ) {
    parts.push(
      `${formatEta(torrent.downloaded, torrent.total_size, torrent.download_rate)} left`,
    );
  }
  return parts.join(" · ");
}

function sortTorrents(torrents: TorrentStatus[], key: SortKey): TorrentStatus[] {
  const byName = (a: TorrentStatus, b: TorrentStatus) =>
    a.name.localeCompare(b.name);

  const sorted = [...torrents];
  switch (key) {
    case "name":
      sorted.sort(byName);
      break;
    case "size":
      sorted.sort((a, b) => b.total_size - a.total_size || byName(a, b));
      break;
    case "progress":
      sorted.sort((a, b) => b.progress - a.progress || byName(a, b));
      break;
    case "active":
      // Transferring first (fastest on top), then incomplete, then by name.
      // The name tiebreak keeps the list stable across status polls.
      sorted.sort((a, b) => {
        const rateA = a.download_rate + a.upload_rate;
        const rateB = b.download_rate + b.upload_rate;
        if (rateA !== rateB) return rateB - rateA;
        if (a.progress !== b.progress) return a.progress - b.progress;
        return byName(a, b);
      });
      break;
  }
  return sorted;
}

interface TorrentListProps {
  torrents: TorrentStatus[];
  selectedTorrent: string | null;
  onSelect: (infoHash: string | null) => void;
  onDoubleClick: (torrent: TorrentStatus) => void;
  onAddClick: () => void;
}

export function TorrentList({
  torrents,
  selectedTorrent,
  onSelect,
  onDoubleClick,
  onAddClick,
}: TorrentListProps) {
  const [sortKey, setSortKey] = useState<SortKey>(getStoredSort);

  const sorted = useMemo(() => sortTorrents(torrents, sortKey), [torrents, sortKey]);

  const changeSort = (key: SortKey) => {
    setSortKey(key);
    localStorage.setItem(SORT_STORAGE_KEY, key);
  };

  const handleBackgroundClick = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (
      target.classList.contains("torrent-rows") ||
      target.classList.contains("torrent-list-container")
    ) {
      onSelect(null);
    }
  };

  if (torrents.length === 0) {
    return (
      <div className="torrent-list-container">
        <div className="empty-state">
          <Icons.Download />
          <h3>No torrents yet</h3>
          <p>Add a .torrent file or paste a magnet link to get started.</p>
          <button className="btn-primary" onClick={onAddClick}>
            <Icons.Add />
            Add torrent
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="torrent-list-container" onClick={handleBackgroundClick}>
      <div className="list-toolbar">
        <span className="list-count">
          {torrents.length === 1 ? "1 torrent" : `${torrents.length} torrents`}
        </span>
        <div className="sort-control">
          <span>Sort by</span>
          <select
            value={sortKey}
            onChange={(e) => changeSort(e.target.value as SortKey)}
          >
            {(Object.keys(SORT_LABELS) as SortKey[]).map((key) => (
              <option key={key} value={key}>
                {SORT_LABELS[key]}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="torrent-rows" onClick={handleBackgroundClick}>
        {sorted.map((torrent) => {
          const pill = pillFor(torrent.state);
          return (
            <div
              key={torrent.info_hash}
              className={`torrent-row ${
                selectedTorrent === torrent.info_hash ? "selected" : ""
              }`}
              tabIndex={0}
              onClick={() => onSelect(torrent.info_hash)}
              onDoubleClick={() => onDoubleClick(torrent)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect(torrent.info_hash);
                }
              }}
            >
              <div className="row-top">
                <span className="torrent-name" title={torrent.name}>
                  {torrent.name}
                </span>
                <span className="row-speeds">
                  {torrent.download_rate > 0 && (
                    <span className="speed-down">
                      ↓ {formatSpeed(torrent.download_rate)}
                    </span>
                  )}
                  {torrent.upload_rate > 0 && (
                    <span className="speed-up">
                      ↑ {formatSpeed(torrent.upload_rate)}
                    </span>
                  )}
                </span>
              </div>

              <div className="row-progress">
                <div className="progress-track">
                  <div
                    className={`progress-fill ${fillFor(torrent.state)}`}
                    style={{ width: `${Math.min(torrent.progress, 100)}%` }}
                  />
                </div>
                <span className="progress-pct">
                  {torrent.progress.toFixed(1)}%
                </span>
              </div>

              <div className="row-meta">
                <span className={`state-pill ${pill.cls} ${pill.pulse ? "pulse" : ""}`}>
                  <i className="pill-dot" />
                  {formatState(torrent.state)}
                </span>
                <span className="meta-text">{metaLine(torrent)}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default TorrentList;
