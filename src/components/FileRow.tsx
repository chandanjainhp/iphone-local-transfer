import { usePreview } from "../previews";
import { Icon } from "../icons";
import { formatBytes, formatDate, formatTime } from "../format";
import type { MediaItem } from "../types";

interface FileRowProps {
  item: MediaItem;
  checked: boolean;
  /** Scanning/transferring: rows stay read-only while the device is busy. */
  busy: boolean;
  udid: string | null;
  onToggle: () => void;
  onPreview: () => void;
}

/** One list row: checkbox, lazy thumbnail, name, size, date + time. */
export function FileRow({ item, checked, busy, udid, onToggle, onPreview }: FileRowProps) {
  // Thumbnails load only while the device is idle and the row is in view.
  const { url, loading, callbackRef } = usePreview(udid, item, !busy);
  const time = formatTime(item.modified_ms);
  return (
    <label className={`media-row ${checked ? "checked" : ""}`}>
      <input
        type="checkbox"
        checked={checked}
        onChange={onToggle}
        disabled={busy}
        aria-label={`Select ${item.name}`}
      />
      <button
        type="button"
        ref={callbackRef}
        className={`file-thumb ${item.kind}`}
        onClick={(e) => {
          // Open the preview instead of toggling the row's checkbox.
          e.preventDefault();
          e.stopPropagation();
          onPreview();
        }}
        aria-label={`Preview ${item.name}`}
        title={`Preview ${item.name}`}
      >
        {url ? (
          <img className="thumb-img" src={url} alt="" draggable={false} />
        ) : loading ? (
          <Icon name="spinner" size="small" className="spin" />
        ) : item.kind === "photo" ? (
          <Icon name="photos" size="small" />
        ) : (
          <Icon name="videos" size="small" />
        )}
        {item.kind === "video" && url ? (
          <span className="thumb-play" aria-hidden="true">
            <Icon name="play" size="badge" />
          </span>
        ) : null}
      </button>
      <span className="file-name" title={item.name}>
        {item.name}
      </span>
      <span className="file-size">{formatBytes(item.size)}</span>
      <span className="file-date">
        <span>{formatDate(item.modified_ms)}</span>
        {time ? <span className="file-time">{time}</span> : null}
      </span>
    </label>
  );
}
