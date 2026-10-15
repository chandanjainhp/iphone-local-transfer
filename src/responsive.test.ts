/**
 * Layout guards for the responsive shell.
 *
 * A unit test cannot see pixels — that needs a real engine and is done in the
 * browser. What it can do is lock the *structure* that was actually broken and
 * is easy to break again:
 *
 *   - a root `min-width` (the old `body { min-width: 760px }` clamped every
 *     narrower viewport to a 760px canvas and produced page-level horizontal
 *     scrolling, which is what made the app unusable below 760px),
 *   - an undocumented breakpoint, which is how the 850px tier drifted out of
 *     sync with the shell's own tiers,
 *   - modal width caps measured against `vw` instead of their own overlay, which
 *     can leave a dialog hanging past the viewport edge.
 */

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

/** A rule's declarations, matched on a selector token inside any block. */
function rule(selector: string, source = css): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`(?:^|[\\s,}])${escaped}\\s*\\{([^}]*)\\}`, "m").exec(source);
  if (!match) throw new Error(`No rule found for ${selector}`);
  return match[1];
}

/** Single-declaration value lookup ("grid-template-columns: a b;"). */
function value(selector: string, property: string, source = css): string {
  const body = rule(selector, source);
  // A declaration may be the block's first, hence the brace in the prefix.
  const match = new RegExp(`(?:^|;|\\{)\\s*${property}\\s*:([^;]*)`, "m").exec(body);
  if (!match) throw new Error(`No ${property} on ${selector}`);
  return match[1].trim();
}

/** Every `@media` prelude in the file, in source order. */
function mediaQueries(): string[] {
  const found: string[] = [];
  let depth = 0;
  let inComment = false;
  for (const line of css.split("\n")) {
    const code = stripComments(line, () => inComment, (next) => (inComment = next));
    if (depth === 0) {
      const match = /^@media\s+(.+?)\s*\{/.exec(code.trim());
      if (match) found.push(match[1]);
    }
    for (const char of code) {
      if (char === "{") depth += 1;
      else if (char === "}") depth -= 1;
    }
  }
  return found;
}

/** Removes `/* … *\/` spans, which may open on one line and close on another. */
function stripComments(
  line: string,
  isOpen: () => boolean,
  setOpen: (open: boolean) => void,
): string {
  let out = "";
  let rest = line;
  while (rest.length > 0) {
    if (isOpen()) {
      const end = rest.indexOf("*/");
      if (end === -1) return out;
      rest = rest.slice(end + 2);
      setOpen(false);
    } else {
      const start = rest.indexOf("/*");
      if (start === -1) return out + rest;
      out += rest.slice(0, start);
      rest = rest.slice(start + 2);
      setOpen(true);
    }
  }
  return out;
}

/** Text of the `@media (max-width: Npx)` tier, or "" when absent. */
function tier(width: number): string {
  const lines = css.split("\n");
  const start = lines.findIndex((line) => line.trim() === `@media (max-width: ${width}px) {`);
  if (start === -1) return "";
  let depth = 0;
  const body: string[] = [];
  for (let i = start; i < lines.length; i += 1) {
    const line = lines[i]!;
    depth += (line.match(/\{/g) ?? []).length;
    depth -= (line.match(/\}/g) ?? []).length;
    body.push(line);
    if (depth === 0) break;
  }
  return body.join("\n");
}

describe("responsive shell", () => {
  it("does not clamp the page to a fixed minimum width", () => {
    const root = /(?:^|\n)body\s*\{([^}]*)\}/m.exec(css)?.[1];
    expect(root).toBeDefined();
    // Declarations only: prose in the comments is allowed to say "min-width".
    expect(root!.replace(/\/\*[\s\S]*?\*\//g, "")).not.toMatch(/min-width\s*:/);
  });

  it("uses only the two documented width tiers", () => {
    const widths = mediaQueries()
      .map((query) => /^\(max-width:\s*(\d+)px\)$/.exec(query)?.[1])
      .filter((width) => width !== undefined);
    expect(widths).toEqual(["1023", "767"]);
  });

  it("reflows stat tiles instead of pinning a five-column row", () => {
    // Five fixed columns cannot fit a narrow window; auto-fit reflows on its own
    // and needs no breakpoint of its own to stay in sync.
    expect(value(".progress-stats", "grid-template-columns")).toMatch(/auto-fit/);
  });

  it("collapses the shell to one column on the compact tier", () => {
    const compact = tier(767);
    expect(compact).toMatch(/\.app-shell\s*\{[^}]*grid-template-columns:\s*minmax\(0, 1fr\)/);
    expect(compact).toMatch(/\.media-row\s*\{[^}]*grid-template-areas/);
    // The sidebar becomes a horizontal rail rather than a column that would
    // otherwise sit above the content at full height.
    expect(compact).toMatch(/\.sidebar\s*\{[^}]*flex-direction:\s*row/);
  });

  it("drops the date column one tier earlier than the shell columns", () => {
    expect(tier(1023)).toMatch(/\.file-date\s*\{[^}]*display:\s*none/);
  });

  it("allows flex and grid children to shrink below their content", () => {
    for (const selector of [".file-name", ".section-heading > div", ".notif-main"]) {
      expect(rule(selector)).toMatch(/min-width:\s*0/);
    }
  });

  it("caps dialogs against the overlay they sit in, never the raw viewport", () => {
    for (const [selector, property] of [
      [".preview-card", "max-width"],
      [".confirm-card", "width"],
    ] as const) {
      expect(value(selector, property)).toContain("100%");
      expect(value(selector, property)).not.toMatch(/vw/);
    }
    // Long content scrolls inside the card instead of growing past the screen.
    expect(value(".preview-overlay", "padding")).toMatch(/clamp\(/);
    expect(rule(".confirm-card")).toMatch(/overflow-y:\s*auto/);
  });

  it("respects the dynamic viewport and display cutouts", () => {
    expect(rule(".app-shell")).toMatch(/min-height:\s*100dvh/);
    expect(rule(".app-shell")).toMatch(/env\(safe-area-inset-left\)/);
    expect(rule(".sidebar")).toMatch(/env\(safe-area-inset-bottom\)/);
    expect(rule(".main-panel")).toMatch(/env\(safe-area-inset-bottom\)/);
  });

  it("grows touch targets when the pointer is coarse", () => {
    const coarse = mediaQueries().find((query) => query.includes("pointer: coarse"));
    expect(coarse).toBeDefined();
    expect(css).toMatch(/@media \(pointer: coarse\)\s*\{[^@]*min-height:\s*44px/);
  });
});
