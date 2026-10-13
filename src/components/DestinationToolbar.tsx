import { ArrowDownToLine, Folder } from "lucide-react";
import type { ConflictPolicy } from "../types";

interface DestinationToolbarProps {
  destination: string | null;
  conflict: ConflictPolicy;
  running: boolean;
  selectedCount: number;
  canTransfer: boolean;
  canTransferAll: boolean;
  /** Why "Transfer selected" is disabled (null while enabled). */
  selectedDisabledReason: string | null;
  /** Why "Transfer all new" is disabled (null while enabled). */
  allDisabledReason: string | null;
  onChangeDestination: () => void;
  onConflict: (policy: ConflictPolicy) => void;
  onTransferSelected: () => void;
  onTransferAll: () => void;
}

/**
 * Action toolbar above the file list: destination, conflict policy, and the
 * transfer actions. Desktop guidance keeps critical actions out of bottom
 * bars, so this band sits directly under the device card.
 */
export function DestinationToolbar({
  destination,
  conflict,
  running,
  selectedCount,
  canTransfer,
  canTransferAll,
  selectedDisabledReason,
  allDisabledReason,
  onChangeDestination,
  onConflict,
  onTransferSelected,
  onTransferAll,
}: DestinationToolbarProps) {
  return (
    <div className="action-toolbar">
      <div className="destination">
        <Folder size={18} />
        <div>
          <span className="destination-label">Save To</span>
          <span className="destination-path" title={destination ?? undefined}>
            {destination ?? "Choose a destination folder"}
          </span>
        </div>
        <button
          type="button"
          className="change-button"
          onClick={onChangeDestination}
          title="Pick the folder files are copied into"
        >
          Change…
        </button>
      </div>

      <label className="conflict-select" title="What to do when a file already exists">
        <span>If It Exists</span>
        <select
          value={conflict}
          onChange={(e) => onConflict(e.target.value as ConflictPolicy)}
          disabled={running}
          aria-label="If a file already exists"
        >
          <option value="skip">Skip (Keep Both)</option>
          <option value="rename">Rename Incoming File</option>
          <option value="replace">Replace Existing File</option>
        </select>
      </label>

      <div className="transfer-actions">
        <button
          type="button"
          className="primary-button"
          disabled={!canTransfer || running}
          title={selectedDisabledReason ?? "Copy the selected files to the destination folder"}
          onClick={onTransferSelected}
        >
          <ArrowDownToLine size={17} />
          {running
            ? "Transferring…"
            : selectedCount > 0
              ? `Transfer Selected (${selectedCount})`
              : "Transfer Selected"}
        </button>
        <button
          type="button"
          className="text-button"
          disabled={!canTransferAll || running}
          title={allDisabledReason ?? "Copy every file that is not already in the destination folder"}
          onClick={onTransferAll}
        >
          Transfer All New
        </button>
      </div>
    </div>
  );
}
