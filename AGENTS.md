# Agent Build Brief — Local iPhone USB Transfer MVP

## Mission
Build a minimal, beginner-friendly, offline-first desktop application that copies accessible iPhone photos and videos to a user-selected folder over a USB cable. Target Windows, macOS, and Linux. The first release is a local file-copy utility, not a cloud service or full iPhone backup tool.

## Architecture decision
- Desktop UI: React + TypeScript + Vite.
- Desktop shell and native command bridge: Tauri 2.
- Native orchestration and file I/O: Rust.
- iPhone USB communication: libimobiledevice/libusbmuxd where the supported platform/device path works.
- No application server, account system, analytics, cloud storage, or network transfer.
- Do not send file names, metadata, or file contents off the user's computer.
- Use a narrow Rust abstraction (`DeviceProvider` / `MediaSource`) so platform-specific USB details do not leak into UI components.

## MVP scope — must ship
1. Detect when an iPhone is connected, paired/trusted, disconnected, or unavailable.
2. Show simple setup guidance: connect cable, unlock iPhone, approve Trust prompt if shown.
3. List supported/accessibly exposed photos and videos. Do not claim access to the whole iOS filesystem.
4. Let the user select a destination directory on the computer.
5. Allow selecting individual files and a clear “Transfer selected” / “Transfer all new files” action.
6. Copy in bounded chunks/streams. Never load entire large files into memory.
7. Show per-file and overall progress, bytes copied, instantaneous/average speed, and estimated remaining time when enough data exists.
8. Show a clear success summary or actionable error.
9. Never delete or modify source files on the iPhone.
10. Keep the UI responsive during transfers.

## Explicitly out of scope for v1
- Cloud sync, accounts, remote servers, telemetry.
- Full iPhone backup/restore.
- Arbitrary access to private app folders or iOS system directories.
- Computer-to-iPhone transfer until supported destinations and UX are validated.
- Persistent transfer analytics, advanced history, smart cross-folder duplicate search, automatic resume across every protocol.
- Fancy notification center or extensive settings.

## UX requirements
- Mac-inspired, restrained interface: sidebar or simple sections, clear hierarchy, system font, generous spacing, accessible contrast, keyboard navigation.
- Primary screen has only: device status, supported file list, destination folder, transfer action, progress/status.
- Non-technical language. Explain “Unlock your iPhone and tap Trust” when required.
- States: `NoDevice`, `NeedsTrust`, `Ready`, `Scanning`, `Transferring`, `Completed`, `Disconnected`, `Error`.
- Disable invalid actions while scanning/transferring.
- Do not show invented live values in production. Any demo/mock values must be explicitly marked as demo.
- If a USB connection is lost: stop the current operation safely, preserve completed files, mark the active item interrupted, and tell the user to reconnect. Resume is not guaranteed unless the selected protocol explicitly supports it.
- Destination filename conflicts must never silently overwrite. Offer skip or rename; safe default is skip.
- Show an accurate message if the platform cannot expose a particular file category.

## Native architecture
Create these Rust modules (names may be improved if rationale is documented):
- `device`: enumerate connected iOS devices and connection events.
- `media`: enumerate media exposed by the selected supported service.
- `transfer`: bounded-buffer copy, progress events, cancellation, and safe partial-file handling.
- `errors`: stable, UI-safe error codes and user-friendly messages.
- `platform`: platform-specific discovery/dependency handling.

Define a `DeviceProvider` trait so tests can use a fake provider. Keep C FFI or subprocess interaction isolated. Prefer maintained libraries and documented protocols. Do not shell out with user-controlled strings. If calling command-line utilities, pass arguments without a shell and validate paths.

## Transfer correctness
- Copy to a temporary file in the destination directory, flush/sync when appropriate, then rename to final path only after success.
- On failure/disconnect, preserve or clean up partial files according to a documented policy; never present a partial file as completed.
- Use bounded buffers (start around 1 MiB; benchmark before tuning).
- Calculate throughput from bytes transferred over a moving time window. Avoid unstable ETA before sufficient samples exist.
- Preserve source creation/modification metadata only when available and representable on the target filesystem. Do not invent missing dates.
- Treat filenames as untrusted input. Prevent path traversal and sanitize invalid destination filename characters.
- For duplicates in MVP, use destination-name conflict handling only. Do not claim two same-named files are identical without content comparison.

## Platform and iOS constraints
- iOS does not expose the entire filesystem to desktop apps. Implement only documented/supported access paths.
- Media access may use platform-specific mechanisms. Validate actual photo/video enumeration on physical devices and current iOS versions.
- `libimobiledevice` integration, device pairing, and USB muxing differ by OS. Do not claim Windows support until tested on Windows with a real iPhone.
- Document platform prerequisites and bundle/install native dependencies where legally and technically appropriate.
- Test physical iPhones, not only simulators.

## Security and privacy
- All transfers remain local. No telemetry or network requests.
- Request only necessary permissions.
- Never log file contents or sensitive full paths by default.
- Do not bypass iOS pairing, sandbox, or user-consent mechanisms.
- Show clear errors for unsupported access rather than attempting private filesystem access.

## Quality gates
- TypeScript typecheck and lint pass.
- Rust formatting and tests pass.
- Unit tests cover progress calculation, filename conflict handling, path sanitization, partial-file cleanup, cancellation, and disconnect handling.
- Integration test the flow on Windows, macOS, and Linux with a physical iPhone.
- Test large files and low-disk-space errors.
- Test disconnect during scan and during copy.
- No false “Completed” status unless final file is present and finalized.

## Build order
1. Inspect this scaffold and document platform-specific dependency strategy.
2. Implement UI states with a fake device provider for predictable UI development; label mock mode clearly.
3. Implement Rust transfer engine and unit tests independently of USB.
4. Add USB device detection and supported media enumeration through native adapter.
5. Connect UI to Tauri commands/events.
6. Test and package one OS at a time; do not claim cross-platform readiness before testing each target.
7. Keep v1 focused; propose additional features only after the core flow works reliably.

## Definition of done
A user can install the app, connect and trust an iPhone, see supported photos/videos, choose a local folder, copy selected items with truthful progress, and get a clear result. The app works without a network connection and never deletes source files.
