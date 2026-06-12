import { Icons } from "./Icons";
import { isPaused } from "../utils";
import type { TorrentStatus } from "../types";

interface ToolbarProps {
  selectedTorrentData: TorrentStatus | undefined;
  onAdd: () => void;
  onResume: () => void;
  onPause: () => void;
  onRemove: () => void;
  onSettings: () => void;
}

export function Toolbar({
  selectedTorrentData,
  onAdd,
  onResume,
  onPause,
  onRemove,
  onSettings,
}: ToolbarProps) {
  const canResume = selectedTorrentData && isPaused(selectedTorrentData.state);
  const canPause = selectedTorrentData && !isPaused(selectedTorrentData.state);
  const canRemove = !!selectedTorrentData;

  return (
    // data-tauri-drag-region: the toolbar doubles as the window titlebar
    // (titleBarStyle Overlay); empty areas drag the window, buttons still
    // receive clicks since the attribute only applies to the element itself.
    <div className="toolbar" data-tauri-drag-region>
      <div className="toolbar-group">
        <button className="toolbar-btn" onClick={onAdd} title="Add Torrent">
          <Icons.Add />
          <span>Add</span>
        </button>
        <div className="toolbar-separator" />
        <button
          className="toolbar-btn"
          onClick={onResume}
          disabled={!canResume}
          title="Resume"
        >
          <Icons.Play />
        </button>
        <button
          className="toolbar-btn"
          onClick={onPause}
          disabled={!canPause}
          title="Pause"
        >
          <Icons.Pause />
        </button>
        <button
          className="toolbar-btn danger"
          onClick={onRemove}
          disabled={!canRemove}
          title="Remove"
        >
          <Icons.Delete />
        </button>
      </div>
      <div className="toolbar-group">
        <button className="toolbar-btn" onClick={onSettings} title="Settings">
          <Icons.Settings />
        </button>
      </div>
    </div>
  );
}

export default Toolbar;
