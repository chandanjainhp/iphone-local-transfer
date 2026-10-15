# Local iPhone Transfer

A local-only desktop app that copies accessible iPhone photos and videos to a folder on your computer over a USB cable.

Nothing leaves your machine: no server, no account, no telemetry, no network transfer of any kind. It is a file-copy utility, not a cloud service and not an iPhone backup tool.

[![Latest release](https://img.shields.io/github/v/release/chandanjainhp/iphone-local-transfer?label=release&color=2ea043)](https://github.com/chandanjainhp/iphone-local-transfer/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/chandanjainhp/iphone-local-transfer/total?label=downloads)](https://github.com/chandanjainhp/iphone-local-transfer/releases)
[![Version](https://img.shields.io/badge/version-0.2.0-blue)](https://github.com/chandanjainhp/iphone-local-transfer/releases)
[![Changelog](https://img.shields.io/badge/changelog-CHANGELOG.md-8957e5)](CHANGELOG.md)
[![Backend](https://img.shields.io/badge/backend-Tauri%202%20%C2%B7%20Rust-24C8DB)](https://v2.tauri.app/)
[![Frontend](https://img.shields.io/badge/frontend-React%2018%20%C2%B7%20TypeScript%20%C2%B7%20Vite-3178C6)](https://vitejs.dev/)
[![Network](https://img.shields.io/badge/network-offline%20only-2ea043)](#privacy-and-security)
[![Platform](https://img.shields.io/badge/platform-Linux%20%E2%9C%94%20%7C%20macOS%20%E2%9A%A0%20%7C%20Windows%20%E2%9A%A0-lightgrey)](#platform-support)

| Light | Dark |
| --- | --- |
| ![Main window in the light appearance](docs/screenshots/light.png) | ![Main window in the dark appearance](docs/screenshots/dark.png) |

---

## Table of contents

- [Download](#download)
- [Install on Linux](#install-on-linux)
- [First run](#first-run)
- [Using the app](#using-the-app)
- [Troubleshooting](#troubleshooting)
- [Privacy and security](#privacy-and-security)
- [Architecture](#architecture)
- [Project layout](#project-layout)
- [Transfer correctness policy](#transfer-correctness-policy)
- [Startup environment check](#startup-environment-check)
- [Develop from source](#develop-from-source)
- [Build and package a release](#build-and-package-a-release)
- [Quality gates](#quality-gates)
- [Security auditing](#security-auditing)
- [Platform support](#platform-support)
- [Current status and limitations](#current-status-and-limitations)
- [Changelog](#changelog)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)

---

## Download

Every build is attached to a [GitHub Release](https://github.com/chandanjainhp/iphone-local-transfer/releases). The current release line is **`v0.2.0`** (Linux x86-64 only — see [Platform support](#platform-support)).

➡️ **[Open the latest release](https://github.com/chandanjainhp/iphone-local-transfer/releases/latest)** — or grab the artifact you need directly:

| Format | Download | Size |
| --- | --- | --- |
| Debian / Ubuntu | **[`Local.iPhone.Transfer_0.2.0_amd64.deb`](https://github.com/chandanjainhp/iphone-local-transfer/releases/download/v0.2.0/Local.iPhone.Transfer_0.2.0_amd64.deb)** | 3.2 MB |
| Fedora / RHEL | **[`Local.iPhone.Transfer-0.2.0-1.x86_64.rpm`](https://github.com/chandanjainhp/iphone-local-transfer/releases/download/v0.2.0/Local.iPhone.Transfer-0.2.0-1.x86_64.rpm)** | 3.2 MB |
| Any Linux distro | **[`Local.iPhone.Transfer_0.2.0_amd64.AppImage`](https://github.com/chandanjainhp/iphone-local-transfer/releases/download/v0.2.0/Local.iPhone.Transfer_0.2.0_amd64.AppImage)** | 104 MB |

```bash
# Debian / Ubuntu
sudo apt install ./Local.iPhone.Transfer_0.2.0_amd64.deb

# Fedora / RHEL
sudo dnf install ./Local.iPhone.Transfer-0.2.0-1.x86_64.rpm

# Any Linux distro (no install step, no root needed)
chmod +x ./Local.iPhone.Transfer_0.2.0_amd64.AppImage
./Local.iPhone.Transfer_0.2.0_amd64.AppImage
```

The `.deb` and `.rpm` declare `libimobiledevice-utils` and `usbmuxd` as package dependencies, so the distro package manager pulls in the USB tooling for you. The AppImage bundles everything it needs and is the largest download.

**New in 0.2.0:** a notification centre, the icon and design systems, a responsive layout, an enabled webview CSP, and the first round of automated security gates. The transfer engine keeps the same non-destructive semantics as 0.1.0. Full list: [`CHANGELOG.md`](CHANGELOG.md).

> Build outputs are named `Local iPhone Transfer_…` on disk. GitHub replaces the spaces with dots when uploading, which is why the release assets and the links above read `Local.iPhone.Transfer_…`.

## Install on Linux

1. Download one of the three artifacts above.
2. Install it with the matching command (or run the AppImage directly — no install step, no root).
3. Start **Local iPhone Transfer** from your application menu or from the terminal.

No Rust, Node.js, or developer tools are required on the machine running the app.

## First run

The app opens on a **setup screen** that checks every prerequisite and says, in plain language, exactly what is missing and how to fix it. You will see one of:

| Message | Meaning |
| --- | --- |
| *Preparing your iPhone connection…* | The environment check is still running. |
| *Your computer is ready.* | Connect and unlock the iPhone. |
| *USB support needs setup.* | The `libimobiledevice` tools are missing. The screen shows the exact copy-paste install command for your distro. |
| *iPhone connection service is not running.* | `usbmuxd` is stopped or unreachable. A stale socket counts as *stopped*, not as *missing*. |
| *Unlock your iPhone and approve the Trust prompt.* | Pairing is pending — unlock the phone and tap **Trust**. |
| *No iPhone detected.* | Connect a data-capable cable (not charge-only) and unlock the phone. |

Every screen offers **Check again** and a collapsed **Details for support** section with typed diagnostics — never raw stack traces. The app never installs anything, never starts a service, and never runs `sudo` on its own.

## Using the app

1. Connect the iPhone with a data-capable USB cable and unlock it.
2. Approve the **Trust This Computer?** prompt on the phone if it appears.
3. The app lists the photos and videos iOS itself exposes over USB. It does not claim access to the whole iOS filesystem.
4. Choose a destination folder with **Change…**.
5. Select individual files, use **Select All**, or use **Transfer All New**.
6. Watch per-file and overall progress, bytes copied, live speed, and an estimated time remaining.
7. The app finishes with a clear success summary or an actionable error, and tells you exactly which files were skipped.

Two more conveniences:

- **Previews** — the file list shows iOS-generated thumbnails when the phone provides them, and a plain icon placeholder when it does not. No image is ever invented.
- **Cancel** — the transfer stops safely at a chunk boundary; completed files are preserved and the active item is reported as interrupted, never as completed.

**Demo mode** (sidebar toggle) exercises the whole interface against a clearly labelled mock device when no iPhone is at hand. Demo data is always marked as such in the UI — no invented live values are ever shown in a normal session.

## Troubleshooting

| Symptom | What to do |
| --- | --- |
| Setup screen says the USB service is not running | Start `usbmuxd` with your service manager, e.g. `sudo systemctl start usbmuxd`, then click **Check again**. |
| Setup screen says tools are missing | Install `libimobiledevice` with your package manager (the screen shows the exact command). On Debian/Ubuntu, if `afcclient` is still missing, build it from the libimobiledevice sources (see the project site). |
| "Waiting for Trust approval" won't clear | Unlock the phone, keep it unlocked, and accept the Trust dialog. A fresh, data-capable cable helps. |
| iPhone detected but no files are listed | iOS exposes no files over AFC while iCloud Photos is set to **Optimize iPhone Storage** and the phone is locked. Keep the phone unlocked and check again. |
| Phone keeps disconnecting | Try another cable or port, and avoid USB hubs. The app marks interrupted files and never reports a partial copy as complete. |
| Destination files were skipped | A file with that name already exists. The default policy is **skip**; nothing is overwritten silently. |

## Privacy and security

- **All transfers stay local.** No telemetry, no analytics, no account, no cloud storage, and no network requests of any kind.
- **Source files are never deleted or modified.** The app only reads from the iPhone.
- **Only documented access paths are used.** The app does not bypass iOS pairing, the app sandbox, or any user-consent mechanism, and it shows a clear error rather than attempting private-filesystem access.
- **Filenames are treated as untrusted input** — path traversal is prevented and invalid destination characters are sanitised.
- **Command-line tools are invoked without a shell**, with fixed argument vectors and validated paths only.
- **No file contents and no sensitive full paths are logged by default.** Diagnostics contain typed status codes, not raw traces.
- **The webview runs under a restrictive Content Security Policy** (`default-src 'self'`, no `unsafe-inline`, no `unsafe-eval`), with a separate development policy for Vite's dev server.
- **Secret scanning is offline and dependency-free**, and it reports only a rule name and `file:line` — never the matched text.

## Architecture

- **React 18 + TypeScript + Vite** — user interface ([src/App.tsx](src/App.tsx), typed backend bridge in [src/api.ts](src/api.ts))
- **Tauri 2 + Rust** — desktop shell and native orchestration
- **libimobiledevice command-line tools** (`idevice_id`, `ideviceinfo`, `idevicepair`, `afcclient`) — USB device detection, pairing state, and AFC file access
- **No application server, account system, analytics, cloud storage, or network transfer**

A narrow Rust `DeviceProvider` / `MediaSource` seam keeps platform-specific USB details out of the UI, so the whole interface can be exercised against a fake provider in tests.

Read [`AGENTS.md`](./AGENTS.md) before implementing features — it is the implementation brief and the acceptance criteria.

## Project layout

```
├── index.html                     Vite entry document
├── package.json                   npm scripts and JS dependencies
├── src/                           React + TypeScript frontend
│   ├── App.tsx                    top-level state machine and layout
│   ├── api.ts                     typed bridge to the Tauri commands
│   ├── types.ts                   shared DTOs and UI state types
│   ├── format.ts                  byte / duration / speed formatting
│   ├── icons.tsx                  the single icon registry, scale, and weights
│   ├── notifications.ts           local notification store (pure, unit-tested)
│   ├── notifications.test.ts      notification store test suite
│   ├── hooks.ts                   shared hooks (close-on-outside-press)
│   ├── previews.ts                thumbnail cache and decoding
│   ├── styles.css                 light + dark design tokens and all styles
│   └── components/                AppHeader, Sidebar, DeviceCard,
│                                  DestinationToolbar, FileList, FileRow,
│                                  TransferProgress, NotificationBell,
│                                  NotificationPanel, SetupScreen, dialogs
├── src-tauri/                     Rust native shell
│   ├── src/                       modules listed below
│   ├── icons/                     bundle icons
│   ├── capabilities/              Tauri 2 capability file
│   └── tauri.conf.json            window, bundle, CSP, and dependency config
├── scripts/
│   └── scan-secrets.sh            offline secret scanner (CI + optional hook)
├── .github/workflows/
│   └── security.yml               dependency, secret, and quality-gate CI
├── .githooks/
│   └── pre-commit                 optional local secret gate
├── docs/screenshots/              README screenshots
├── docs/releases/                 release notes, used as the GitHub release body
├── CHANGELOG.md                   release history
├── THIRD_PARTY_NOTICES.md         dependency license audit
├── security-audit-report.md       security assessment and remediation record
└── AGENTS.md                      implementation brief and acceptance criteria
```

### Rust module layout (`src-tauri/src/`)

| Module | Responsibility |
| --- | --- |
| `device.rs` | Device model and the narrow `DeviceProvider` / `MediaSource` seam (fakes for tests) |
| `media.rs` | Media item DTOs, enumeration traits, source-path validation |
| `provider_libimobile.rs` | Real adapter: detection, pairing/Trust classification, AFC listing and reads |
| `provider_mock.rs` | Development-only mock adapter (explicitly labelled **demo** in the UI) |
| `transfer.rs` | Bounded-buffer copy, progress events, cancellation, conflict handling, non-clobbering publish, partial-file policy |
| `progress.rs` | Rolling-window speed / ETA calculation (no invented values) |
| `preview.rs` | Thumbnail discovery and bounded preview payloads derived only from validated source paths |
| `naming.rs` | Filename sanitisation, path-traversal prevention, conflict resolution (skip/rename) |
| `errors.rs` | Stable UI-safe error codes and user-friendly messages |
| `platform.rs` | Tool discovery in trusted search order, executability check, install hints |
| `environment.rs` | Startup prerequisite check with explicit states |
| `lib.rs` | Tauri commands, events, and application state |
| `main.rs` | Binary entry point |

### Native command surface

| Command | Purpose |
| --- | --- |
| `check_environment` | One-shot read-only prerequisite probe |
| `diagnostics` | Typed tool-discovery report for the setup screen |
| `get_device_status` | Latest device snapshot |
| `list_media` | Enumerate accessible photos and videos |
| `get_media_preview` | Fetch an iOS-generated thumbnail for one item |
| `pick_destination` | Native folder picker |
| `start_transfer` | Begin a transfer run |
| `cancel_transfer` | Request cancellation at the next chunk boundary |
| `set_mock_mode`, `set_mock_device_state` | Development-only demo controls (disabled in release builds) |

The backend streams progress back to the UI over two events: `device_status` and `transfer_event`.

### Design language

The interface follows the Apple web design language captured in
`DESIGN-apple.md`, applied through the token block at the top of
[styles.css](src/styles.css). Nothing in the CSS hard-codes a colour, size,
radius or spacing step: every rule uses a token, so the two appearances and the
whole scale are edited in one place.

| Concern | Rule |
| --- | --- |
| Accent | **One** interactive colour: Action Blue `#0066cc` on light surfaces, Sky Link Blue `#2997ff` on dark tiles, Focus Blue `#0071e3` for rings. Semantic status colours exist for progress and results only — they are never a second brand colour |
| Surfaces | White canvas, parchment `#f5f5f7` page and rail, pearl `#fafafc` ghost fill, and the near-black tile ramp `#272729` / `#2a2a2c` / `#252527` / `#000000`. A surface change is the divider — no borders or shadows doing that job |
| Type | Ladder `34 / 28 / 21 / 17 / 14 / 12 / 10`, weights 400 and 600 (500 deliberately absent), negative tracking from 17 px up. Body copy runs at 17 px, not 16 |
| Radii | `0 · 5 · 8 · 11 · 18 · pill` and nothing in between |
| Spacing | 4 / 8 / 12 / 17 / 24 / 32 / 48 on an 8 px base |
| Elevation | Exactly one drop-shadow, `3px 5px 30px rgba(0,0,0,0.22)`, under product imagery resting on a surface (the preview lightbox). Cards, bars and buttons are flat |
| Press | Every button presses with `transform: scale(0.95)` |
| Floating surfaces | The notification panel uses the language's frosted recipe — translucent surface plus `backdrop-filter: saturate(180%) blur(20px)` — which is what gives it elevation without a shadow |
| Buttons | Four grammars, matching the source: Action Blue **pill** (primary), accent-outline **ghost pill** (second CTA), **pearl capsule** (compact utility), and plain accent **text link** |

Three deviations from the source analysis, recorded rather than taken silently:

- **No 56/40 px display sizes.** This is a desktop utility, so the ladder starts
  at `display-md` (34 px) and stops there instead of running the marketing range.
  The top of the ladder steps down fluidly below ~620 px (`clamp(24px, 5.5vw,
  34px)`), so the headline never needs an extra line on a narrow window.
- **No full-bleed dark product tiles.** The alternating light/dark section rhythm
  belongs to a scrolling catalog; a two-pane utility has no section to alternate.
  The dark ramp is used where it is real: the dark appearance, and the frosted
  popover.
- **`ink-muted-48` (`#7a7a7a`) is not used for text**, because it measures
  4.29:1 on white and text must clear 4.5:1. Tertiary text uses `#6e6e73`
  (5.1:1). Contrast is measured against each element's real surface, in both
  appearances.

No webfont is bundled or fetched (the CSP sets `font-src 'self'`, and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) declares a system-font-only
build), so SF Pro resolves on Apple platforms and every other platform falls
through to its own UI face.

### Icons

Every glyph comes from **`@phosphor-icons/react`** (v2) and is chosen in exactly one
place: the semantic registry in [`src/icons.tsx`](src/icons.tsx). Components never
import from the package directly, so a meaning can be re-pointed at a different
glyph without touching call sites.

| Concern | Convention |
| --- | --- |
| Glyph | A semantic name (`transfer`, `warning`, `bellEmpty`) — never a picture name |
| Size | Only the steps in `ICON_SIZE` (`badge` 10 → `hero` 30). No ad-hoc numbers |
| Weight | `regular`, except `bold` below 14 px and `fill` for severity marks |
| Colour | Inherited via `currentColor`; the light/dark tokens drive it |
| Spacing | None on the icon — gap and padding belong to the parent layout |
| Interaction | Always a real `<button>`; the button owns hover/focus/disabled |

Accessibility is enforced by the types, not by convention: an `<Icon>` without a
`label` is emitted `aria-hidden` (decorative), one *with* a `label` becomes
`role="img"`. `IconButton` **requires** a `label`, so an icon-only control cannot be
added without an accessible name. Imports are tree-shaken per icon, so only the
glyphs in the registry reach the bundle.

### Notifications

The notification centre is a **local, offline store** in
[`src/notifications.ts`](src/notifications.ts) — pure functions plus a reducer, so
unread counts, collapsing, dismissal and persistence are unit-tested without a DOM.

- **No server, no accounts.** `AGENTS.md` forbids a backend, and this is a
  single-user offline utility, so there is no server to persist to, no socket to
  subscribe to, and no cross-user access to authorise.
- **Nothing is fabricated.** Rows are derived only from events this app genuinely
  produces: device connect / disconnect / Trust, scan results, transfer outcomes,
  destination picks, and startup prerequisite failures. A category with no real
  event source is deliberately absent.
- **Collapsing.** A repeat of the same event (`dedupeKey`) increments `count` on the
  newest *unread* row instead of adding a duplicate. Once the user has read it, a
  fresh unread row is created — otherwise a recurring fault would stay silently
  marked as handled.
- **Bounded and validated.** At most 50 rows are kept and persisted to
  `localStorage`; `parseNotifications` treats that JSON as untrusted input, drops
  malformed records, repairs wrong types, de-duplicates ids and bounds the work, so
  a poisoned payload degrades to an empty list instead of a broken startup.
- **Never sensitive.** Bodies carry typed UI-safe messages only, are length-capped,
  and have control characters stripped. File contents and full source paths are
  never stored — matching the project's logging policy.
- **One record, two views.** The inline `role="alert"` banner shows the newest
  unread problem. It is the *same* row the panel shows and dismissing it marks that
  row read, so the banner and the panel can never disagree.
- **Filters that cannot lie.** `All` / `Unread` chips carry counts computed from
  the *currently scoped* rows, and the category picker lists only categories that
  actually have rows — so no chip advertises a row the active filter is hiding and
  no option is a dead end. Scoping by category and by read state are orthogonal, so
  "unread transfers" is a reachable view.
- **A popover, not a modal.** The panel is a non-modal `role="dialog"`: Escape
  closes it, clicking outside closes it, focus moves in on open and returns to the
  bell on close, and focus rings are visible for keyboard users.

### Responsive layout

The shell has **two width tiers**, both declared once at the end of
[styles.css](src/styles.css), and everything above them is intrinsically fluid
(`auto-fit` grids, `clamp()` padding, `min()` caps, `min-width: 0` on flex and
grid children):

| Tier | Layout |
| --- | --- |
| ≥ 1024 px | 230 px rail + content; file rows carry a date column and five stat tiles sit on one line |
| ≤ 1023 px | 196 px rail; rows drop the date column, stat tiles reflow to two columns |
| ≤ 767 px | One column: the sidebar becomes a wrapping horizontal rail above the content, the topbar stacks, rows go two-line (name, then size), and the notification panel becomes a viewport-anchored sheet |

The packaged window's minimum is 760 px ([tauri.conf.json](src-tauri/tauri.conf.json)),
so the last tier is what the browser build uses at phone sizes — but it is real
layout, not a leftover: it is what makes the app correct if that minimum is ever
lowered, and it is what keeps resizing sane down to 320 px.

Decisions worth recording:

- **No root `min-width`.** The page used to set `body { min-width: 760px }`, which
  clamped every narrower viewport to a 760 px canvas and produced page-level
  horizontal scrolling. Overflow is never hidden to compensate — `overflow-x`
  stays `visible` so a regression is measurable rather than masked.
- **Bases, not `flex: 1`, on wrapping content columns.** The device card's copy
  column used `flex: 1` (zero basis) inside a wrapping row, so the line-breaker
  thought it fit beside the icon and the button and squeezed it to ~20 px — one
  character per line. `flex: 1 1 240px` says "give me the next line".
- **The date column is the first thing to go, at every tier below desktop**, and
  the time goes with it. On a 320 px window the size and a full date need ~157 px
  and there are ~129 px; a wrapped size ("5.2" over "MB") or a clipped date
  ("Oct 14…") are both worse than the size alone.
- **Dialogs are capped against the overlay they sit in** (`max-width: min(720px,
  100%)` inside a `clamp()`-padded overlay), not against `vw`, so a modal cannot
  hang past the screen edge; long content scrolls inside the card.
- **Touch targets follow the pointer, not the width**: a `@media (pointer:
  coarse)` block raises chips, selects and glyph buttons to the 44 px minimum,
  because a touch laptop in landscape is wide and still needs finger-sized
  controls.
- **Dynamic viewport and cutouts**: the shell is `min-height: 100dvh` and reserves
  `env(safe-area-inset-*)` on the rail, panel and bottom padding.

`npm test` includes [`src/responsive.test.ts`](src/responsive.test.ts), which reads
the stylesheet and locks the invariants above — no root `min-width`, only the two
documented width tiers, `auto-fit` stat tiles, the single-column collapse, the
shrink guards, viewport-capped dialogs and the coarse-pointer block. It cannot see
pixels (that needs a real engine); it exists so the specific regressions that broke
this layout cannot come back unnoticed.

## Transfer correctness policy

- Every file is copied to a hidden temporary file in the destination folder, flushed with `sync_all`, verified against the expected byte count, and only then published under its final name.
- On failure, cancellation, or USB disconnect the temporary file is deleted; the item is reported as failed or interrupted — **never** as completed.
- Destination name conflicts never silently overwrite. The default policy is **skip**; **rename** is optional. On filesystems without hard-link support (FAT/exFAT, many network shares) the final name is claimed with an exclusive create before publishing, so a file that appears during the copy is never replaced either.
- A destination path that is empty or relative is rejected before any device work begins.
- A bounded 1 MiB read buffer streams large files instead of loading them whole.
- Speed and ETA come from real samples over a moving window. The UI shows `—` / `Estimating…` until enough data exists.
- Source creation/modification times are preserved when the device reports them and the target filesystem can represent them. Missing dates are never invented.
- The UI stays responsive throughout: all native work runs off the main thread and reports through events.

## Startup environment check

`check_environment` (Rust: `environment.rs`) runs a read-only, one-shot probe in this order:

```
toolchain present  →  usbmuxd reachable  →  iPhone listed by idevice_id  →  trust verified
```

The usbmuxd step performs a real socket connect, so a stale socket is reported as *stopped*, not *missing*. Trust is verified with `idevicepair validate` and falls back to a direct lockdown query.

It returns one of these explicit states, plus a per-check list the setup screen renders:

`ready` · `device_not_connected` · `dependency_missing` · `service_not_running` · `permission_required` · `pairing_required` · `unsupported_platform` · `error`

The mock adapter is development-only; release builds refuse mock mode.

## Develop from source

### Prerequisites

Install Node.js LTS, Rust stable, and the Tauri v2 system prerequisites for your OS: <https://v2.tauri.app/start/prerequisites/>

Then install the libimobiledevice tools:

| Platform | Command |
| --- | --- |
| Linux (Arch) | `sudo pacman -S libimobiledevice` (provides `idevice_id`, `ideviceinfo`, `idevicepair`, `afcclient`) |
| Linux (Debian/Ubuntu) | `sudo apt install libimobiledevice-utils usbmuxd` (`afcclient` may need to be built from libimobiledevice sources on older releases) |
| Linux (Fedora) | `sudo dnf install libimobiledevice usbmuxd` |
| macOS | `brew install libimobiledevice` |
| Windows | not yet supported or validated — do not claim Windows support until tested with a real iPhone |

The app's **Help** panel, the `diagnostics` command, and the startup `check_environment` command all report which tools were found and how to install any that are missing.

### Run

```bash
npm install
npm run dev        # frontend only (browser preview — device features disabled)
npm run tauri dev  # full desktop app
```

The dev server listens on <http://localhost:1420> with `strictPort` enabled, so a second concurrent dev server will fail loudly rather than silently shifting ports.

## Build and package a release

```bash
npm run tauri build
```

Verified output on Linux x86-64:

| Artifact | Path | Size |
| --- | --- | --- |
| Release binary | `src-tauri/target/release/local-iphone-transfer` | 12 MB |
| Debian package | `src-tauri/target/release/bundle/deb/Local iPhone Transfer_0.2.0_amd64.deb` | 3.2 MB |
| RPM package | `src-tauri/target/release/bundle/rpm/Local iPhone Transfer-0.2.0-1.x86_64.rpm` | 3.2 MB |
| AppImage | `src-tauri/target/release/bundle/appimage/Local iPhone Transfer_0.2.0_amd64.AppImage` | 104 MB |

Packaging notes:

- The bundle icons are declared in `src-tauri/tauri.conf.json` (`bundle.icon`). Without them the AppImage step fails with *"couldn't find a square icon"*.
- The `.deb` and `.rpm` declare dependencies on the distribution's `libimobiledevice*` and `usbmuxd` packages instead of bundling third-party binaries, and ship `THIRD_PARTY_NOTICES.md` to `/usr/share/doc/local-iphone-transfer/`.
- The AppImage bundles the build host's libraries (see the licensing table in [`THIRD_PARTY_NOTICES.md`](./THIRD_PARTY_NOTICES.md)).
- Linux artifacts are unsigned; no code-signing infrastructure is configured.
- **macOS and Windows packaging is not configured or validated.**

### Publishing the release

The `v0.2.0` release is published, so the links in [Download](#download) resolve. Releases do not tag themselves: bump the version in three files first, and only then build, because the bundle filenames come from that version.

```bash
# 1. Bump: package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml
#    (and the local `local-iphone-transfer` entry in src-tauri/Cargo.lock)
# 2. Build the artifacts
npm run tauri build
# 3. Tag and publish
cd src-tauri/target/release/bundle
git tag -a v0.2.1 -m "Local iPhone Transfer v0.2.1"
git push origin master
gh release create v0.2.1 --verify-tag --title "v0.2.1" \
  --notes-file ../../../../docs/releases/v0.2.0.md \
  "deb/Local iPhone Transfer_0.2.1_amd64.deb" \
  "rpm/Local iPhone Transfer-0.2.1-1.x86_64.rpm" \
  "appimage/Local iPhone Transfer_0.2.1_amd64.AppImage"
```

**GitHub replaces spaces in uploaded asset names with dots**, so `Local iPhone Transfer_0.2.0_amd64.deb` becomes `Local.iPhone.Transfer_0.2.0_amd64.deb` on the release and in its download URL. Every download link in this README uses the dotted form. After publishing, verify the real asset names instead of assuming them:

```bash
gh release view v0.2.0 --json assets --jq '.assets[] | "\(.name)  \(.size)"'
```

## Quality gates

```bash
npm run build                # tsc -b && vite build
npm test                     # vitest: 72 tests (63 notification store + 9 responsive guards)
cd src-tauri
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings   # 0 warnings
cargo test                   # 65 unit tests
```

The release build is warning-free too: `npm run tauri build` compiles the crate with `debug_assertions` off, so dead development-only helpers are `cfg`-gated rather than left to warn.

Plus a dependency and secret gate that needs no compiler:

```bash
cd src-tauri && cargo audit          # 0 vulnerabilities (2 accepted warnings — see below)
npm audit --omit=dev --audit-level=high
bash scripts/scan-secrets.sh         # offline, dependency-free
```

All of the above run in CI on every push and pull request, and weekly, via [`.github/workflows/security.yml`](.github/workflows/security.yml).

The two accepted `cargo audit` warnings are `RUSTSEC-2024-0429` (`glib` unsoundness — the advisory is patched only at `glib >= 0.20.0`, while `tauri 2.12.3` and even `tauri 3.0.0-alpha.4` require `gtk ^0.18`, so no upgrade path exists today) and `RUSTSEC-2024-0370` (`proc-macro-error` unmaintained, build-time only, `patched = []`). Neither is reachable from application code. Full reasoning and measurements: [`security-audit-report.md`](./security-audit-report.md) §13.5–§13.6.

Covered by unit tests: progress and speed calculation, ETA sample thresholds, filename conflict handling, non-clobbering publish on hard-link-less filesystems, path sanitisation, partial-file cleanup, cancellation, disconnect handling, tool-discovery order and executability, and every startup-check state (dependency present/missing, service stopped vs. not installed, permission denied, device disconnected, pairing required, unsupported platform, malformed device response, tool present but not runnable).

### Optional pre-commit secret scan

```bash
git config core.hooksPath .githooks   # opt in; unset to disable
```

Hooks are local and never pushed, so this is a convenience, not a security boundary — the authoritative gate is the `secret-scan` CI job.

## Security auditing

The full assessment — severity ratings, code-level evidence, and the remediation record — is in [`security-audit-report.md`](./security-audit-report.md).

| | Current state |
| --- | --- |
| Confirmed Critical / High findings | **none** |
| Fixed after the assessment | **7**, including the silent-overwrite path and the disabled webview CSP |
| Accepted, with measurements | **2** — both upstream-only and unreachable from application code (§13.5–§13.6) |
| Secret material found in the repository | **none** |
| Shipped dependency advisories | **0 vulnerabilities** (138 npm packages, 422 Cargo packages) |

What runs automatically on every push and pull request, and weekly:

- `cargo audit` and `npm audit --omit=dev --audit-level=high`.
- `scripts/scan-secrets.sh` — offline and dependency-free. It prints only a rule name and `file:line`, never the matched text, so the scanner cannot itself leak a credential into a CI log.
- `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `npm run build`.

The workflow uses a single third-party action, pinned to a commit SHA rather than a mutable tag, and runs with read-only repository permissions. It also runs weekly, so an advisory published *after* the last commit still surfaces instead of going unnoticed until the next change.

**What has still not been assessed:** runtime behaviour against real hardware, the packaged artifacts, and macOS/Windows. §8 and §13.11 of the report list these gaps explicitly rather than implying coverage.

## Platform support

| Platform | Detection | Listing | Transfer | Packaged | Notes |
| --- | --- | --- | --- | --- | --- |
| **Linux** | ✅ verified | ✅ verified | ✅ verified | ✅ deb / rpm / AppImage | Development and verification platform. |
| **macOS** | ⚠️ untested | ⚠️ untested | ⚠️ untested | ❌ not configured | Expected to work with Homebrew libimobiledevice; needs a physical iPhone to confirm. |
| **Windows** | ⚠️ untested | ⚠️ untested | ⚠️ untested | ❌ not configured | Do not claim support until tested with a real iPhone. |

## Current status and limitations

- Implemented and verified locally on **Linux**: detection, Trust/pairing states, media listing, streaming transfer with progress, previews, conflicts, cancellation, and disconnect handling (at unit level).
- **Physical-device validation on each target OS is still required** before claiming cross-platform readiness.
- iOS may expose no files over AFC when iCloud Photos is set to **Optimize iPhone Storage** and the phone is locked. The app reports this honestly instead of inventing content.
- iOS does not expose its entire filesystem. Only files iOS itself exposes over the supported services are listed; private app and system files are out of scope.
- macOS and Windows builds are not produced yet, and there is no automated release workflow. A CI pipeline does exist, but it covers dependency/secret scanning and the quality gates only — it does not build or publish artifacts.
- Cancellation and disconnect are handled per item, but resuming an interrupted transfer is not guaranteed.

## Changelog

Release history lives in [`CHANGELOG.md`](CHANGELOG.md), and each release's notes are kept in [`docs/releases/`](docs/releases/). In brief:

| Version | Highlights |
| --- | --- |
| **0.2.0** | Notification centre, icon and design systems, responsive layout, webview CSP enabled, security assessment and CI quality gates, committed lockfiles |
| **0.1.0** | First public release: device/Trust detection, accessible media listing, streaming transfer with truthful progress, safe conflict handling |

## Roadmap

Explicitly out of scope for v1, in rough order of likely value:

- Physical-device validation on macOS and Windows, then signed installers for both.
- A GitHub Actions **release** workflow that builds and attaches the three Linux artifacts automatically on every tag. (Dependency and secret scanning already run in CI.)
- Optional content-comparison for same-named destination files (today the app only handles name conflicts and never claims two same-named files are identical).
- Computer-to-iPhone transfer, once supported destinations and their UX are validated.
- Resume across a disconnect, where the underlying protocol can support it.

## Contributing

1. Read [`AGENTS.md`](./AGENTS.md) — it is the binding brief, and it lists what is deliberately out of scope for v1.
2. Keep the `DeviceProvider` / `MediaSource` seam intact so everything stays testable without a physical phone.
3. Run the [quality gates](#quality-gates) before opening a pull request — CI enforces the same set.
4. Label any mock or demo data explicitly. Never invent live values.
5. Test with a physical iPhone before claiming a new platform works.
6. Never commit a credential. `bash scripts/scan-secrets.sh` checks tracked files locally, and you can wire it into `git commit` with `git config core.hooksPath .githooks`. If a secret does get pushed, rotate it first — removing the file is not enough once it is public.

## License

This project does not declare an application license yet — a `LICENSE` file has not been added, so all rights are reserved by the authors until one is chosen.

Third-party dependencies and how they are shipped:

- Bundled Rust crates are MIT / Apache-2.0.
- `libimobiledevice` (LGPL-2.1) and `usbmuxd` (GPL-2.0) are invoked as user-installed programs and are never bundled or modified.

Full audit: [`THIRD_PARTY_NOTICES.md`](./THIRD_PARTY_NOTICES.md).
