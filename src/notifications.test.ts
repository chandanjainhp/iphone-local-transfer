/**
 * Unit tests for the notification store.
 *
 * These cover the behaviour that actually goes wrong in notification UIs:
 * wrong unread counts, duplicate rows from repeated events, notifications that
 * cannot be dismissed, records lost on reload, and a corrupt persisted payload
 * taking the app down at startup.
 */

import { describe, expect, it } from "vitest";
import {
  addNotification,
  byNewest,
  CATEGORY_LABELS,
  clearAll,
  dismissNotification,
  filterNotifications,
  isCategoryFilter,
  markAllRead,
  markRead,
  markUnread,
  MAX_BODY_LENGTH,
  MAX_TITLE_LENGTH,
  NOTIFICATION_LIMIT,
  newestUnseenProblem,
  notificationReducer,
  parseNotifications,
  presentCategories,
  sanitizeText,
  serializeNotifications,
  transferOutcomeNotification,
  unreadCount,
  type AppNotification,
  type NotificationInput,
} from "./notifications";
import type { FinishedPayload } from "./types";

const base: NotificationInput = {
  severity: "success",
  category: "transfer",
  title: "Transfer complete",
};

/** Build a list deterministically: explicit id and timestamp per row. */
function seed(count: number, overrides: Partial<NotificationInput> = {}) {
  let list: AppNotification[] = [];
  for (let i = 0; i < count; i += 1) {
    list = addNotification(
      list,
      { ...base, title: `event ${i}`, ...overrides },
      { now: 1_000 + i, id: `id-${i}` },
    );
  }
  return list;
}

describe("addNotification", () => {
  it("adds newest first and marks the row unread", () => {
    const list = seed(3);
    expect(list.map((n) => n.title)).toEqual(["event 2", "event 1", "event 0"]);
    expect(list.every((n) => !n.read)).toBe(true);
  });

  it("starts every new row with count 1", () => {
    expect(seed(1)[0].count).toBe(1);
  });

  it("collapses a repeat of the same event instead of duplicating it", () => {
    let list = addNotification([], { ...base, dedupeKey: "no-device" }, { now: 10, id: "a" });
    list = addNotification(list, { ...base, dedupeKey: "no-device" }, { now: 20, id: "b" });

    expect(list).toHaveLength(1);
    expect(list[0].count).toBe(2);
    expect(list[0].id).toBe("a");
    expect(list[0].createdAt).toBe(20);
  });

  it("keeps distinct dedupe keys as separate rows", () => {
    let list = addNotification([], { ...base, dedupeKey: "a" }, { now: 1, id: "1" });
    list = addNotification(list, { ...base, dedupeKey: "b" }, { now: 2, id: "2" });
    expect(list).toHaveLength(2);
  });

  it("creates a fresh unread row once the user has read the previous one", () => {
    let list = addNotification([], { ...base, dedupeKey: "trust" }, { now: 1, id: "a" });
    list = markRead(list, "a");
    list = addNotification(list, { ...base, dedupeKey: "trust" }, { now: 2, id: "b" });

    expect(list).toHaveLength(2);
    expect(unreadCount(list)).toBe(1);
    expect(list[0].id).toBe("b");
  });

  it("never collapses rows without a dedupe key", () => {
    let list = addNotification([], base, { now: 1, id: "a" });
    list = addNotification(list, base, { now: 2, id: "b" });
    expect(list).toHaveLength(2);
  });

  it("evicts the oldest rows beyond the limit", () => {
    const list = seed(NOTIFICATION_LIMIT + 5);
    expect(list).toHaveLength(NOTIFICATION_LIMIT);
    // Newest survive; the earliest titles are gone.
    expect(list[0].title).toBe(`event ${NOTIFICATION_LIMIT + 4}`);
    expect(list.some((n) => n.title === "event 0")).toBe(false);
  });

  it("stores null rather than an empty body", () => {
    expect(addNotification([], base, { now: 1, id: "x" })[0].body).toBeNull();
  });
});

describe("unread counts", () => {
  it("is zero for an empty list", () => {
    expect(unreadCount([])).toBe(0);
  });

  it("counts only unread rows", () => {
    const list = markRead(seed(4), "id-1");
    expect(unreadCount(list)).toBe(3);
  });

  it("is unaffected by collapsing repeats of one event", () => {
    let list = addNotification([], { ...base, dedupeKey: "k" }, { now: 1, id: "a" });
    list = addNotification(list, { ...base, dedupeKey: "k" }, { now: 2, id: "a2" });
    list = addNotification(list, { ...base, dedupeKey: "k" }, { now: 3, id: "a3" });
    expect(unreadCount(list)).toBe(1);
  });
});

describe("read state transitions", () => {
  it("marks a single row read", () => {
    const list = markRead(seed(2), "id-0");
    expect(list.find((n) => n.id === "id-0")?.read).toBe(true);
    expect(list.find((n) => n.id === "id-1")?.read).toBe(false);
  });

  it("marks a row unread again", () => {
    let list = markAllRead(seed(2));
    list = markUnread(list, "id-1");
    expect(list.find((n) => n.id === "id-1")?.read).toBe(false);
    expect(unreadCount(list)).toBe(1);
  });

  it("marks all rows read", () => {
    const list = markAllRead(seed(5));
    expect(unreadCount(list)).toBe(0);
    expect(list).toHaveLength(5);
  });

  it("returns the same length when marking all read twice", () => {
    const once = markAllRead(seed(3));
    const twice = markAllRead(once);
    expect(twice).toHaveLength(once.length);
    expect(unreadCount(twice)).toBe(0);
  });

  it("ignores an unknown id", () => {
    const list = seed(2);
    expect(markRead(list, "nope")).toEqual(list);
    expect(markUnread(list, "nope")).toEqual(list);
    expect(dismissNotification(list, "nope")).toHaveLength(2);
  });
});

describe("dismissal", () => {
  it("removes only the requested row", () => {
    const list = dismissNotification(seed(3), "id-1");
    expect(list.map((n) => n.id)).toEqual(["id-2", "id-0"]);
  });

  it("decrements the unread count when an unread row is dismissed", () => {
    const list = seed(3);
    expect(unreadCount(dismissNotification(list, "id-0"))).toBe(2);
  });

  it("clears everything", () => {
    expect(clearAll()).toEqual([]);
  });
});

describe("filtering", () => {
  it("returns every row in newest-first order for 'all'", () => {
    const list = markRead(seed(3), "id-1");
    expect(filterNotifications(list, "all").map((n) => n.id)).toEqual(["id-2", "id-1", "id-0"]);
  });

  it("returns only unread rows for 'unread'", () => {
    const list = markRead(seed(3), "id-1");
    expect(filterNotifications(list, "unread").map((n) => n.id)).toEqual(["id-2", "id-0"]);
  });

  it("orders unsorted input newest-first", () => {
    const shuffled = [...seed(3)].reverse();
    expect(byNewest(shuffled).map((n) => n.id)).toEqual(["id-2", "id-1", "id-0"]);
  });

  // ---- category scoping ----------------------------------------------------

  /** Three rows, one per category, newest first: scan, device, transfer. */
  function mixed() {
    let list = addNotification([], { ...base, category: "transfer" }, { now: 1, id: "t" });
    list = addNotification(list, { ...base, category: "device" }, { now: 2, id: "d" });
    list = addNotification(list, { ...base, category: "scan" }, { now: 3, id: "s" });
    return list;
  }

  it("keeps every category when scoping is 'any'", () => {
    expect(filterNotifications(mixed(), "all", "any").map((n) => n.id)).toEqual(["s", "d", "t"]);
    expect(filterNotifications(mixed(), "all").map((n) => n.id)).toEqual(["s", "d", "t"]);
  });

  it("narrows to a single category", () => {
    expect(filterNotifications(mixed(), "all", "device").map((n) => n.id)).toEqual(["d"]);
  });

  it("combines the unread and category filters rather than letting one win", () => {
    const list = markRead(mixed(), "s");
    expect(filterNotifications(list, "unread", "scan")).toEqual([]);
    expect(filterNotifications(list, "all", "scan").map((n) => n.id)).toEqual(["s"]);
    expect(filterNotifications(list, "unread", "any").map((n) => n.id)).toEqual(["d", "t"]);
  });

  it("re-sorts a scoped result newest-first", () => {
    const list = [...mixed()].reverse();
    expect(filterNotifications(list, "all", "any").map((n) => n.id)).toEqual(["s", "d", "t"]);
  });

  it("keeps the source list untouched", () => {
    const list = mixed();
    const snapshot = list.map((n) => n.id);
    filterNotifications(list, "unread", "device");
    expect(list.map((n) => n.id)).toEqual(snapshot);
  });
});

describe("category picker options", () => {
  it("lists only categories that actually have rows", () => {
    let list = addNotification([], { ...base, category: "transfer" }, { now: 1, id: "t" });
    list = addNotification(list, { ...base, category: "setup" }, { now: 2, id: "s" });
    expect(presentCategories(list)).toEqual(["transfer", "setup"]);
  });

  it("never offers a category with no rows, so no option is a dead end", () => {
    const list = addNotification([], { ...base, category: "device" }, { now: 1, id: "d" });
    expect(presentCategories(list)).toEqual(["device"]);
    expect(presentCategories(list)).not.toContain("scan");
  });

  it("is stable in order regardless of how rows arrived", () => {
    let list = addNotification([], { ...base, category: "setup" }, { now: 1, id: "a" });
    list = addNotification(list, { ...base, category: "device" }, { now: 2, id: "b" });
    expect(presentCategories(list)).toEqual(["device", "setup"]);
  });

  it("is empty for an empty store", () => {
    expect(presentCategories([])).toEqual([]);
  });

  it("labels every category in plain language", () => {
    for (const category of presentCategories(
      // One row of every real category.
      (["device", "scan", "transfer", "destination", "setup"] as const).reduce(
        (list, category, index) =>
          addNotification(list, { ...base, category }, { now: index, id: `id-${index}` }),
        [] as AppNotification[],
      ),
    )) {
      expect(CATEGORY_LABELS[category]).toBeTruthy();
    }
  });

  it("validates a persisted or client-supplied scope", () => {
    expect(isCategoryFilter("any")).toBe(true);
    expect(isCategoryFilter("transfer")).toBe(true);
    expect(isCategoryFilter("finance")).toBe(false);
    expect(isCategoryFilter(undefined)).toBe(false);
    expect(isCategoryFilter(7)).toBe(false);
  });
});

describe("inline alert selection", () => {
  it("returns the newest unread problem", () => {
    let list = addNotification([], { ...base, severity: "warning", title: "old" }, { now: 1, id: "w" });
    list = addNotification(list, { ...base, severity: "error", title: "new" }, { now: 2, id: "e" });
    expect(newestUnseenProblem(list)?.id).toBe("e");
  });

  it("ignores success and info rows", () => {
    const list = addNotification([], { ...base, severity: "success" }, { now: 1, id: "s" });
    expect(newestUnseenProblem(list)).toBeNull();
  });

  it("ignores problems the user already read", () => {
    let list = addNotification([], { ...base, severity: "error" }, { now: 1, id: "e" });
    list = markRead(list, "e");
    expect(newestUnseenProblem(list)).toBeNull();
  });
});

describe("persistence", () => {
  it("round-trips a list without loss", () => {
    const list = markRead(seed(3), "id-1");
    expect(parseNotifications(serializeNotifications(list))).toEqual(byNewest(list));
  });

  it("preserves collapsed counts across a reload", () => {
    let list = addNotification([], { ...base, dedupeKey: "k" }, { now: 1, id: "a" });
    list = addNotification(list, { ...base, dedupeKey: "k" }, { now: 2, id: "a" });
    const restored = parseNotifications(serializeNotifications(list));
    expect(restored[0].count).toBe(2);
    expect(restored[0].dedupeKey).toBe("k");
  });

  it("caps what is written to storage", () => {
    const stored = serializeNotifications(seed(NOTIFICATION_LIMIT + 20));
    expect(parseNotifications(stored)).toHaveLength(NOTIFICATION_LIMIT);
  });

  // ---- untrusted input -----------------------------------------------------

  it("returns an empty list for absent, empty, or malformed JSON", () => {
    expect(parseNotifications(null)).toEqual([]);
    expect(parseNotifications(undefined)).toEqual([]);
    expect(parseNotifications("")).toEqual([]);
    expect(parseNotifications("{not json")).toEqual([]);
    expect(parseNotifications("null")).toEqual([]);
    expect(parseNotifications("42")).toEqual([]);
    expect(parseNotifications('"a string"')).toEqual([]);
    expect(parseNotifications("{}")).toEqual([]);
    expect(parseNotifications('{"notifications":"nope"}')).toEqual([]);
  });

  it("drops entries with a bad shape instead of coercing them", () => {
    const raw = JSON.stringify({
      version: 1,
      notifications: [
        { id: "ok", severity: "info", category: "device", title: "fine", createdAt: 5, read: false, count: 1 },
        { id: "", severity: "info", category: "device", title: "no id" },
        { id: "b", severity: "catastrophe", category: "device", title: "bad severity" },
        { id: "c", severity: "info", category: "finance", title: "bad category" },
        { id: "d", severity: "info", category: "device", title: "" },
        { id: "e", severity: "info", category: "device" },
        "nonsense",
        null,
      ],
    });
    const parsed = parseNotifications(raw);
    expect(parsed.map((n) => n.id)).toEqual(["ok"]);
  });

  it("repairs hostile values rather than trusting them", () => {
    const raw = JSON.stringify({
      notifications: [
        {
          id: "x",
          severity: "error",
          category: "transfer",
          title: "t",
          body: 12345, // wrong type
          createdAt: "yesterday", // wrong type
          read: "yes", // wrong type, truthy
          count: -5, // out of range
          dedupeKey: 99, // wrong type
          target: "https://evil.example", // not a known target
        },
      ],
    });
    const [n] = parseNotifications(raw);
    expect(n.body).toBeNull();
    expect(n.createdAt).toBe(0);
    expect(n.read).toBe(false);
    expect(n.count).toBe(1);
    expect(n.dedupeKey).toBeNull();
    expect(n.target).toBeNull();
  });

  it("drops duplicate ids", () => {
    const row = { id: "dup", severity: "info", category: "device", title: "t", createdAt: 1, read: false, count: 1 };
    const parsed = parseNotifications(JSON.stringify({ notifications: [row, { ...row }] }));
    expect(parsed).toHaveLength(1);
  });

  it("bounds work on an oversized hostile payload", () => {
    const notifications = Array.from({ length: NOTIFICATION_LIMIT * 50 }, (_, i) => ({
      id: `i-${i}`,
      severity: "info",
      category: "device",
      title: "flood",
      createdAt: i,
      read: false,
      count: 1,
    }));
    expect(parseNotifications(JSON.stringify({ notifications }))).toHaveLength(NOTIFICATION_LIMIT);
  });
});

describe("sanitizeText", () => {
  it("strips control characters that would break a single-line slot", () => {
    expect(sanitizeText("line one\nline two", 100)).toBe("line one line two");
    expect(sanitizeText("tab\there\u0007", 100)).toBe("tab here");
  });

  it("collapses runs of whitespace", () => {
    expect(sanitizeText("a    b", 100)).toBe("a b");
  });

  it("truncates to the limit and marks the cut", () => {
    const out = sanitizeText("x".repeat(500), MAX_TITLE_LENGTH);
    expect(out.length).toBe(MAX_TITLE_LENGTH);
    expect(out.endsWith("…")).toBe(true);
  });

  it("returns an empty string for non-strings", () => {
    expect(sanitizeText(undefined, 10)).toBe("");
    expect(sanitizeText(null, 10)).toBe("");
    expect(sanitizeText({}, 10)).toBe("");
    expect(sanitizeText(42, 10)).toBe("");
  });

  it("is applied to titles and bodies on insert", () => {
    const [n] = addNotification(
      [],
      { ...base, title: "a\nb", body: "c\nd".repeat(MAX_BODY_LENGTH) },
      { now: 1, id: "x" },
    );
    expect(n.title).toBe("a b");
    expect(n.body?.includes("\n")).toBe(false);
    expect(n.body!.length).toBeLessThanOrEqual(MAX_BODY_LENGTH);
  });
});

/** A finished-transfer payload with every count zeroed, then overridden. */
function finished(overrides: Partial<FinishedPayload> = {}): FinishedPayload {
  return {
    completed: 0,
    skipped: 0,
    failed: 0,
    interrupted: 0,
    bytes_copied: 0,
    cancelled: false,
    error: null,
    message: "Transfer finished",
    ...overrides,
  };
}

describe("transferOutcomeNotification", () => {
  it("reports a clean run as a success", () => {
    const n = transferOutcomeNotification(finished({ completed: 3, bytes_copied: 4_821_963 }));
    expect(n.severity).toBe("success");
    expect(n.title).toBe("Transfer complete");
    expect(n.target).toBe("files");
  });

  it("formats the byte count instead of printing raw bytes", () => {
    const n = transferOutcomeNotification(finished({ completed: 1, bytes_copied: 1_234_567 }));
    expect(n.body).toBe("1 copied, 1.2 MB");
    expect(n.body).not.toContain("bytes");
    // No bare integer that could only have come from an unformatted count.
    expect(n.body).not.toMatch(/\d{4,}/);
  });

  it("omits the size when nothing was copied", () => {
    const n = transferOutcomeNotification(finished({ skipped: 2 }));
    expect(n.title).toBe("Nothing to transfer");
    expect(n.body).toBe("0 copied, 2 skipped");
  });

  it("reports a hard failure as an error carrying the reason", () => {
    const n = transferOutcomeNotification(
      finished({
        completed: 1,
        bytes_copied: 2_000,
        error: { code: "E_DEVICE_LOST", message: "The iPhone was disconnected" },
      }),
    );
    expect(n.severity).toBe("error");
    expect(n.title).toBe("Transfer stopped");
    expect(n.body).toContain("The iPhone was disconnected");
    expect(n.dedupeKey).toBe("transfer:stopped");
  });

  it("reports a cancellation as a warning, not a failure", () => {
    const n = transferOutcomeNotification(
      finished({ completed: 2, bytes_copied: 3_000, cancelled: true }),
    );
    expect(n.severity).toBe("warning");
    expect(n.title).toBe("Transfer cancelled");
    expect(n.dedupeKey).toBe("transfer:cancelled");
  });

  it("singularises one failed file", () => {
    const n = transferOutcomeNotification(finished({ completed: 2, failed: 1 }));
    expect(n.severity).toBe("warning");
    expect(n.title).toBe("Transfer finished with 1 file failed");
    expect(n.body).toContain("Transfer finished");
  });

  it("pluralises several failed files", () => {
    expect(transferOutcomeNotification(finished({ failed: 4 })).title).toBe(
      "Transfer finished with 4 files failed",
    );
  });

  it("distinguishes interrupted files from failed ones", () => {
    const n = transferOutcomeNotification(finished({ interrupted: 2 }));
    expect(n.title).toBe("Transfer finished with interrupted files");
    expect(n.body).toContain("2 interrupted");
  });

  it("lets an error outrank a cancellation", () => {
    const n = transferOutcomeNotification(
      finished({ cancelled: true, error: { code: "E_IO", message: "read failed" } }),
    );
    expect(n.severity).toBe("error");
    expect(n.dedupeKey).toBe("transfer:stopped");
  });

  it("never reports a partial run as complete", () => {
    const n = transferOutcomeNotification(finished({ completed: 9, failed: 1 }));
    expect(n.title).not.toBe("Transfer complete");
    expect(n.severity).toBe("warning");
  });

  it("gives each outcome its own dedupe key so one cannot mask another", () => {
    const keys = [
      transferOutcomeNotification(finished({ error: { code: "E", message: "x" } })).dedupeKey,
      transferOutcomeNotification(finished({ cancelled: true })).dedupeKey,
      transferOutcomeNotification(finished({ failed: 1 })).dedupeKey,
    ];
    expect(new Set(keys).size).toBe(3);
    // A clean success is unique by nature and must not collapse with anything.
    expect(
      transferOutcomeNotification(finished({ completed: 1, bytes_copied: 10 })).dedupeKey,
    ).toBeUndefined();
  });
});

describe("notificationReducer", () => {
  it("adds, reads, and dismisses", () => {
    let state = notificationReducer([], { type: "add", input: base, now: 1, id: "a" });
    state = notificationReducer(state, { type: "add", input: base, now: 2, id: "b" });
    expect(state).toHaveLength(2);

    state = notificationReducer(state, { type: "markRead", id: "a" });
    expect(unreadCount(state)).toBe(1);

    state = notificationReducer(state, { type: "markAllRead" });
    expect(unreadCount(state)).toBe(0);

    state = notificationReducer(state, { type: "dismiss", id: "a" });
    expect(state.map((n) => n.id)).toEqual(["b"]);

    state = notificationReducer(state, { type: "clearAll" });
    expect(state).toEqual([]);
  });

  it("does not mutate the previous state", () => {
    const before = seed(2);
    const snapshot = JSON.parse(JSON.stringify(before)) as AppNotification[];
    notificationReducer(before, { type: "markRead", id: "id-0" });
    notificationReducer(before, { type: "clearAll" });
    notificationReducer(before, { type: "add", input: base, now: 99, id: "new" });
    expect(before).toEqual(snapshot);
  });

  it("toggles a row back to unread", () => {
    let state = notificationReducer([], { type: "add", input: base, now: 1, id: "a" });
    state = notificationReducer(state, { type: "markRead", id: "a" });
    state = notificationReducer(state, { type: "markUnread", id: "a" });
    expect(unreadCount(state)).toBe(1);
  });
});
