/**
 * Local notification store.
 *
 * ## Why this is not a server-backed notification centre
 *
 * This application is an offline, single-user desktop utility: there is no
 * server, database, account, session, or multi-user data, and `AGENTS.md`
 * forbids introducing one. So there is nothing to persist server-side, no
 * WebSocket/SSE transport to subscribe to, and no cross-user access to
 * authorise. Notifications here are derived **only** from events this app
 * genuinely produces — device connect/disconnect/Trust, scan results, transfer
 * outcomes, destination picks, and startup prerequisite failures.
 *
 * Nothing is fabricated: no placeholder rows, no invented counts, no sample
 * data. If an event did not happen, no notification exists.
 *
 * ## Design rules
 *
 *  - **Persisted locally**, because a desktop user expects the bell to still
 *    hold yesterday's failed transfer after a restart. The storage is
 *    `localStorage`, the same mechanism the app already uses for the chosen
 *    destination — no new dependency and no database.
 *  - **Validated on read.** Persisted JSON is untrusted input (a user, another
 *    program, or an older build can rewrite it). `parseNotifications` validates
 *    every field and drops malformed records instead of trusting the shape, and
 *    bounds the array so a poisoned payload cannot exhaust memory.
 *  - **Never sensitive.** Bodies carry typed, UI-safe messages only. Source
 *    file contents and full source paths are never stored (matching the
 *    project's logging policy). Text is length-capped and control characters
 *    are stripped as defence in depth.
 *  - **Pure core.** Every function here is pure so the behaviour that matters —
 *    unread counts, de-duplication, dismissal, eviction, persistence — is
 *    unit-testable without a DOM.
 */

import { formatBytes } from "./format";
import type { FinishedPayload } from "./types";

export type NotificationSeverity = "info" | "success" | "warning" | "error";

/**
 * Only the categories this app can actually produce. A category with no event
 * source would be a fake feature, so security/account/system categories from
 * the generic list are deliberately absent: there are no accounts and no
 * server to raise them.
 */
export type NotificationCategory = "device" | "scan" | "transfer" | "destination" | "setup";

/** In-app resource a notification navigates to when activated. */
export type NotificationTarget = "device" | "files" | "destination";

export interface AppNotification {
  id: string;
  severity: NotificationSeverity;
  category: NotificationCategory;
  title: string;
  body: string | null;
  /** Epoch milliseconds. */
  createdAt: number;
  read: boolean;
  /** How many identical events collapsed into this row. */
  count: number;
  /** Stable identity for collapsing repeats; null when events are unique. */
  dedupeKey: string | null;
  target: NotificationTarget | null;
}

/** What a caller supplies; the store fills in id/time/read state. */
export interface NotificationInput {
  severity: NotificationSeverity;
  category: NotificationCategory;
  title: string;
  body?: string | null;
  dedupeKey?: string | null;
  target?: NotificationTarget | null;
}

/** Rows kept before the oldest are evicted. */
export const NOTIFICATION_LIMIT = 50;

/** Longest title/body accepted from any source. */
export const MAX_TITLE_LENGTH = 120;
export const MAX_BODY_LENGTH = 400;

export type NotificationFilter = "all" | "unread";

/**
 * Category scoping for the panel. `"any"` is the default: the user has not
 * narrowed anything. Kept separate from `NotificationFilter` because the two
 * are orthogonal — "unread transfer activity" is a meaningful view.
 */
export type NotificationCategoryFilter = NotificationCategory | "any";

/** User-facing names for the categories. The panel labels the filter with these. */
export const CATEGORY_LABELS: Record<NotificationCategory, string> = {
  device: "Device",
  scan: "Scanning",
  transfer: "Transfers",
  destination: "Destination",
  setup: "Setup",
};

export const STORAGE_KEY = "lit-notifications";

const SEVERITIES: readonly NotificationSeverity[] = ["info", "success", "warning", "error"];
const CATEGORIES: readonly NotificationCategory[] = [
  "device",
  "scan",
  "transfer",
  "destination",
  "setup",
];
const TARGETS: readonly NotificationTarget[] = ["device", "files", "destination"];

export function isSeverity(value: unknown): value is NotificationSeverity {
  return typeof value === "string" && (SEVERITIES as readonly string[]).includes(value);
}

export function isCategory(value: unknown): value is NotificationCategory {
  return typeof value === "string" && (CATEGORIES as readonly string[]).includes(value);
}

export function isTarget(value: unknown): value is NotificationTarget {
  return typeof value === "string" && (TARGETS as readonly string[]).includes(value);
}

export function isFilter(value: unknown): value is NotificationFilter {
  return value === "all" || value === "unread";
}

export function isCategoryFilter(value: unknown): value is NotificationCategoryFilter {
  return value === "any" || isCategory(value);
}

/**
 * Collapse anything that reaches a notification body into a single bounded,
 * control-character-free line. Bodies are rendered as text by React (never as
 * HTML), so this is about layout stability and defence in depth rather than
 * escaping.
 */
export function sanitizeText(value: unknown, maxLength: number): string {
  if (typeof value !== "string") return "";
  // Strip C0/C1 control characters, including newlines, so a message can never
  // break out of its single-line slot.
  // eslint-disable-next-line no-control-regex
  const flattened = value.replace(/[\u0000-\u001f\u007f-\u009f]+/g, " ").trim();
  const collapsed = flattened.replace(/\s{2,}/g, " ");
  return collapsed.length > maxLength ? `${collapsed.slice(0, maxLength - 1)}…` : collapsed;
}

let idCounter = 0;

/** Unique-enough id without a dependency: time plus a monotonic counter. */
export function newNotificationId(now: number = Date.now()): string {
  idCounter += 1;
  return `n-${now.toString(36)}-${idCounter.toString(36)}`;
}

/**
 * Insert a notification, newest first.
 *
 * Repeats collapse: when `dedupeKey` matches the newest **unread** row of the
 * same key, that row's `count` increments and its timestamp refreshes instead of
 * a second row appearing. If the matching row was already read, the user has
 * acknowledged that event, so a fresh unread row is created — otherwise a
 * recurring problem would silently stay marked as handled.
 */
export function addNotification(
  list: readonly AppNotification[],
  input: NotificationInput,
  options: { now?: number; id?: string } = {},
): AppNotification[] {
  const now = options.now ?? Date.now();
  const title = sanitizeText(input.title, MAX_TITLE_LENGTH);
  const body = sanitizeText(input.body ?? "", MAX_BODY_LENGTH);
  const dedupeKey = input.dedupeKey ?? null;

  const base = {
    severity: input.severity,
    category: input.category,
    title,
    body: body.length > 0 ? body : null,
    read: false,
    dedupeKey,
    target: input.target ?? null,
  };

  if (dedupeKey !== null) {
    const existingIndex = list.findIndex((n) => n.dedupeKey === dedupeKey && !n.read);
    if (existingIndex !== -1) {
      const existing = list[existingIndex];
      const merged: AppNotification = {
        ...existing,
        ...base,
        id: existing.id,
        createdAt: now,
        read: false,
        count: existing.count + 1,
      };
      return [merged, ...list.filter((_, i) => i !== existingIndex)].slice(0, NOTIFICATION_LIMIT);
    }
  }

  const created: AppNotification = {
    ...base,
    id: options.id ?? newNotificationId(now),
    createdAt: now,
    count: 1,
  };
  return [created, ...list].slice(0, NOTIFICATION_LIMIT);
}

/** Newest first, always. */
export function byNewest(list: readonly AppNotification[]): AppNotification[] {
  return [...list].sort((a, b) => b.createdAt - a.createdAt);
}

export function unreadCount(list: readonly AppNotification[]): number {
  return list.reduce((total, n) => total + (n.read ? 0 : 1), 0);
}

export function markRead(list: readonly AppNotification[], id: string): AppNotification[] {
  return list.map((n) => (n.id === id ? { ...n, read: true } : n));
}

export function markUnread(list: readonly AppNotification[], id: string): AppNotification[] {
  return list.map((n) => (n.id === id ? { ...n, read: false } : n));
}

export function markAllRead(list: readonly AppNotification[]): AppNotification[] {
  return list.map((n) => (n.read ? n : { ...n, read: true }));
}

export function dismissNotification(list: readonly AppNotification[], id: string): AppNotification[] {
  return list.filter((n) => n.id !== id);
}

export function clearAll(): AppNotification[] {
  return [];
}

export function filterNotifications(
  list: readonly AppNotification[],
  filter: NotificationFilter,
  category: NotificationCategoryFilter = "any",
): AppNotification[] {
  const scoped = category === "any" ? list : list.filter((n) => n.category === category);
  return filter === "unread" ? scoped.filter((n) => !n.read) : byNewest(scoped);
}

/**
 * The categories actually present in the store, in a stable order.
 *
 * Drives the panel's category picker: offering a category that has no rows
 * would just be a control that always shows an empty list, so the picker only
 * ever lists what the user could actually filter down to.
 */
export function presentCategories(list: readonly AppNotification[]): NotificationCategory[] {
  const seen = new Set(list.map((n) => n.category));
  return CATEGORIES.filter((category) => seen.has(category));
}

/**
 * The newest unread error/warning, shown inline as an `role="alert"` banner so
 * a failure is visible without opening the panel. It is the *same record* the
 * panel shows — dismissing the banner marks that record read, so the two views
 * can never disagree.
 */
export function newestUnseenProblem(list: readonly AppNotification[]): AppNotification | null {
  for (const n of list) {
    if (!n.read && (n.severity === "error" || n.severity === "warning")) return n;
  }
  return null;
}

/** Serialize for persistence. Never throws. */
export function serializeNotifications(list: readonly AppNotification[]): string {
  try {
    return JSON.stringify({ version: 1, notifications: byNewest(list).slice(0, NOTIFICATION_LIMIT) });
  } catch {
    return JSON.stringify({ version: 1, notifications: [] });
  }
}

function parseEntry(raw: unknown): AppNotification | null {
  if (typeof raw !== "object" || raw === null) return null;
  const value = raw as Record<string, unknown>;

  if (typeof value.id !== "string" || value.id.length === 0 || value.id.length > 80) return null;
  if (!isSeverity(value.severity)) return null;
  if (!isCategory(value.category)) return null;
  if (typeof value.title !== "string") return null;

  const title = sanitizeText(value.title, MAX_TITLE_LENGTH);
  if (title.length === 0) return null;

  const bodyText = sanitizeText(value.body, MAX_BODY_LENGTH);
  const createdAt = typeof value.createdAt === "number" && Number.isFinite(value.createdAt)
    ? value.createdAt
    : 0;
  const count =
    typeof value.count === "number" && Number.isFinite(value.count) && value.count >= 1
      ? Math.min(Math.floor(value.count), 9_999)
      : 1;

  return {
    id: value.id,
    severity: value.severity,
    category: value.category,
    title,
    body: bodyText.length > 0 ? bodyText : null,
    createdAt,
    read: value.read === true,
    count,
    dedupeKey: typeof value.dedupeKey === "string" && value.dedupeKey.length <= 120
      ? value.dedupeKey
      : null,
    target: isTarget(value.target) ? value.target : null,
  };
}

/**
 * Parse persisted JSON. Untrusted input: every field is validated and any
 * record that does not fit the shape is dropped rather than coerced. Malformed
 * JSON yields an empty list — a corrupt store must never break startup.
 */
export function parseNotifications(raw: string | null | undefined): AppNotification[] {
  if (typeof raw !== "string" || raw.length === 0) return [];
  let decoded: unknown;
  try {
    decoded = JSON.parse(raw);
  } catch {
    return [];
  }
  if (typeof decoded !== "object" || decoded === null) return [];
  const container = decoded as Record<string, unknown>;
  const entries = Array.isArray(container.notifications)
    ? container.notifications
    : Array.isArray(decoded)
      ? (decoded as unknown[])
      : null;
  if (!entries) return [];

  const parsed: AppNotification[] = [];
  const seenIds = new Set<string>();
  // Bound the work, not just the result: a hostile payload could otherwise be
  // arbitrarily long.
  for (const entry of entries.slice(0, NOTIFICATION_LIMIT * 4)) {
    const notification = parseEntry(entry);
    if (!notification) continue;
    if (seenIds.has(notification.id)) continue;
    seenIds.add(notification.id);
    parsed.push(notification);
    if (parsed.length >= NOTIFICATION_LIMIT) break;
  }
  return byNewest(parsed);
}

/**
 * Map a finished transfer run onto the notification it should raise.
 *
 * Kept here rather than inline in the component for two reasons: it is the one
 * piece of notification logic with real branching (failed vs interrupted vs
 * cancelled vs clean), so it needs to be unit-testable; and the byte count is
 * formatted with the app's own formatter instead of being interpolated raw, so
 * a notification never reads "48219632 bytes" while the on-screen summary says
 * "48.2 MB".
 *
 * Dedupe keys are per outcome, so repeatedly hitting the same fault collapses
 * into one row, while a clean success never masks an earlier failure.
 */
export function transferOutcomeNotification(finished: FinishedPayload): NotificationInput {
  const parts = [
    `${finished.completed} copied`,
    finished.bytes_copied > 0 ? formatBytes(finished.bytes_copied) : null,
    finished.skipped > 0 ? `${finished.skipped} skipped` : null,
    finished.failed > 0 ? `${finished.failed} failed` : null,
    finished.interrupted > 0 ? `${finished.interrupted} interrupted` : null,
  ].filter((part): part is string => part !== null);
  const detail = parts.join(", ");

  if (finished.error) {
    return {
      severity: "error",
      category: "transfer",
      title: "Transfer stopped",
      body: `${finished.error.message} (${detail})`,
      target: "files",
      dedupeKey: "transfer:stopped",
    };
  }

  if (finished.cancelled) {
    return {
      severity: "warning",
      category: "transfer",
      title: "Transfer cancelled",
      body: detail,
      target: "files",
      dedupeKey: "transfer:cancelled",
    };
  }

  if (finished.failed > 0 || finished.interrupted > 0) {
    return {
      severity: "warning",
      category: "transfer",
      title:
        finished.failed > 0
          ? `Transfer finished with ${finished.failed} file${finished.failed === 1 ? "" : "s"} failed`
          : "Transfer finished with interrupted files",
      body: `${finished.message} (${detail})`,
      target: "files",
      dedupeKey: "transfer:partial",
    };
  }

  return {
    severity: "success",
    category: "transfer",
    title: finished.completed > 0 ? "Transfer complete" : "Nothing to transfer",
    body: detail,
    target: "files",
  };
}

// ---- reducer --------------------------------------------------------------

export type NotificationAction =
  | { type: "add"; input: NotificationInput; now?: number; id?: string }
  | { type: "markRead"; id: string }
  | { type: "markUnread"; id: string }
  | { type: "markAllRead" }
  | { type: "dismiss"; id: string }
  | { type: "clearAll" };

export function notificationReducer(
  state: readonly AppNotification[],
  action: NotificationAction,
): AppNotification[] {
  switch (action.type) {
    case "add":
      return addNotification(state, action.input, { now: action.now, id: action.id });
    case "markRead":
      return markRead(state, action.id);
    case "markUnread":
      return markUnread(state, action.id);
    case "markAllRead":
      return markAllRead(state);
    case "dismiss":
      return dismissNotification(state, action.id);
    case "clearAll":
      return clearAll();
    default:
      return [...state];
  }
}
