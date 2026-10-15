// Display helpers. Every one of them prints "—" instead of inventing a value.

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return "—";
  if (bytes < 1000) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1000;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`;
}

export function formatDate(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || Number.isNaN(ms)) return "Date not provided";
  try {
    return new Date(ms).toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  } catch {
    return "Date not provided";
  }
}

export function formatTime(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined || Number.isNaN(ms)) return null;
  try {
    return new Date(ms).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return null;
  }
}

/** Date and time together, e.g. "Oct 9, 2026, 04:54 PM". */
export function formatDateTime(ms: number | null | undefined): string {
  const date = formatDate(ms);
  const time = formatTime(ms);
  return time ? `${date}, ${time}` : date;
}

/**
 * Compact "time ago" for notifications, e.g. "Just now", "4 min", "3 h",
 * "2 days". Returns "—" for a missing timestamp rather than inventing one.
 *
 * `now` is a parameter so the result is deterministic in tests and so a
 * re-render with a fresher clock can update the label.
 */
export function formatRelativeTime(
  ms: number | null | undefined,
  now: number = Date.now(),
): string {
  if (ms === null || ms === undefined || Number.isNaN(ms) || ms <= 0) return "—";
  const delta = Math.max(0, now - ms);
  const seconds = Math.floor(delta / 1000);
  if (seconds < 45) return "Just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${Math.max(1, minutes)} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days} day${days === 1 ? "" : "s"}`;
  return formatDate(ms);
}

export function formatSpeed(bps: number | null | undefined): string {
  if (bps === null || bps === undefined || bps <= 0) return "—";
  return `${formatBytes(bps)}/s`;
}

export function formatEta(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || seconds < 0) return "—";
  if (seconds < 60) return `${seconds}s`;
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  if (mins < 60) return `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  return `${hours}h ${mins % 60}m`;
}
