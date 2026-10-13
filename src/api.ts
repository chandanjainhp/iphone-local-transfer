import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ConflictPolicy,
  DeviceSnapshot,
  Diagnostics,
  EnvReport,
  MediaItem,
  MockDeviceState,
  TransferEvent,
} from "./types";

/** True when running inside the Tauri desktop shell (not a plain browser). */
export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>)
  );
}

export async function getDiagnostics(): Promise<Diagnostics> {
  return invoke<Diagnostics>("diagnostics");
}

/**
 * One-shot startup prerequisite check: tools present, USB service reachable,
 * iPhone detected, trust approved. Read-only — never installs or starts
 * anything.
 */
export async function checkEnvironment(): Promise<EnvReport> {
  return invoke<EnvReport>("check_environment");
}

export async function getDeviceStatus(): Promise<DeviceSnapshot> {
  return invoke<DeviceSnapshot>("get_device_status");
}

export async function listMedia(): Promise<MediaItem[]> {
  return invoke<MediaItem[]>("list_media");
}

/**
 * Preview thumbnail for a listed file as a data URL, or `null` when the
 * source exposes no preview for it.
 */
export async function getMediaPreview(id: string): Promise<string | null> {
  return invoke<string | null>("get_media_preview", { id });
}

export async function pickDestination(): Promise<string | null> {
  return invoke<string | null>("pick_destination");
}

export async function startTransfer(
  items: string[],
  destination: string,
  conflict: ConflictPolicy,
  /** "Transfer all new": skip files already copied to the destination. */
  newOnly = false,
): Promise<void> {
  // Tauri 2 matches command arguments by lowerCamelCase key.
  return invoke<void>("start_transfer", { items, destination, conflict, newOnly });
}

export async function cancelTransfer(): Promise<void> {
  return invoke<void>("cancel_transfer");
}

export async function setMockMode(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_mock_mode", { enabled });
}

export async function setMockDeviceState(value: MockDeviceState): Promise<void> {
  return invoke<void>("set_mock_device_state", { value });
}

export function onDeviceStatus(cb: (snapshot: DeviceSnapshot) => void) {
  return listen<DeviceSnapshot>("device_status", (event) => cb(event.payload));
}

export function onTransferEvent(cb: (event: TransferEvent) => void) {
  return listen<TransferEvent>("transfer_event", (event) => cb(event.payload));
}

/** Extract a user-facing message from an invoke rejection. */
export function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    const e = err as { message?: unknown };
    if (typeof e.message === "string" && e.message.length > 0) return e.message;
  }
  if (typeof err === "string") return err;
  return "Something went wrong. Please try again.";
}
