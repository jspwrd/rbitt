// Utility functions for formatting and state checking

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KiB", "MiB", "GiB", "TiB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export function formatSpeed(bytesPerSecond: number): string {
  return formatBytes(bytesPerSecond) + "/s";
}

export function formatDate(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString();
}

export function formatEta(downloaded: number, total: number, rate: number): string {
  if (rate === 0) return "\u221e";
  const remaining = total - downloaded;
  const seconds = remaining / rate;
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)}h`;
  return `${Math.round(seconds / 86400)}d`;
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)}h`;
  return `${Math.round(seconds / 86400)}d`;
}

// State checking functions
export function isDownloading(state: string): boolean {
  return [
    "downloading",
    "forcedDL",
    "metaDL",
    "forcedMetaDL",
    "checkingDL",
    "pausedDL",
    "stoppedDL",
    "allocating",
    "checkingResumeData",
  ].includes(state);
}

export function isUploading(state: string): boolean {
  return [
    "uploading",
    "forcedUP",
    "checkingUP",
    "pausedUP",
    "stoppedUP",
  ].includes(state);
}

export function isStalledDownloading(state: string): boolean {
  return state === "stalledDL";
}

export function isStalledUploading(state: string): boolean {
  return state === "stalledUP";
}

export function isStalled(state: string): boolean {
  return state === "stalledDL" || state === "stalledUP";
}

export function isPaused(state: string): boolean {
  return ["pausedDL", "pausedUP", "stoppedDL", "stoppedUP"].includes(state);
}

export function isChecking(state: string): boolean {
  return ["checkingDL", "checkingUP", "checkingResumeData"].includes(state);
}

export function isActive(state: string): boolean {
  return ["downloading", "uploading", "forcedDL", "forcedUP", "metaDL", "forcedMetaDL"].includes(state);
}

export function isError(state: string): boolean {
  return state === "error" || state === "missingFiles";
}

export function isCompleted(state: string): boolean {
  return state === "completed";
}

export function isQueued(state: string): boolean {
  return state === "queuedDL" || state === "queuedUP";
}

export function getStateColor(state: string): string {
  switch (state) {
    case "downloading":
    case "forcedDL":
      return "var(--state-downloading)";
    case "uploading":
    case "forcedUP":
      return "var(--state-seeding)";
    case "stalledDL":
    case "stalledUP":
      return "var(--state-stalled)";
    case "completed":
      return "var(--state-completed)";
    case "pausedDL":
    case "pausedUP":
    case "stoppedDL":
    case "stoppedUP":
      return "var(--state-paused)";
    case "checkingDL":
    case "checkingUP":
    case "checkingResumeData":
      return "var(--state-checking)";
    case "queuedDL":
    case "queuedUP":
      return "var(--state-queued)";
    case "metaDL":
    case "forcedMetaDL":
      return "var(--state-metadata)";
    case "allocating":
      return "var(--state-checking)";
    case "moving":
      return "var(--state-moving)";
    case "missingFiles":
    case "error":
      return "var(--state-error)";
    default:
      return "var(--text-secondary)";
  }
}

export function formatState(state: string): string {
  const stateLabels: Record<string, string> = {
    downloading: "Downloading",
    uploading: "Seeding",
    stalledDL: "Stalled",
    stalledUP: "Stalled",
    completed: "Completed",
    pausedDL: "Paused",
    pausedUP: "Paused",
    stoppedDL: "Paused",
    stoppedUP: "Paused",
    checkingDL: "Checking",
    checkingUP: "Checking",
    checkingResumeData: "Checking",
    queuedDL: "Queued",
    queuedUP: "Queued",
    metaDL: "Fetching metadata",
    forcedDL: "[F] Downloading",
    forcedUP: "[F] Seeding",
    forcedMetaDL: "[F] Fetching metadata",
    moving: "Moving",
    missingFiles: "Missing files",
    error: "Error",
    allocating: "Allocating",
    unknown: "Unknown",
  };
  return stateLabels[state] || state;
}
