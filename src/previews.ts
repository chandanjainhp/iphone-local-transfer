// Lazy preview loading.
//
// Every row can ask for a thumbnail; requests start only once the row scrolls
// into view (an `afcclient` subprocess per fetch on the Rust side), are
// deduplicated, run at most `MAX_CONCURRENT` at a time, and land in a cache
// that is itself bounded so scrolling a large library cannot grow memory
// without limit.
//
// A cache entry means: `ready` (data URL), `none` (the source has no preview
// for this file — the UI shows an icon placeholder instead of an invented
// image), or `loading`.

import { useCallback, useEffect, useRef, useState } from "react";
import { getMediaPreview, isTauri } from "./api";
import type { MediaItem } from "./types";

interface PreviewEntry {
  status: "loading" | "ready" | "none";
  url: string | null;
}

/** Keyed by device + file identity so a replaced file never shows a stale thumb. */
const cache = new Map<string, PreviewEntry>();
const readyOrder: string[] = [];
const listeners = new Map<string, Set<() => void>>();
const queue: Array<{ id: string; key: string }> = [];
/** Transient failures per key — bounded retries, then show the icon. */
const failures = new Map<string, number>();

/** Cap on cached thumbnails (~250 × ~150 KB ≈ 37 MB worst case). */
const MAX_ENTRIES = 250;
/** Simultaneous thumbnail fetches. */
const MAX_CONCURRENT = 4;
let active = 0;

export function previewKey(udid: string | null, item: MediaItem): string {
  return `${udid ?? ""}|${item.id}|${item.size ?? ""}|${item.modified_ms ?? ""}`;
}

function notify(key: string) {
  listeners.get(key)?.forEach((cb) => cb());
}

function evictIfNeeded() {
  let ready = 0;
  for (const entry of cache.values()) {
    if (entry.status === "ready") ready += 1;
  }
  while (ready > MAX_ENTRIES && readyOrder.length > 0) {
    const key = readyOrder.shift();
    if (!key) break;
    const entry = cache.get(key);
    if (entry?.status === "ready") {
      cache.delete(key);
      ready -= 1;
    }
  }
}

function pump() {
  while (active < MAX_CONCURRENT && queue.length > 0) {
    const next = queue.shift();
    if (!next) break;
    const entry = cache.get(next.key);
    if (!entry || entry.status !== "loading") continue;
    active += 1;
    getMediaPreview(next.id)
      .then((url) => {
        if (url === null) {
          // The source has no preview for this file — same as an icon fallback,
          // and tracked as `none` so the eviction pass sees every entry.
          cache.set(next.key, { status: "none", url: null });
        } else {
          cache.set(next.key, { status: "ready", url });
          readyOrder.push(next.key);
        }
      })
      .catch(() => {
        // Transient failure: retry once (the row is re-queued by the effect),
        // then accept the icon fallback rather than hammering the device.
        const count = (failures.get(next.key) ?? 0) + 1;
        failures.set(next.key, count);
        if (count >= 2) {
          cache.set(next.key, { status: "none", url: null });
        } else {
          cache.delete(next.key);
        }
      })
      .finally(() => {
        active -= 1;
        evictIfNeeded();
        notify(next.key);
        pump();
      });
  }
}

function ensure(id: string, key: string): PreviewEntry {
  const existing = cache.get(key);
  if (existing) return existing;
  if (!isTauri()) {
    // Browser preview has no device bridge; don't pretend otherwise.
    const none: PreviewEntry = { status: "none", url: null };
    cache.set(key, none);
    return none;
  }
  const loading: PreviewEntry = { status: "loading", url: null };
  cache.set(key, loading);
  queue.push({ id, key });
  pump();
  return loading;
}

/** Drop everything (new scan): releases memory; keys also include size + mtime,
 * so an unchanged file simply reloads and a changed one never shows stale art. */
export function clearPreviews() {
  cache.clear();
  readyOrder.length = 0;
  queue.length = 0;
  failures.clear();
}

export interface PreviewView {
  /** Data URL to render, or null while loading / when unavailable. */
  url: string | null;
  loading: boolean;
  /** Attach to the row/host element to lazy-load only when it scrolls into view. */
  callbackRef: (el: HTMLElement | null) => void;
}

export interface PreviewOptions {
  /** Skip the visibility gate (lightbox: the user explicitly asked for it). */
  immediate?: boolean;
}

/**
 * Subscribe to one item's preview. Fetching starts when the host element is
 * near the viewport (or immediately with `immediate`) *and* `enabled` is true,
 * so scanning/transferring rows never compete with the device work in flight;
 * flipping `enabled` back on resumes loading.
 */
export function usePreview(
  udid: string | null,
  item: MediaItem,
  enabled: boolean,
  options?: PreviewOptions,
): PreviewView {
  const key = previewKey(udid, item);
  const immediate = options?.immediate ?? false;
  const [entry, setEntry] = useState<PreviewEntry | undefined>(() => cache.get(key));
  const [host, setHost] = useState<HTMLElement | null>(null);
  const [seen, setSeen] = useState(immediate);
  const [fetchToken, setFetchToken] = useState(0);
  const observer = useRef<IntersectionObserver | null>(null);

  const callbackRef = useCallback((el: HTMLElement | null) => setHost(el), []);

  // One-shot: mark the host visible the first time it nears the viewport.
  useEffect(() => {
    if (immediate) return;
    if (!host || typeof IntersectionObserver === "undefined") {
      if (host) setSeen(true);
      return;
    }
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setSeen(true);
          io.disconnect();
        }
      },
      { rootMargin: "240px" },
    );
    io.observe(host);
    observer.current = io;
    return () => io.disconnect();
  }, [host, immediate]);

  useEffect(() => {
    const cached = cache.get(key);
    if (cached) setEntry(cached);
    let set = listeners.get(key);
    if (!set) {
      set = new Set();
      listeners.set(key, set);
    }
    const cb = () => {
      const next = cache.get(key);
      if (next === undefined) {
        // A transient failure dropped the entry: re-run the effect to retry.
        setFetchToken((t) => t + 1);
        return;
      }
      setEntry(next);
    };
    set.add(cb);
    if (enabled && seen && !cached) setEntry(ensure(item.id, key));
    return () => {
      set?.delete(cb);
      if (set && set.size === 0) listeners.delete(key);
    };
    // `fetchToken` re-runs the effect after a transient failure cleared the
    // entry, so a visible row can retry instead of spinning forever.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item.id, key, enabled, seen, fetchToken]);

  const ready = entry?.status === "ready";
  return {
    url: ready ? entry?.url ?? null : null,
    loading:
      !ready && entry?.status !== "none" && enabled && (seen || immediate),
    callbackRef,
  };
}
