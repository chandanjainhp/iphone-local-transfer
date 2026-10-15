import { Icon } from "../icons";
import type { Diagnostics, DeviceSnapshot } from "../types";

interface DeviceCardProps {
  /** False outside the Tauri shell (plain browser preview). */
  native: boolean;
  scanning: boolean;
  /** Scanning or transferring — disables the Rescan button. */
  busy: boolean;
  device: DeviceSnapshot | null;
  diag: Diagnostics | null;
  onScan: () => void;
}

/**
 * Horizontal device card. Every piece of metadata (model, iOS version,
 * readiness) comes from the backend snapshot — nothing is invented here.
 */
export function DeviceCard({ native, scanning, busy, device, diag, onScan }: DeviceCardProps) {
  return <section className="connect-card">{content()}</section>;

  function content() {
    if (!native) {
      return (
        <>
          <div className="device-icon">
            <Icon name="device" size="hero" />
          </div>
          <div className="connect-copy">
            <h2>Browser Preview</h2>
            <p>
              USB detection and transfers only work in the desktop app. Run
              <code> npm run tauri dev</code> to use this app for real.
            </p>
            <div className="micro-status">
              <Icon name="usb" size="small" /> No files are sent anywhere, in any mode.
            </div>
          </div>
        </>
      );
    }
    if (scanning) {
      return (
        <>
          <div className="device-icon">
            <Icon name="spinner" size="hero" className="spin" />
          </div>
          <div className="connect-copy">
            <h2>Scanning Your Photo Library…</h2>
            <p>Reading the photos and videos iOS exposes over USB. This can take a moment.</p>
          </div>
        </>
      );
    }
    switch (device?.state) {
      case "ready":
        return (
          <>
            <div className="device-icon">
              <Icon name="device" size="hero" />
            </div>
            <div className="connect-copy">
              <h2>
                iPhone Is Ready
                {device.device?.model ? (
                  <span className="device-meta">
                    {" "}
                    {device.device.model}
                    {device.device.ios_version ? ` · iOS ${device.device.ios_version}` : ""}
                  </span>
                ) : null}
              </h2>
              <p>Choose the photos and videos you want to copy to this computer.</p>
              <div className="micro-status">
                <Icon name="usb" size="small" /> USB connection <span className="micro-divider">·</span> Nothing
                is uploaded anywhere
              </div>
            </div>
            <button
              type="button"
              className="secondary-button"
              onClick={onScan}
              disabled={busy}
              aria-label="Rescan the iPhone for photos and videos"
            >
              <Icon name="refresh" size="small" /> Rescan
            </button>
          </>
        );
      case "needs_trust":
        return (
          <>
            <div className="device-icon warn">
              <Icon name="device" size="hero" />
            </div>
            <div className="connect-copy">
              <h2>{device.message}</h2>
              <p>{device.hint ?? "Unlock your iPhone and tap Trust on the iPhone screen."}</p>
              <div className="micro-status">
                <Icon name="usb" size="small" /> Keep the cable connected while you do this.
              </div>
            </div>
          </>
        );
      case "unsupported":
      case "error":
        return (
          <>
            <div className="device-icon warn">
              <Icon name="error" size="hero" />
            </div>
            <div className="connect-copy">
              <h2>{device.message}</h2>
              {device.hint ? <p>{device.hint}</p> : null}
              {diag && !diag.toolchain_ready ? (
                <p className="tool-list">Missing: {diag.missing_tools.join(", ")}</p>
              ) : null}
            </div>
          </>
        );
      default:
        return (
          <>
            <div className="device-icon">
              <Icon name="device" size="hero" />
            </div>
            <div className="connect-copy">
              <h2>Connect Your iPhone</h2>
              <p>
                Plug in a USB cable, unlock your iPhone, and tap <strong>Trust</strong> if asked.
                Your files will appear here.
              </p>
              <div className="micro-status">
                <Icon name="usb" size="small" /> USB only <span className="micro-divider">·</span> No cloud or
                account required
              </div>
            </div>
            <div className="connect-state">
              <span className="state-ring">
                <Icon name="spinner" size="action" className="spin" />
              </span>
              <span>Waiting for device</span>
            </div>
          </>
        );
    }
  }
}
