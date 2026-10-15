# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [semantic versioning](https://semver.org/spec/v2.0.0.html)
(`0.x` releases are pre-1.0: the CLI surface is stable, the UI is not frozen).

## [0.2.0] — 2026-10-15

The first release to include the full interface work, the security assessment and
the automated quality gates. No transfer-protocol changes: the copy engine keeps
the same non-destructive, non-clobbering semantics as `0.1.0`.

### Added

- **Notification centre** — a local, offline store ([`src/notifications.ts`](src/notifications.ts))
  built from pure functions and a reducer, with an inline `role="alert"` banner, a
  non-modal popover panel, `All` / `Unread` filters with counts derived from the
  rows actually in scope, per-category filtering, collapse-by-`dedupeKey`,
  dismissal, and bounded `localStorage` persistence (50 rows, treated as untrusted
  input on read). Every row comes from an event the app genuinely produces; nothing
  is fabricated and no file contents or full source paths are stored.
- **Icon system** — one semantic registry ([`src/icons.tsx`](src/icons.tsx)) over
  `@phosphor-icons/react`, a fixed size ladder, and accessibility enforced by types
  (an unlabelled icon is emitted `aria-hidden`; `IconButton` requires a label).
- **Design system** — a single token block in [`src/styles.css`](src/styles.css)
  for light and dark appearance: one interactive accent colour, a type ladder,
  documented radii and spacing steps, one elevation recipe, and `scale(0.95)` press
  feedback. No rule hard-codes a colour, size, radius or spacing value.
- **Responsive layout** — two documented width tiers (≤ 1023 px: narrower rail, no
  date column; ≤ 767 px: single column, wrapping sidebar rail, stacked topbar,
  viewport-anchored notification sheet), plus `auto-fit` stat tiles, fluid
  `clamp()` / `min()` sizing, `min-width: 0` shrink guards, `100dvh`, safe-area
  insets, and 44 px touch targets under `@media (pointer: coarse)`.
- **Frontend test suite** — 72 tests: the notification store (63) and the
  responsive-layout guards (9), which read the stylesheet and lock the invariants
  above so the specific regressions cannot return unnoticed.
- **Security assessment** — [`security-audit-report.md`](security-audit-report.md):
  severity ratings, code-level evidence, and the remediation record.
- **CI and local gates** — [`.github/workflows/security.yml`](.github/workflows/security.yml)
  runs `cargo audit`, `npm audit`, the secret scan, `cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo test` and `npm run build` on every push and
  pull request and weekly; [`scripts/scan-secrets.sh`](scripts/scan-secrets.sh) is
  an offline, dependency-free scanner that reports only a rule name and `file:line`;
  [`.githooks/pre-commit`](.githooks/pre-commit) is an optional local gate.
- **Committed lockfiles** — `package-lock.json` and `src-tauri/Cargo.lock`, so
  builds are reproducible and dependency diffs are reviewable.
- **Dependency licence audit** — [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)
  records every bundled dependency and how it is shipped.

### Changed

- **README** rewritten into full project documentation: install, first run,
  troubleshooting, architecture, module and command tables, the design language,
  transfer-correctness policy, quality gates, and honest platform/limitation
  status.
- **Conflict policy surface** — destination-name conflicts are shown in the
  toolbar with an explicit default of **skip**; **rename** is available, and
  **replace** requires a confirmation dialog that states how many files will be
  replaced and that the replacement cannot be recovered.
- **Diagnostics** are typed, readable check lists instead of raw traces, and the
  setup screen names the exact failing prerequisite and the copy-paste fix.

### Fixed

- **Silent overwrite path removed** — publishing now claims the final name only
  after the temporary file has been flushed and its byte count verified, and on
  filesystems without hard links (FAT/exFAT, many network shares) the name is
  claimed with an exclusive create, so a file appearing during the copy is never
  replaced.
- **Webview CSP enabled** — the previous `csp: null` was replaced with
  `default-src 'self'` and no `unsafe-inline` / `unsafe-eval`, with a separate
  development policy for the Vite dev server.
- **Trusted tool discovery** — `libimobiledevice` binaries are resolved through a
  documented search order and checked for executability before use, with install
  hints when they are missing.
- **Page-level horizontal scrolling removed** — the root `min-width: 760px` that
  clamped narrow viewports is gone, and overflow is deliberately not hidden so a
  regression stays measurable.
- **Device card copy column** no longer collapses to one character per line inside
  the wrapping row.
- Copy at a chunk boundary during cancellation or disconnect is reported per item,
  and the partial file is deleted — a partial copy is never shown as completed.

### Security

- 0 critical and 0 high findings after remediation; 7 issues fixed during the
  assessment.
- 2 accepted `cargo audit` warnings (`RUSTSEC-2024-0429`, `RUSTSEC-2024-0370`),
  both upstream-only and unreachable from application code — reasoning and
  measurements in §13.5–§13.6 of the report.
- 0 vulnerabilities in shipped dependencies; no secret material found in the
  repository.

### Known limitations

- Linux x86-64 only. macOS and Windows are untested and unpackaged — do not
  claim support for them until verified with a physical iPhone.
- No automated release workflow: artifacts are built and attached by hand (see the
  release section of the README).
- Resuming an interrupted transfer is not guaranteed; completed files are always
  preserved.

## [0.1.0] — 2026-10-13

First public release, Linux x86-64 (`.deb`, `.rpm`, AppImage).

- Device detection with explicit states: `NoDevice`, `NeedsTrust`, `Ready`,
  `Scanning`, `Transferring`, `Completed`, `Disconnected`, `Error`.
- Plain-language setup screen for the cable, unlock and Trust steps.
- Listing of the photos and videos iOS itself exposes over USB, with iOS-generated
  thumbnails where the phone provides them.
- Destination folder picker, per-file selection, **Transfer Selected**, and
  **Transfer All New**, with skip/rename conflict handling and a safe default.
- Bounded 1 MiB streaming copy to a temporary file, published under its final name
  only after a flush and a byte-count check.
- Per-file and overall progress, bytes copied, live speed from a moving window, and
  an ETA only once enough samples exist.
- Cancellation and disconnect handling that never reports a partial file as
  completed, and never deletes or modifies anything on the iPhone.

[0.2.0]: https://github.com/chandanjainhp/iphone-local-transfer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/chandanjainhp/iphone-local-transfer/releases/tag/v0.1.0
