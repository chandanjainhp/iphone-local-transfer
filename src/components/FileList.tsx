import { CircleX, Folder, Image as ImageIcon, LoaderCircle, RefreshCw } from "lucide-react";
import { FileRow } from "./FileRow";
import type { Category, MediaItem, SortOrder } from "../types";

interface FileListProps {
  /** null until the first scan has run. */
  media: MediaItem[] | null;
  /** Files in the current category filter, in the current sort order. */
  visible: MediaItem[];
  category: Category;
  selected: Set<string>;
  udid: string | null;
  scanning: boolean;
  transferRunning: boolean;
  deviceReady: boolean;
  scanError: string | null;
  sort: SortOrder;
  onSort: (sort: SortOrder) => void;
  onScan: () => void;
  onToggle: (id: string) => void;
  onSelectAll: () => void;
  onClear: () => void;
  onPreview: (item: MediaItem) => void;
}

/** "Available files" section: heading, selection controls, sorting, list. */
export function FileList(props: FileListProps) {
  const {
    media,
    visible,
    category,
    selected,
    udid,
    scanning,
    transferRunning,
    deviceReady,
    scanError,
    sort,
    onSort,
    onScan,
    onToggle,
    onSelectAll,
    onClear,
    onPreview,
  } = props;
  const busy = scanning || transferRunning;

  const countLine = media
    ? `${visible.length} of ${media.length} accessible file${media.length === 1 ? "" : "s"} · ${
        sort === "newest" ? "newest first" : "oldest first"
      }`
    : "Choose the items you want to copy.";

  return (
    <>
      <div className="section-heading">
        <div>
          <h2>Available Files</h2>
          <p>{countLine}</p>
        </div>
        <div className="heading-actions">
          {media && visible.length > 0 && (
            <>
              <button
                type="button"
                className="ghost-button"
                onClick={onSelectAll}
                disabled={busy}
              >
                Select All
              </button>
              <button
                type="button"
                className="ghost-button"
                onClick={onClear}
                disabled={busy || selected.size === 0}
              >
                Clear
              </button>
            </>
          )}
          <label className="sort-select" title="Sort order">
            <select
              value={sort}
              onChange={(e) => onSort(e.target.value as SortOrder)}
              disabled={!media || busy}
              aria-label="Sort order"
            >
              <option value="newest">Newest First</option>
              <option value="oldest">Oldest First</option>
            </select>
          </label>
        </div>
      </div>

      {renderList()}
    </>
  );

  function renderList() {
    if (busy && !media) {
      return (
        <section className="empty-state">
          <div className="empty-icon">
            <LoaderCircle size={27} className="spin" strokeWidth={1.5} />
          </div>
          <h3>{scanning ? "Reading Your Library…" : "Transferring…"}</h3>
          <p>Please keep your iPhone unlocked and connected.</p>
        </section>
      );
    }
    if (scanError) {
      return (
        <section className="empty-state error">
          <div className="empty-icon">
            <CircleX size={27} strokeWidth={1.5} />
          </div>
          <h3>Could Not Read Your Photos</h3>
          <p>{scanError}</p>
          <button type="button" className="secondary-button" onClick={onScan}>
            <RefreshCw size={14} /> Try Again
          </button>
        </section>
      );
    }
    if (!media) {
      return (
        <section className="empty-state">
          <div className="empty-icon">
            <Folder size={27} strokeWidth={1.5} />
          </div>
          <h3>{deviceReady ? "Ready to Scan" : "Nothing to Show Yet"}</h3>
          <p>
            {deviceReady
              ? "Scan your iPhone to list the photos and videos it exposes."
              : "Connect your iPhone to load accessible photos and videos."}
          </p>
          {deviceReady ? (
            <button type="button" className="secondary-button" onClick={onScan}>
              <RefreshCw size={14} /> Scan Now
            </button>
          ) : null}
          <span className="scope-note">
            Only files exposed by iOS are available. Private app and system files are not
            accessible.
          </span>
        </section>
      );
    }
    if (visible.length === 0) {
      return (
        <section className="empty-state">
          <div className="empty-icon">
            <ImageIcon size={27} strokeWidth={1.5} />
          </div>
          <h3>
            No{" "}
            {category === "Photos"
              ? "Photos"
              : category === "Videos"
                ? "Videos"
                : "Photos or Videos"}
          </h3>
          <p>
            {media.length === 0
              ? "iOS did not expose any photos or videos over USB. iCloud Photos may still be uploading — try again later."
              : "Nothing matches this category."}
          </p>
          <span className="scope-note">
            Only files exposed by iOS are available. Private app and system files are not
            accessible.
          </span>
        </section>
      );
    }

    return (
      <section
        className="media-list"
        role="listbox"
        aria-label="Files on your iPhone"
        aria-multiselectable
      >
        {visible.map((item) => (
          <FileRow
            key={item.id}
            item={item}
            checked={selected.has(item.id)}
            busy={busy}
            udid={udid}
            onToggle={() => onToggle(item.id)}
            onPreview={() => onPreview(item)}
          />
        ))}
      </section>
    );
  }
}
