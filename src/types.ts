// Types mirroring the Rust DTOs (serde snake_case) — keep in sync with
// src-tauri/src/{device,media,transfer}.rs and errors.rs.

export type DeviceState = "no_device" | "needs_trust" | "ready" | "unsupported" | "error";

export interface DeviceInfo {
  udid: string;
  name: string | null;
  model: string | null;
  ios_version: string | null;
}

export interface DeviceSnapshot {
  state: DeviceState;
  device: DeviceInfo | null;
  message: string;
  hint: string | null;
  /** True only for the development-only mock adapter. */
  demo: boolean;
  source: string;
}

export type MediaKind = "photo" | "video";

export interface MediaItem {
  id: string;
  name: string;
  kind: MediaKind;
  /** null when the source does not expose a size — never invented. */
  size: number | null;
  /** Epoch milliseconds, or null when not exposed. */
  modified_ms: number | null;
  source_path: string;
}

export interface AppError {
  code: string;
  message: string;
}

export type ConflictPolicy = "skip" | "rename" | "replace";
export type FileOutcome = "completed" | "skipped" | "failed" | "interrupted";

/** Sidebar navigation filter. */
export type Category = "All Media" | "Photos" | "Videos";

/** File-list ordering; the newest file first, or the oldest. */
export type SortOrder = "newest" | "oldest";

export interface ProgressPayload {
  index: number;
  total_files: number;
  name: string;
  file_done: number;
  file_total: number | null;
  overall_done: number;
  overall_total: number | null;
  speed_bps: number | null;
  avg_bps: number | null;
  eta_secs: number | null;
}

export interface FileFinishedPayload {
  index: number;
  total_files: number;
  name: string;
  outcome: FileOutcome;
  saved_as: string | null;
  bytes: number;
  error: AppError | null;
  /** Why the backend skipped the file (e.g. "Already transferred"). */
  note: string | null;
}

export interface FinishedPayload {
  completed: number;
  skipped: number;
  failed: number;
  interrupted: number;
  bytes_copied: number;
  cancelled: boolean;
  error: AppError | null;
  message: string;
}

export type TransferEvent =
  | { type: "started"; total_files: number; total_bytes: number | null }
  | { type: "file_started"; index: number; total_files: number; name: string }
  | ({ type: "progress" } & ProgressPayload)
  | ({ type: "file_finished" } & FileFinishedPayload)
  | ({ type: "finished" } & FinishedPayload);

export interface ToolReport {
  idevice_id: string | null;
  ideviceinfo: string | null;
  idevicepair: string | null;
  afcclient: string | null;
}

export interface Diagnostics {
  os: "linux" | "macos" | "windows" | "other";
  tools: ToolReport;
  missing_tools: string[];
  toolchain_ready: boolean;
  install_hint: string;
  mock_available: boolean;
}

export type MockDeviceState = "no_device" | "needs_trust" | "ready";

// ---- startup environment check (mirrors environment.rs) -------------------

/** Overall prerequisite state the setup screen branches on. */
export type EnvStatus =
  | "ready"
  | "device_not_connected"
  | "dependency_missing"
  | "service_not_running"
  | "permission_required"
  | "pairing_required"
  | "unsupported_platform"
  | "error";

export type ServiceState =
  | "running"
  | "not_installed"
  | "not_running"
  | "permission_denied"
  | "unsupported";

export type PairState = "paired" | "needs_trust" | "no_device" | "unknown";

export interface EnvCheck {
  id: string;
  label: string;
  ok: boolean;
  /** True when an earlier failure made this check impossible to run. */
  skipped: boolean;
  detail: string | null;
}

export interface EnvReport {
  status: EnvStatus;
  message: string;
  hint: string | null;
  code: string | null;
  os: string;
  tools: ToolReport;
  missing_tools: string[];
  service: ServiceState;
  service_detail: string;
  device_udid: string | null;
  pairing: PairState | null;
  checks: EnvCheck[];
  install_hint: string;
  mock_available: boolean;
}
