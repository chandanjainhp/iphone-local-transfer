# Third-Party Notices

This file lists the third-party components that **Local iPhone Transfer**
uses, their exact versions where they are pinned, their licenses, and how they
are shipped. Audited against the actual dependency graph
(`src-tauri/Cargo.lock`, 422 pinned crate packages) and the runtime
toolchain on the build/test machine (Arch Linux, `libimobiledevice 1.4.0-2`,
`usbmuxd 1.1.1-4`).

Distribution policy in one line: **the deb/rpm bundles no third-party
binaries at all** (it depends on distribution packages instead); the
AppImage necessarily bundles distribution libraries and is documented
accordingly below.

---

## 1. Components bundled inside the application binary

### Rust crates (compiled into `local-iphone-transfer`)

All crates in `src-tauri/Cargo.lock` are MIT **or** Apache-2.0 licensed
(dual-licensed); several are LGPL/GPL-adjacent only in their *system*
counterparts, not the Rust bindings. Direct dependencies, with the versions
pinned by the lockfile:

| Crate | Version | License | Source |
| --- | --- | --- | --- |
| `tauri` | 2.12.3 | MIT or Apache-2.0 | https://github.com/tauri-apps/tauri |
| `tauri-plugin-dialog` | 2.x (locked) | MIT or Apache-2.0 | https://github.com/tauri-apps/plugins-workspace |
| `serde` | 1.0.229 | MIT or Apache-2.0 | https://github.com/serde-rs/serde |
| `serde_json` | 1.x (locked) | MIT or Apache-2.0 | https://github.com/serde-rs/json |
| `filetime` | 0.2.29 | MIT or Apache-2.0 | https://github.com/calebgeorge/filetime |
| `libc` | 0.2.190 | MIT or Apache-2.0 | https://github.com/rust-lang/libc |

The transitive closure (422 pinned packages) carries the same permissive
policy; no copyleft crate is linked into the binary in a way that triggers
redistribution obligations beyond attribution. To regenerate the full list:

```bash
cd src-tauri && cargo tree --duplicates | head      # overview
grep -A2 '^name = ' Cargo.lock                      # exact pinned versions
```

Rust crates licensed "MIT OR Apache-2.0" require only that the binary
retains their copyright notices; no source-offer obligation applies to
permissively licensed code.

### Application resources

The application bundle includes first-party code (Rust backend, React
frontend) and first-party icons only. No third-party fonts are bundled: the
UI uses the system font stack.

---

## 2. Runtime dependencies **not** bundled (installed by the user or distro)

These are invoked as separate programs or run as OS services. They are not
redistributed by this project's installers, so no license text is
redistributed with them; their own packages carry their own notices. Listed
here for transparency and attribution.

| Component | Version tested | License | Role | Source |
| --- | --- | --- | --- | --- |
| `libimobiledevice` (tools `idevice_id`, `ideviceinfo`, `idevicepair`, `afcclient`) | 1.4.0 | LGPL-2.1 | USB detection, pairing/Trust, AFC media reads | https://github.com/libimobiledevice/libimobiledevice |
| `usbmuxd` | 1.1.1 | GPL-2.0 | System service that multiplexes USB to lockdown | https://github.com/libimobiledevice/usbmuxd |
| `libusbmuxd` | 2.1.1 | LGPL-2.1 | Client library used by the tools above | https://github.com/libimobiledevice/libusbmuxd |

Notes:

- The tools are **executed as separate processes with fixed argument
  vectors** (never loaded into this app's address space, never invoked
  through a shell). LGPL-2.1 and GPL-2.0 obligations for executed
  programs — including the user's right to replace them — are satisfied by
  depending on the distribution's packages rather than bundling.
- If a future release **does** bundle these binaries (e.g. a portable
  Windows zip), it must ship the corresponding LGPL-2.1/GPL-2.0 license
  texts, an attribution file, and a corresponding-source offer covering the
  exact bundled versions. That release must update this file accordingly.
  This release does **not** bundle them.
- The tools are not modified by this project. The only behavioural
  accommodation is passing the standard `POSIXLY_CORRECT=1` environment
  variable when spawning them so GNU-style option permutation cannot
  reorder arguments; this is a runtime environment flag, not a
  modification of their source.

### Apple components

On macOS and Windows, device access relies on Apple's own device-support
stack (part of macOS; iTunes / Apple Mobile Device Support on Windows).
Those are proprietary, are obtained from Apple, and are **not** bundled or
modified by this project.

---

## 3. Packaging-specific notes

| Artifact | Third-party content | Obligations handled |
| --- | --- | --- |
| `.deb` / `.rpm` | None bundled; declares dependencies on distro packages (`libimobiledevice*`, `usbmuxd`) so the package manager installs them with their own notices | None beyond this attribution |
| `.AppImage` | Bundles the build host's runtime libraries (glibc, GTK3, WebKitGTK and their X11/Wayland stack) plus the AppImage runtime | The bundled libraries are LGPL-2.1+/LGPL-3.0+; their license texts and corresponding source remain available via the distribution's source packages (e.g. `pacman -Si` / Ubuntu source archives). The AppImage runtime is MIT with LGPL portions — see https://github.com/AppImage/AppImageKit. Users can extract the AppImage (`--appimage-extract`) to inspect or replace the bundled libraries, satisfying LGPL relink/replace expectations. |
| Release binary alone | Static Rust runtime + system dynamic libraries | Attribution only (permissive crates) |

Code signing: Linux artifacts are **unsigned** (no signing infrastructure is
configured for this project). macOS and Windows releases are not published
by this project and would require Developer ID / Authenticode signing plus
notarization before distribution.

---

## 4. Compliance checklist for future releases

1. Pin dependency versions via `Cargo.lock` and `package-lock.json` (done).
2. Re-audit licenses before adding any new native dependency or bundling
   any new binary (`cargo tree`, distro package licenses).
3. If bundling `libimobiledevice`/`usbmuxd` binaries for the first time,
   add their full license texts and a corresponding-source offer to the
   release, and update this file with exact versions.
4. Keep this file in the repository root and ship it inside any archive
   that contains bundled libraries (the AppImage already embeds this
   repository's contents; the deb/rpm should install it under
   `/usr/share/doc/local-iphone-transfer/` — provided by the packaging
   step from this file).
