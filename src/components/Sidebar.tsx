import { useState } from "react";
import {
  ArrowDownToLine,
  CircleHelp,
  Folder,
  Image as ImageIcon,
  PauseOctagon,
  Smartphone,
  Video,
} from "lucide-react";
import type { Category, Diagnostics, MockDeviceState } from "../types";

interface SidebarProps {
  category: Category;
  onCategory: (category: Category) => void;
  /** Real backend device state — the green dot is never hardcoded. */
  deviceReady: boolean;
  /** True only while the development mock adapter is active. */
  demoDevice: boolean;
  diag: Diagnostics | null;
  mockEnabled: boolean;
  mockState: MockDeviceState;
  onToggleMock: () => void;
  onMockState: (state: MockDeviceState) => void;
}

export function Sidebar({
  category,
  onCategory,
  deviceReady,
  demoDevice,
  diag,
  mockEnabled,
  mockState,
  onToggleMock,
  onMockState,
}: SidebarProps) {
  const [showHelp, setShowHelp] = useState(false);

  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-mark">
          <ArrowDownToLine size={18} />
        </span>
        <span>Local Transfer</span>
      </div>

      <div className="side-label">Device</div>
      <div className="nav-item selected">
        <Smartphone size={17} />
        <span>{demoDevice ? "Demo iPhone" : "iPhone"}</span>
        <span
          className={`status-dot ${deviceReady ? "on" : ""}`}
          role="img"
          aria-label={deviceReady ? "Connected" : "Not connected"}
          title={deviceReady ? "Connected" : "Not connected"}
        />
      </div>

      <div className="side-label section-label">Library</div>
      {(["All Media", "Photos", "Videos"] as Category[]).map((cat) => (
        <button
          key={cat}
          type="button"
          className={`nav-item ${category === cat ? "active" : ""}`}
          aria-current={category === cat ? "page" : undefined}
          onClick={() => onCategory(cat)}
        >
          {cat === "All Media" ? (
            <Folder size={17} />
          ) : cat === "Photos" ? (
            <ImageIcon size={17} />
          ) : (
            <Video size={17} />
          )}
          <span>{cat}</span>
        </button>
      ))}

      <div className="sidebar-bottom">
        {diag?.mock_available ? (
          <div className="demo-block">
            <button type="button" className="help-button" onClick={onToggleMock}>
              <PauseOctagon size={16} />
              {mockEnabled ? "Turn Off Demo Mode" : "Demo Mode"}
            </button>
            {mockEnabled && (
              <div className="demo-states">
                {(["ready", "needs_trust", "no_device"] as MockDeviceState[]).map((s) => (
                  <button
                    key={s}
                    type="button"
                    className={`demo-chip ${mockState === s ? "active" : ""}`}
                    onClick={() => onMockState(s)}
                  >
                    {s === "ready" ? "Connected" : s === "needs_trust" ? "Needs Trust" : "No Device"}
                  </button>
                ))}
              </div>
            )}
          </div>
        ) : null}
        <button
          type="button"
          className="help-button"
          onClick={() => setShowHelp(!showHelp)}
          aria-expanded={showHelp}
        >
          <CircleHelp size={17} /> Help &amp; Connection
        </button>
        {showHelp && (
          <p className="help-copy">
            Connect your iPhone with a data-capable USB cable. Unlock it and accept the Trust
            prompt if shown. Everything stays on this computer.
          </p>
        )}
      </div>
    </aside>
  );
}
