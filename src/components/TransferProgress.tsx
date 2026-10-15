import { formatBytes, formatEta, formatSpeed } from "../format";
import { Icon } from "../icons";
import type { FileOutcome, FinishedPayload, ProgressPayload } from "../types";

export interface FileResult {
  name: string;
  outcome: FileOutcome;
  savedAs: string | null;
  /** Backend explanation for a skip (e.g. "Already transferred"). */
  note: string | null;
  error: string | null;
}

export interface TransferState {
  running: boolean;
  totalFiles: number;
  totalBytes: number | null;
  currentName: string | null;
  currentIndex: number;
  progress: ProgressPayload | null;
  results: FileResult[];
  finished: FinishedPayload | null;
}

export const EMPTY_TRANSFER: TransferState = {
  running: false,
  totalFiles: 0,
  totalBytes: null,
  currentName: null,
  currentIndex: 0,
  progress: null,
  results: [],
  finished: null,
};

interface TransferProgressProps {
  transfer: TransferState;
  onCancel: () => void;
}

/** Live progress: bars, current file, bytes, percent, speed, ETA, results. */
export function TransferProgress({ transfer, onCancel }: TransferProgressProps) {
  const progress = transfer.progress;
  const overallPercent =
    progress && progress.overall_total
      ? Math.min(100, Math.floor((progress.overall_done / progress.overall_total) * 100))
      : null;
  const filePercent =
    progress && progress.file_total
      ? Math.min(100, Math.floor((progress.file_done / progress.file_total) * 100))
      : null;

  return (
    <section className="progress-panel" aria-live="polite">
      <div className="progress-head">
        <div>
          <h2>
            Transferring {transfer.totalFiles} File{transfer.totalFiles === 1 ? "" : "s"}
          </h2>
          <p className="progress-file">
            {transfer.currentName
              ? `Copying ${transfer.currentName}${
                  progress?.file_total ? ` (${formatBytes(progress.file_total)})` : ""
                }${filePercent !== null ? ` · ${filePercent}%` : ""} · file ${
                  transfer.currentIndex + 1
                } of ${transfer.totalFiles}`
              : "Finishing up…"}
          </p>
        </div>
        <button type="button" className="cancel-button" onClick={onCancel}>
          <Icon name="cancel" size="small" /> Cancel
        </button>
      </div>

      <div className="progress-bar">
        <div
          className={`progress-fill ${overallPercent === null ? "indeterminate" : ""}`}
          style={overallPercent === null ? undefined : { width: `${overallPercent}%` }}
        />
      </div>

      {filePercent !== null && filePercent < 100 && (
        <div className="progress-bar thin">
          <div className="progress-fill file" style={{ width: `${filePercent}%` }} />
        </div>
      )}

      <dl className="progress-stats">
        <div>
          <dt>Copied</dt>
          <dd>
            {formatBytes(progress?.overall_done ?? 0)}
            {progress?.overall_total ? ` / ${formatBytes(progress.overall_total)}` : ""}
          </dd>
        </div>
        <div>
          <dt>Percent</dt>
          <dd>{overallPercent !== null ? `${overallPercent}%` : "—"}</dd>
        </div>
        <div>
          <dt>Speed</dt>
          <dd>{formatSpeed(progress?.speed_bps ?? null)}</dd>
        </div>
        <div>
          <dt>Average</dt>
          <dd>{formatSpeed(progress?.avg_bps ?? null)}</dd>
        </div>
        <div>
          <dt>Remaining</dt>
          <dd>
            {progress?.eta_secs !== null && progress?.eta_secs !== undefined
              ? formatEta(progress.eta_secs)
              : "Estimating…"}
          </dd>
        </div>
      </dl>

      {transfer.results.length > 0 && (
        <ul className="result-list">
          {transfer.results.slice(-4).map((r, i) => (
            <li key={`${r.name}-${i}`} className={`result ${r.outcome}`}>
              <OutcomeIcon outcome={r.outcome} />
              <span className="result-name">{r.name}</span>
              <span className="result-note">
                {r.outcome === "completed"
                  ? `Saved${r.savedAs && r.savedAs !== r.name ? ` as ${r.savedAs}` : ""}`
                  : r.outcome === "skipped"
                    ? (r.note ?? "Already exists — skipped")
                    : r.outcome === "interrupted"
                      ? "Interrupted"
                      : (r.error ?? "Failed")}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

interface TransferSummaryProps {
  finished: FinishedPayload;
  onDismiss: () => void;
}

/** Result panel after a run: message plus exact copied/skipped/failed counts. */
export function TransferSummary({ finished, onDismiss }: TransferSummaryProps) {
  const kind: "success" | "cancelled" | "error" = finished.error
    ? "error"
    : finished.cancelled
      ? "cancelled"
      : "success";

  return (
    <section
      className={`summary-panel ${kind}`}
      role={kind === "error" ? "alert" : "status"}
      aria-live={kind === "error" ? "assertive" : "polite"}
    >
      <div className="summary-icon">
        <Icon
          name={kind === "success" ? "success" : kind === "cancelled" ? "stopped" : "error"}
          size="large"
        />
      </div>
      <div className="summary-copy">
        <h2>
          {kind === "success"
            ? "Transfer Complete"
            : kind === "cancelled"
              ? "Transfer Cancelled"
              : "Transfer Stopped"}
        </h2>
        <p>{finished.message}</p>
        <p className="summary-stats">
          {finished.completed} copied · {formatBytes(finished.bytes_copied)}
          {finished.skipped > 0 ? ` · ${finished.skipped} skipped` : ""}
          {finished.failed > 0 ? ` · ${finished.failed} failed` : ""}
          {finished.interrupted > 0 ? ` · ${finished.interrupted} interrupted` : ""}
        </p>
      </div>
      <button type="button" className="secondary-button" onClick={onDismiss}>
        Done
      </button>
    </section>
  );
}

function OutcomeIcon({ outcome }: { outcome: FileOutcome }) {
  if (outcome === "completed") return <Icon name="success" size="small" />;
  if (outcome === "skipped") return <Icon name="skipped" size="small" />;
  if (outcome === "interrupted") return <Icon name="stopped" size="small" />;
  return <Icon name="error" size="small" />;
}
