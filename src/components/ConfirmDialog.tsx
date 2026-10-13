import { useEffect } from "react";
import { CheckCircle2 } from "lucide-react";

interface ConfirmDialogProps {
  /** Number of files the user is about to replace with. */
  count: number;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Explicit confirmation gate for `ConflictPolicy::Replace`. The backend never
 * overwrites on its own — this dialog is where the user says yes.
 */
export function ConfirmDialog({ count, onConfirm, onCancel }: ConfirmDialogProps) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div className="preview-overlay" role="presentation" onClick={onCancel}>
      <div
        className="confirm-card"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-body"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id="confirm-title">Replace Existing Files?</h2>
        <p id="confirm-body">
          {count} file{count === 1 ? "" : "s"} in the destination folder will be replaced. Files
          that are replaced cannot be recovered. Files that match an already-transferred copy are
          still skipped.
        </p>
        <div className="confirm-actions">
          <button type="button" className="ghost-button" onClick={onCancel} autoFocus>
            Cancel
          </button>
          <button type="button" className="primary-button" onClick={onConfirm}>
            <CheckCircle2 size={15} /> Replace Files
          </button>
        </div>
      </div>
    </div>
  );
}
