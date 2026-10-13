# Local iPhone Transfer

A local-only desktop app that copies accessible iPhone photos and videos to a folder on your computer over a USB cable. Nothing leaves your machine: no server, no account, no telemetry, no network transfer of any kind.

## Architecture

- **React + TypeScript + Vite** — user interface ([src/App.tsx](src/App.tsx), typed backend bridge in [src/api.ts](src/api.ts))
- **Tauri 2 + Rust** — desktop shell and native orchestration
- **libimobiledevice command-line tools** (`idevice_id`, `ideviceinfo`, `idevicepair`, `afcclient`) — USB device detection, pairing state, and AFC file access. Invoked without a shell, with fixed argument vectors only.
- **No application server, accounts, analytics, cloud storage, or network transfer.**

Read [`AGENTS.md`](./AGENTS.md) before implementing features. It is the implementation brief and acceptance criteria.

## For end users (Linux)

1. Install the package for your distribution (no Rust, Node.js, or developer
   tools needed on the user's machine):
   - Debian/Ubuntu: `sudo apt install ./local-iphone-transfer_0.1.0_amd64.deb`
     (apt pulls in `libimobiledevice-utils` and `usbmuxd` automatically)
   - Fedora: `sudo dnf install ./Local iPhone Transfer-0.1.0-1.x86_64.rpm`
   - Any distro: run the AppImage (bundled libraries; larger download)
2. Start the app. A **setup screen** checks every prerequisite and says, in
   plain language, exactly what is missing and how to fix it:
   - *Preparing your iPhone connection...* — the check is running
   - *Your computer is ready.* — connect and unlock the iPhone
   - *USB support needs setup.* — the libimobiledevice tools are missing,
     with copy-paste install commands
   - *iPhone connection service is not running.* — usbmuxd is stopped or
     unreachable (a stale socket counts as stopped, not missing)
   - *Unlock your iPhone and approve the Trust prompt.* — pairing pending
   - *No iPhone detected.* — connect a data-capable cable and unlock
3. Every screen has **Check again** and a collapsed **Details for support**
   section with the typed diagnostics (never raw stack traces). The app never
   installs anything, starts services, or runs `sudo` on its own.

### Troubleshooting

| Symptom | What to do |
| --- | --- |
| Setup screen says the USB service is not running | Start usbmuxd with your service manager, e.g. `sudo systemctl start usbmuxd`, then click **Check again**. |
| Setup screen says tools are missing | Install libimobiledevice with your package manager (the screen shows the exact command). On Debian/Ubuntu, if `afcclient` is still missing, build it from the libimobiledevice sources (see the project site). |
| "Waiting for Trust approval" won't clear | Unlock the phone, keep it unlocked, accept the Trust dialog. A fresh cable (data-capable, not charge-only) helps. |
| iPhone detected but no files listed | iOS exposes no files over AFC while iCloud Photos is set to "Optimize iPhone Storage" and the phone is locked. Keep the phone unlocked and check again. |
| Phone keeps disconnecting | Try another cable/port; avoid USB hubs. The app marks interrupted files and never reports a partial copy as complete. |

## Rust module layout (`src-tauri/src/`)

| Module | Responsibility |
| --- | --- |
| `device.rs` | Device model and the narrow `DeviceProvider` / `MediaSource` seam (fakes for tests) |
| `media.rs` | Media item DTOs and enumeration traits |
| `provider_libimobile.rs` | Real adapter: detection, pairing/Trust classification, AFC listing and reads |
| `provider_mock.rs` | Development-only mock adapter (explicitly labelled **demo** in the UI) |
| `transfer.rs` | Bounded-buffer copy, progress events, cancellation, conflict handling, partial-file policy |
| `progress.rs` | Rolling-window speed / ETA calculation (no invented values) |
| `naming.rs` | Filename sanitisation, path-traversal prevention, conflict resolution (skip/rename) |
| `errors.rs` | Stable UI-safe error codes and user-friendly messages |
| `platform.rs` | Platform-specific tool discovery and install hints |
| `environment.rs` | Startup prerequisite check with explicit states (`ready`, `dependency_missing`, `service_not_running`, `permission_required`, `pairing_required`, `device_not_connected`, `unsupported_platform`, `error`) |
| `lib.rs` | Tauri commands, events, app state |

## Transfer correctness policy

- Every file is copied to a hidden temporary file in the destination folder, flushed with `sync_all`, verified against the expected byte count, and only then published under its final name.
- On failure, cancellation, or USB disconnect the temporary file is deleted; the item is reported as failed/interrupted — never as completed.
- Destination name conflicts never silently overwrite. Default policy is **skip**; **rename** is optional.
- Bounded 1 MiB read buffer — large files are streamed, never loaded whole.
- Speed and ETA come from real samples over a moving window; the UI shows "—" / "Estimating…" until enough data exists.
- Source files on the iPhone are never deleted or modified.

## Platform prerequisites (developers)

Install Node.js LTS, Rust stable, and the Tauri v2 prerequisites for your OS: https://v2.tauri.app/start/prerequisites/

Install the libimobiledevice tools:

- **Linux (Arch):** `sudo pacman -S libimobiledevice` (provides `idevice_id`, `ideviceinfo`, `idevicepair`, `afcclient`)
- **Linux (Debian/Ubuntu):** `sudo apt install libimobiledevice-utils usbmuxd` (`afcclient` may need to be built from libimobiledevice sources on older releases)
- **Linux (Fedora):** `sudo dnf install libimobiledevice usbmuxd`
- **macOS:** `brew install libimobiledevice`
- **Windows:** not yet supported/validated — do not claim Windows support until tested with a real iPhone.

The app's **Help** panel, the `diagnostics` command, and the startup `check_environment` command all report which tools were found and how to install the missing ones.

## Startup environment check

`check_environment` (Rust: `environment.rs`) runs a read-only, one-shot probe:
toolchain present → usbmuxd reachable (real socket connect, so a stale
socket is reported as *stopped*, not *missing*) → iPhone listed by
`idevice_id` → trust verified via `idevicepair validate` (with a direct
lockdown query as fallback). It returns the explicit states
`ready` / `device_not_connected` / `dependency_missing` / `service_not_running` /
`permission_required` / `pairing_required` / `unsupported_platform` / `error`
plus a per-check list the setup screen renders. The mock adapter is
development-only; release builds refuse mock mode.

## Run

```bash
npm install
npm run dev        # frontend only (browser preview — device features disabled)
npm run tauri dev  # full desktop app
```

### Demo mode

With no iPhone handy, enable **Demo mode** in the sidebar to exercise the full UI against a clearly labelled mock device (including Needs-Trust and No-Device states). Demo data is always marked as such in the interface.

## Build and release packaging

```bash
npm run tauri build
```

Produces (verified on Linux x86-64):

- `src-tauri/target/release/local-iphone-transfer` — release binary (12 MB)
- `…/bundle/deb/Local iPhone Transfer_0.1.0_amd64.deb` (3.2 MB)
- `…/bundle/rpm/Local iPhone Transfer-0.1.0-1.x86_64.rpm` (3.2 MB)
- `…/bundle/appimage/Local iPhone Transfer_0.1.0_amd64.AppImage` (101 MB)

The bundle icons are declared in `src-tauri/tauri.conf.json` (`bundle.icon`); without them the
AppImage step fails with "couldn't find a square icon".

Packaging notes:

- The deb/rpm declare dependencies on the distribution's `libimobiledevice*`
  and `usbmuxd` packages instead of bundling third-party binaries, and ship
  `THIRD_PARTY_NOTICES.md` under `/usr/share/doc/local-iphone-transfer/`.
- The AppImage bundles the build host's libraries (see the licensing table
  in [`THIRD_PARTY_NOTICES.md`](./THIRD_PARTY_NOTICES.md)).
- Linux artifacts are unsigned; no code-signing infrastructure is configured.
- **macOS and Windows packaging is not configured or validated** — do not
  claim support until a physical iPhone has been tested on each OS.

Licensing: bundled Rust crates are MIT/Apache-2.0; `libimobiledevice`
(LGPL-2.1) and `usbmuxd` (GPL-2.0) are invoked as user-installed programs,
never bundled or modified. Full audit in
[`THIRD_PARTY_NOTICES.md`](./THIRD_PARTY_NOTICES.md).

## Quality gates

```bash
npm run build                # tsc -b && vite build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets   # 0 warnings
cargo test                   # unit tests
```

Covered by unit tests: progress/speed calculation, filename conflict handling, path sanitisation, partial-file cleanup, cancellation, disconnect handling, and every startup-check state (dependency present/missing, service stopped vs not installed, permission denied, device disconnected, pairing required, unsupported platform, malformed device response, tool present but not runnable).

## Current status and limitations

- Implemented and verified locally on **Linux**: detection, Trust/pairing states, media listing, streaming transfer with progress, conflicts, cancellation, and disconnect handling (unit level).
- **Physical-device validation on each target OS is still required** before claiming cross-platform readiness. iOS may not expose any files over AFC when iCloud Photos is set to "Optimize iPhone Storage" and the phone is locked — the app reports this honestly instead of inventing content.
- iOS does not expose its entire filesystem. Only files iOS itself exposes over the supported services are listed; private app and system files are out of scope.
