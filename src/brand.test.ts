/**
 * Brand guards for the Mavora identity.
 *
 * A rebrand is mostly a rename, and renames rot: a stray "Local iPhone Transfer"
 * in a window title, a second copy of the logo that drifts from the first, a
 * palette tweak that quietly drops a text colour below 4.5:1. None of that needs
 * a browser to catch, so all of it is locked here:
 *
 *   - the user-facing name, in every place a user can see it,
 *   - one geometry for the mark, shared by the app and the favicon,
 *   - the four brand colours, defined once and never re-typed elsewhere,
 *   - and the contrast of every token pair the interface actually uses,
 *     computed from the stylesheet rather than trusted from a comment.
 */

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

const css = read("src/styles.css");
const icons = read("src/icons.tsx");
const favicon = read("public/favicon.svg");
const html = read("index.html");
const sidebar = read("src/components/Sidebar.tsx");
const tauriConf = read("src-tauri/tauri.conf.json");

const BRAND = {
  navy: "#101827",
  cyan: "#06b6d4",
  white: "#ffffff",
  slate: "#64748b",
};

/* ---- token reading -------------------------------------------------------- */

/** Body of the first `{ … }` block at or after `marker`, braces balanced. */
function blockAfter(marker: string, source = css): string {
  const start = source.indexOf(marker);
  if (start === -1) throw new Error(`Marker not found: ${marker}`);
  const open = source.indexOf("{", start);
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    else if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  throw new Error(`Unbalanced braces after: ${marker}`);
}

const LIGHT = blockAfter(":root {");
const DARK = blockAfter(":root {", blockAfter("@media (prefers-color-scheme: dark) {"));

/** One `--name: value;` declaration, ignoring comment bodies with the same text. */
function declaration(block: string, token: string): string | undefined {
  const bare = block.replace(/\/\*[\s\S]*?\*\//g, "");
  const match = new RegExp(`(?:^|;|\\{)\\s*${token}\\s*:([^;]*)`).exec(bare);
  return match?.[1]?.trim();
}

/**
 * A token, resolved within `block` and then the light block: the dark appearance
 * overrides only what it changes, and `--bg-canvas: var(--surface-tile-1)` has
 * to reach the ramp declared once at the top.
 */
function lookup(block: string, token: string): string | undefined {
  return declaration(block, token) ?? declaration(LIGHT, token);
}

/** A token's literal value, following `var()` indirection until it reaches a hex. */
function color(block: string, token: string): string {
  let value = lookup(block, token);
  if (value === undefined) throw new Error(`Token not defined: ${token}`);
  for (let hop = 0; hop < 4 && value.startsWith("var("); hop += 1) {
    const referenced = value.slice(4, -1).trim();
    const next = lookup(block, referenced);
    if (next === undefined) throw new Error(`Unresolved var: ${referenced}`);
    value = next;
  }
  if (!/^#[0-9a-f]{6}$/i.test(value)) throw new Error(`${token} is not a hex colour: ${value}`);
  return value.toLowerCase();
}

/** A dark-appearance token, falling back to the light block when unset. */
function darkColor(token: string): string {
  return color(DARK, token);
}

/* ---- WCAG contrast -------------------------------------------------------- */

const channel = (value: number) => {
  const srgb = value / 255;
  return srgb <= 0.03928 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
};

function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  return (
    0.2126 * channel((n >> 16) & 255) +
    0.7152 * channel((n >> 8) & 255) +
    0.0722 * channel(n & 255)
  );
}

function contrast(a: string, b: string): number {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (high + 0.05) / (low + 0.05);
}

describe("Mavora identity", () => {
  it("names the product Mavora wherever a user reads it", () => {
    expect(/<title>\s*Mavora\s*<\/title>/.test(html)).toBe(true);
    expect(html).toMatch(/<link rel="icon" type="image\/svg\+xml" href="\/favicon\.svg"/);

    const conf = JSON.parse(tauriConf) as {
      productName: string;
      app: { windows: { title: string }[] };
      bundle: { shortDescription: string };
    };
    expect(conf.productName).toBe("Mavora");
    expect(conf.app.windows[0]?.title).toBe("Mavora");
    // The window title is the installed app's name and must not still say the
    // old product name in any form.
    expect(JSON.stringify(conf)).not.toMatch(/Local iPhone Transfer|Local Transfer/);

    expect(sidebar).toMatch(/<span>Mavora<\/span>/);
  });

  it("has no old product name left in user-facing text", () => {
    // Internal identifiers are deliberately exempt: the crate, the npm package
    // and the bundle identifier keep their pre-rebrand names on purpose (see the
    // README), and the historical release notes stay as they shipped.
    const userFacing = [
      "index.html",
      "src/components/Sidebar.tsx",
      "src/components/AppHeader.tsx",
      "src/components/SetupScreen.tsx",
      "src/components/DeviceCard.tsx",
      "src-tauri/tauri.conf.json",
      "src-tauri/src/environment.rs",
      "src-tauri/src/errors.rs",
      "src-tauri/src/lib.rs",
      "src-tauri/capabilities/default.json",
    ];
    for (const path of userFacing) {
      expect(read(path), `${path} still names the product by its old name`).not.toMatch(
        /Local iPhone Transfer|Local Transfer\b/,
      );
    }
  });

  it("defines the four brand colours once, in tokens", () => {
    expect(color(LIGHT, "--brand-navy")).toBe(BRAND.navy);
    expect(color(LIGHT, "--brand-cyan")).toBe(BRAND.cyan);
    expect(color(LIGHT, "--brand-white")).toBe(BRAND.white);
    expect(color(LIGHT, "--brand-slate")).toBe(BRAND.slate);

    // The mark's three colours are the brand tokens, not second definitions.
    expect(color(LIGHT, "--brand-tile")).toBe(BRAND.navy);
    expect(color(LIGHT, "--brand-slope-right")).toBe(BRAND.cyan);
    expect(color(LIGHT, "--brand-slope-left")).toBe(BRAND.white);
  });

  it("draws the mark once and shares that geometry with the favicon", () => {
    const geometry = /export const MAVORA_GEOMETRY = "([^"]+)"/.exec(icons)?.[1];
    expect(geometry).toBeDefined();
    // The favicon is rasterized into the bundled app icons, so its path must be
    // the same drawing as the one the app renders — not a lookalike.
    const faviconLower = favicon.toLowerCase();
    expect(favicon).toContain(`d="${geometry}"`);
    expect(faviconLower).toContain(BRAND.navy);
    expect(faviconLower).toContain(BRAND.cyan);
    expect(faviconLower).toContain(BRAND.white);

    // One seam, two clipped halves: the apex keeps a single unbroken miter.
    expect((favicon.match(/clip-path/g) ?? []).length).toBe(2);
    expect(sidebar).toMatch(/<MavoraMark /);
    expect(sidebar).not.toMatch(/name="brand"/);
  });

  it("keeps cyan off text, where it cannot clear 4.5:1", () => {
    // The rule that follows from the brand cyan being bright, asserted so a
    // palette edit cannot quietly put it where it fails.
    expect(contrast(BRAND.cyan, BRAND.white)).toBeLessThan(3);
    expect(contrast(BRAND.cyan, BRAND.white)).toBeLessThan(4.5);

    const accent = color(LIGHT, "--accent");
    expect(accent).not.toBe(BRAND.cyan);
    expect(contrast(accent, color(LIGHT, "--canvas"))).toBeGreaterThanOrEqual(4.5);
  });

  it("meets contrast for every text pair it uses, in both appearances", () => {
    const light = {
      canvas: color(LIGHT, "--canvas"),
      rail: color(LIGHT, "--canvas-parchment"),
      surface: color(LIGHT, "--bg-surface"),
    };
    expect(contrast(color(LIGHT, "--text-primary"), light.canvas)).toBeGreaterThanOrEqual(7);
    expect(contrast(color(LIGHT, "--text-primary"), light.rail)).toBeGreaterThanOrEqual(7);
    expect(contrast(color(LIGHT, "--text-secondary"), light.rail)).toBeGreaterThanOrEqual(7);
    expect(contrast(color(LIGHT, "--text-tertiary"), light.canvas)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color(LIGHT, "--text-tertiary"), light.rail)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color(LIGHT, "--on-accent"), color(LIGHT, "--accent-solid"))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color(LIGHT, "--on-accent"), color(LIGHT, "--accent-solid-strong"))).toBeGreaterThanOrEqual(4.5);
    // Neutral glyphs are graphics: 3:1 is the bar, on either surface.
    expect(contrast(color(LIGHT, "--glyph-muted"), light.canvas)).toBeGreaterThanOrEqual(3);
    expect(contrast(color(LIGHT, "--glyph-muted"), light.rail)).toBeGreaterThanOrEqual(3);
    // Progress reads against its own track, which is where the bright cyan failed.
    expect(contrast(color(LIGHT, "--accent"), color(LIGHT, "--bg-track"))).toBeGreaterThanOrEqual(3);
    expect(contrast(color(LIGHT, "--focus-ring"), light.canvas)).toBeGreaterThanOrEqual(3);
    // A native checkbox is ticked in white, so its fill is the deep step.
    expect(contrast(color(LIGHT, "--accent-check"), BRAND.white)).toBeGreaterThanOrEqual(4.5);

    const dark = {
      canvas: darkColor("--bg-canvas"),
      surface: darkColor("--bg-surface"),
      window: darkColor("--bg-window"),
    };
    expect(contrast(darkColor("--text-primary"), dark.canvas)).toBeGreaterThanOrEqual(7);
    expect(contrast(darkColor("--text-secondary"), dark.surface)).toBeGreaterThanOrEqual(7);
    expect(contrast(darkColor("--text-tertiary"), dark.canvas)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(darkColor("--text-tertiary"), dark.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(darkColor("--accent"), dark.canvas)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(darkColor("--accent"), dark.surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(darkColor("--on-accent"), darkColor("--accent-solid"))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(darkColor("--glyph-muted"), dark.canvas)).toBeGreaterThanOrEqual(3);
    expect(contrast(darkColor("--accent"), darkColor("--bg-track"))).toBeGreaterThanOrEqual(3);
    expect(contrast(darkColor("--focus-ring"), dark.canvas)).toBeGreaterThanOrEqual(3);
    for (const signal of ["--success", "--warning", "--danger"]) {
      expect(contrast(darkColor(signal), dark.canvas), `${signal} on dark`).toBeGreaterThanOrEqual(4.5);
    }

    // The dark canvas is midnight-navy family, not the old pure black: the
    // identity has to survive the appearance switch, not just the light theme.
    expect(dark.window).not.toBe("#000000");
    expect(contrast(dark.window, BRAND.navy)).toBeLessThan(1.5);
  });

  it("carries no leftover accent from the previous identity", () => {
    for (const stale of ["#0066cc", "#2997ff", "#0071e3"]) {
      expect(css.toLowerCase(), `${stale} is still in the stylesheet`).not.toContain(stale);
    }
  });
});
