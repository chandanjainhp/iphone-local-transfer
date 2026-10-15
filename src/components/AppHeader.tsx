import type { ReactNode } from "react";

interface AppHeaderProps {
  /** Connection label derived from the real backend snapshot. */
  connectionLabel: string;
  /** Device state is "ready". */
  connected: boolean;
  /** Device is waiting for the Trust prompt. */
  warn: boolean;
  /** The bell (and its open panel), supplied by the app. */
  notificationCenter: ReactNode;
}

export function AppHeader({
  connectionLabel,
  connected,
  warn,
  notificationCenter,
}: AppHeaderProps) {
  return (
    <header className="topbar">
      <div>
        <h1>Transfer Your Files</h1>
        <p className="subtitle">Move photos and videos from your iPhone to this computer.</p>
      </div>
      <div className="topbar-right">
        <div
          className={`connection-pill ${connected ? "connected" : ""} ${warn ? "warn" : ""}`}
          role="status"
        >
          <span className="status-dot" />
          {connectionLabel}
        </div>
        {notificationCenter}
      </div>
    </header>
  );
}
