import {
  AlertTriangle,
  CheckCircle2,
  HelpCircle,
  Loader2,
  Usb,
  XCircle,
} from "lucide-react";
import type { EnvReport } from "../types";

/**
 * First-run / startup setup screen.
 *
 * Renders one of the explicit prerequisite states returned by the Rust
 * environment checker. Copy is deliberately non-technical; the raw details
 * live in a collapsed diagnostics section and are never stack traces — only
 * the typed report.
 */
interface Props {
  report: EnvReport | null;
  /** True while a check is running. */
  checking: boolean;
  /** The check itself failed to run (invoke error). */
  error: string | null;
  onRecheck: () => void;
  /** "Open the app anyway" — the app stays usable for diagnostics/browsing. */
  onDismiss: () => void;
}

type Variant = {
  icon: React.ReactNode;
  title: string;
  body: string;
  primaryLabel: string;
};

export function SetupScreen({ report, checking, error, onRecheck, onDismiss }: Props) {
  const status = error ? "error" : (report?.status ?? "checking");

  const variant: Variant = (() => {
    switch (status) {
      case "checking":
        return {
          icon: <Loader2 className="spin" size={30} aria-hidden />,
          title: "Preparing your iPhone connection...",
          body: "Checking that this computer can talk to an iPhone over USB.",
          primaryLabel: "Check Again",
        };
      case "ready":
        return {
          icon: <CheckCircle2 className="setup-ok" size={30} aria-hidden />,
          title: "Your computer is ready.",
          body: "Connect your iPhone with a USB cable and unlock it to see your photos and videos here.",
          primaryLabel: "Continue",
        };
      case "dependency_missing":
        return {
          icon: <AlertTriangle className="setup-warn" size={30} aria-hidden />,
          title: "USB support needs setup.",
          body: report && report.missing_tools.length > 0
            ? `This app needs the free USB tools “${report.missing_tools.join(
                "”, “",
              )}” to talk to your iPhone. They are not installed on this computer yet.`
            : "The free USB tools this app uses to talk to your iPhone are not installed on this computer yet.",
          primaryLabel: "Check Again",
        };
      case "service_not_running":
      case "permission_required":
        return {
          icon: <Usb className="setup-warn" size={30} aria-hidden />,
          title: "iPhone connection service is not running.",
          body:
            status === "permission_required"
              ? "This user account is not allowed to reach the iPhone connection service. An administrator needs to grant access."
              : "Your computer's iPhone connection service is installed but not switched on, so a connected iPhone cannot be discovered.",
          primaryLabel: "Check Again",
        };
      case "pairing_required":
        return {
          icon: <HelpCircle className="setup-warn" size={30} aria-hidden />,
          title: "Unlock your iPhone and approve the Trust prompt.",
          body: "Your iPhone is connected, but it is waiting for you to approve this computer. Unlock the phone, tap Trust on its screen, then check again.",
          primaryLabel: "I've Approved It — Check Again",
        };
      case "device_not_connected":
        return {
          icon: <Usb className="setup-muted" size={30} aria-hidden />,
          title: "No iPhone detected.",
          body: "Connect your iPhone to this computer with a USB cable and unlock it. If the phone asks whether to trust this computer, tap Trust.",
          primaryLabel: "Check Again",
        };
      case "unsupported_platform":
        return {
          icon: <XCircle className="setup-warn" size={30} aria-hidden />,
          title: "This operating system is not supported yet.",
          body: report?.message ?? "This app is built and tested for Linux, macOS and Windows.",
          primaryLabel: "Check Again",
        };
      default:
        return {
          icon: <XCircle className="setup-warn" size={30} aria-hidden />,
          title: "The connection check needs attention.",
          body: error
            ? `The connection check could not run (${error}).`
            : (report?.message ?? "Something went wrong while checking this computer."),
          primaryLabel: "Check Again",
        };
    }
  })();

  const isChecking = status === "checking" && checking;
  const primaryIsRecheck = status !== "ready";

  return (
    <div className="setup-screen" role="status" aria-live="polite">
      <div className="setup-card">
        <div className="setup-icon">{variant.icon}</div>
        <h1 className="setup-title">{variant.title}</h1>
        <p className="setup-body">{variant.body}</p>

        {report?.hint && status !== "ready" && (
          <p className="setup-hint">{report.hint}</p>
        )}
        {status === "dependency_missing" && report && (
          <pre className="setup-command" tabIndex={0}>
            {report.install_hint}
          </pre>
        )}
        {status === "service_not_running" && report?.install_hint && (
          <pre className="setup-command" tabIndex={0}>
            {report.service_detail}
          </pre>
        )}

        {report && report.checks.length > 0 && (
          <ul className="setup-checks" aria-label="Requirement checks">
            {report.checks.map((check) => (
              <li key={check.id} className={check.ok ? "ok" : check.skipped ? "skipped" : "failed"}>
                <span className="setup-check-mark" aria-hidden>
                  {check.ok ? "✓" : check.skipped ? "—" : "✕"}
                </span>
                <span className="setup-check-label">
                  {check.label}
                  {check.detail && <em>{check.detail}</em>}
                </span>
              </li>
            ))}
          </ul>
        )}

        <div className="setup-actions">
          <button
            type="button"
            className="primary-button"
            onClick={onRecheck}
            disabled={checking}
          >
            {isChecking && <Loader2 className="spin" size={15} aria-hidden />}
            {primaryIsRecheck ? variant.primaryLabel : "Continue"}
          </button>
          {status !== "ready" && (
            <button type="button" className="setup-secondary" onClick={onDismiss}>
              Open the App Anyway
            </button>
          )}
        </div>

        <details className="setup-details">
          <summary>Details for support</summary>
          <pre>
            {JSON.stringify(
              {
                status: report?.status ?? (error ? "check_failed" : "checking"),
                os: report?.os,
                service: report?.service,
                service_detail: report?.service_detail,
                missing_tools: report?.missing_tools,
                tools: report?.tools,
                pairing: report?.pairing,
                device_detected: Boolean(report?.device_udid),
                error: error ?? undefined,
              },
              null,
              2,
            )}
          </pre>
        </details>
      </div>
    </div>
  );
}
