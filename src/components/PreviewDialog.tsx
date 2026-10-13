import { useEffect } from "react";
import { Image as ImageIcon, LoaderCircle, Video, X } from "lucide-react";
import { usePreview } from "../previews";
import { formatBytes, formatDateTime } from "../format";
import type { MediaItem } from "../types";

interface PreviewDialogProps {
  item: MediaItem;
  udid: string | null;
  onClose: () => void;
}

/**
 * Enlarged preview with the file's date and time. Honest about gaps: no
 * preview means an icon and a plain sentence, never a fabricated image.
 */
export function PreviewDialog({ item, udid, onClose }: PreviewDialogProps) {
  const { url, loading } = usePreview(udid, item, true, { immediate: true });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="preview-overlay" role="presentation" onClick={onClose}>
      <div
        className="preview-card"
        role="dialog"
        aria-modal="true"
        aria-label={`Preview of ${item.name}`}
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          className="preview-close"
          onClick={onClose}
          aria-label="Close preview"
        >
          <X size={15} />
        </button>
        <div className={`preview-media ${url ? "" : "empty"}`}>
          {url ? (
            <img src={url} alt={item.name} />
          ) : loading ? (
            <LoaderCircle size={26} className="spin" />
          ) : (
            <>
              {item.kind === "photo" ? <ImageIcon size={28} /> : <Video size={28} />}
              <p>No preview is available for this file.</p>
            </>
          )}
        </div>
        <div className="preview-meta">
          <span className="preview-name">{item.name}</span>
          <span className="preview-facts">
            <span>{item.kind === "photo" ? "Photo" : "Video"}</span>
            <span>{formatBytes(item.size)}</span>
            <span>{formatDateTime(item.modified_ms)}</span>
          </span>
          {item.kind === "video" && url ? (
            <span className="preview-note">Preview shows a still frame from the video.</span>
          ) : null}
        </div>
      </div>
    </div>
  );
}
