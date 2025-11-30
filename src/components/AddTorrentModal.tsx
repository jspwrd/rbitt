import { Icons } from "./Icons";
import { formatBytes, formatDate } from "../utils";
import type { PendingTorrent, MagnetInfo } from "../types";

interface AddTorrentModalProps {
  pendingTorrents: PendingTorrent[];
  selectedPendingIndex: number;
  onSelectPending: (index: number) => void;
  magnetUri: string;
  onMagnetChange: (uri: string) => void;
  magnetInfo: MagnetInfo | null;
  loading: boolean;
  onClose: () => void;
  onOpenFile: () => void;
  onParseMagnet: () => void;
  onAddFromFile: () => void;
  onAddFromMagnet: () => void;
}

export function AddTorrentModal({
  pendingTorrents,
  selectedPendingIndex,
  onSelectPending,
  magnetUri,
  onMagnetChange,
  magnetInfo,
  loading,
  onClose,
  onOpenFile,
  onParseMagnet,
  onAddFromFile,
  onAddFromMagnet,
}: AddTorrentModalProps) {
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Add Torrent</h2>
          <button className="modal-close" onClick={onClose}>
            &times;
          </button>
        </div>
        <div className="modal-content">
          <div className="add-section">
            <h3>
              <Icons.Folder /> From File
            </h3>
            <button className="btn-secondary btn-full" onClick={onOpenFile}>
              Select .torrent files...
            </button>

            {pendingTorrents.length > 0 && (
              <div className="torrent-preview-multi">
                <div className="torrent-list-panel">
                  <div className="torrent-list-header">
                    Torrents to Add ({pendingTorrents.length})
                  </div>
                  <div className="torrent-list-items">
                    {pendingTorrents.map((torrent, index) => (
                      <div
                        key={index}
                        className={`torrent-list-item ${
                          selectedPendingIndex === index ? "selected" : ""
                        }`}
                        onClick={() => onSelectPending(index)}
                      >
                        <span className="torrent-list-name" title={torrent.info.name}>
                          {torrent.info.name}
                        </span>
                        <span className="torrent-list-size">
                          {formatBytes(torrent.info.total_size)}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
                <div className="torrent-detail-panel">
                  {pendingTorrents[selectedPendingIndex] && (
                    <>
                      <h4>{pendingTorrents[selectedPendingIndex].info.name}</h4>
                      <div className="preview-grid">
                        <span>Info Hash:</span>
                        <span className="monospace">
                          {pendingTorrents[selectedPendingIndex].info.info_hash}
                        </span>
                        <span>Size:</span>
                        <span>
                          {formatBytes(pendingTorrents[selectedPendingIndex].info.total_size)}
                        </span>
                        <span>Files:</span>
                        <span>{pendingTorrents[selectedPendingIndex].info.file_count}</span>
                        <span>Pieces:</span>
                        <span>
                          {pendingTorrents[selectedPendingIndex].info.piece_count} x{" "}
                          {formatBytes(pendingTorrents[selectedPendingIndex].info.piece_length)}
                        </span>
                        <span>Version:</span>
                        <span>{pendingTorrents[selectedPendingIndex].info.version}</span>
                        <span>Private:</span>
                        <span>
                          {pendingTorrents[selectedPendingIndex].info.is_private ? "Yes" : "No"}
                        </span>
                        {pendingTorrents[selectedPendingIndex].info.created_by && (
                          <>
                            <span>Created By:</span>
                            <span>{pendingTorrents[selectedPendingIndex].info.created_by}</span>
                          </>
                        )}
                        {pendingTorrents[selectedPendingIndex].info.creation_date && (
                          <>
                            <span>Created:</span>
                            <span>
                              {formatDate(pendingTorrents[selectedPendingIndex].info.creation_date!)}
                            </span>
                          </>
                        )}
                        {pendingTorrents[selectedPendingIndex].info.comment && (
                          <>
                            <span>Comment:</span>
                            <span>{pendingTorrents[selectedPendingIndex].info.comment}</span>
                          </>
                        )}
                      </div>
                      {pendingTorrents[selectedPendingIndex].info.trackers.length > 0 && (
                        <details className="preview-trackers">
                          <summary>
                            Trackers ({pendingTorrents[selectedPendingIndex].info.trackers.length})
                          </summary>
                          <ul>
                            {pendingTorrents[selectedPendingIndex].info.trackers.map(
                              (tracker, i) => (
                                <li key={i} className="tracker-item">
                                  {tracker}
                                </li>
                              )
                            )}
                          </ul>
                        </details>
                      )}

                      {pendingTorrents[selectedPendingIndex].info.files.length > 0 && (
                        <details className="preview-files">
                          <summary>
                            Files ({pendingTorrents[selectedPendingIndex].info.files.length})
                          </summary>
                          <ul>
                            {pendingTorrents[selectedPendingIndex].info.files
                              .slice(0, 10)
                              .map((file, i) => (
                                <li key={i}>
                                  <span className="file-path">{file.path}</span>
                                  <span className="file-size">{formatBytes(file.size)}</span>
                                </li>
                              ))}
                            {pendingTorrents[selectedPendingIndex].info.files.length > 10 && (
                              <li className="more-files">
                                ...and{" "}
                                {pendingTorrents[selectedPendingIndex].info.files.length - 10} more
                                files
                              </li>
                            )}
                          </ul>
                        </details>
                      )}
                    </>
                  )}
                </div>
                <div className="torrent-add-actions">
                  <button
                    className="btn-primary btn-full"
                    onClick={onAddFromFile}
                    disabled={loading}
                  >
                    {loading
                      ? "Adding..."
                      : `Add ${pendingTorrents.length} Torrent${
                          pendingTorrents.length > 1 ? "s" : ""
                        }`}
                  </button>
                </div>
              </div>
            )}
          </div>

          <div className="add-divider">
            <span>OR</span>
          </div>

          <div className="add-section">
            <h3>
              <Icons.Link /> From Magnet Link
            </h3>
            <div className="magnet-input">
              <input
                type="text"
                value={magnetUri}
                onChange={(e) => onMagnetChange(e.target.value)}
                placeholder="magnet:?xt=urn:btih:..."
              />
              <button
                className="btn-secondary"
                onClick={onParseMagnet}
                disabled={loading || !magnetUri}
              >
                Parse
              </button>
            </div>

            {magnetInfo && (
              <div className="magnet-preview">
                <h4>{magnetInfo.display_name || "Unknown"}</h4>
                <div className="preview-grid">
                  <span>Info Hash:</span>
                  <span className="monospace">{magnetInfo.info_hash}</span>
                </div>
                {magnetInfo.trackers.length > 0 && (
                  <details>
                    <summary>Trackers ({magnetInfo.trackers.length})</summary>
                    <ul>
                      {magnetInfo.trackers.map((tracker, i) => (
                        <li key={i}>{tracker}</li>
                      ))}
                    </ul>
                  </details>
                )}
                <button
                  className="btn-primary btn-full"
                  onClick={onAddFromMagnet}
                  disabled={loading}
                >
                  {loading ? "Adding..." : "Add Magnet"}
                </button>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default AddTorrentModal;
