# Security Assessment Report

## Report: Local iPhone Transfer (`chandanjainhp/iphone-local-transfer`)

| Field | Value |
| --- | --- |
| **Date** | 2026-10-14 |
| **Assessor** | Buffy (Freebuff), using the OWASP Secure Agent Playbook |
| **Playbook** | `OWASP/secure-agent-playbook` v0.2.9, marketplace `agent-security-playbook` |
| **Scope** | Full repository at commit `3a03a98` (branch `master`), local static assessment |
| **Analysis type** | Static only — no exploitation, no live targets, no production systems, no application code modified |
| **Host** | Arch Linux (EndeavourOS), kernel 6.18.55-1-lts, x86-64 |

---

## 1. Executive Summary

**No Critical or High severity vulnerabilities were confirmed.** The project is a well-hardened, offline-first desktop utility, and several controls that are usually missing in this class of app are correctly implemented and unit-tested (device-path confinement to `/DCIM/`, filename sanitisation with traversal removal, shell-free process invocation, exclusive-create temp files, magic-byte image validation, and release-gated mock mode).

Two **Medium** findings are worth fixing before a wider release: a silent-overwrite path in the transfer publish fallback, and a disabled webview Content Security Policy. Both are defence-in-depth/loss-of-guarantee issues rather than directly exploitable remote bugs. Three **Low** and three **Informational** findings are also recorded.

The most consequential finding is that the app's headline safety promise — *"Destination filename conflicts must never silently overwrite"* — is **not upheld on filesystems without hard-link support** (FAT/exFAT/network shares — the common case for removable media). I confirmed the underlying mechanism with a standalone compiled probe rather than relying on documentation alone (§5.1).

Scanned dependency surface: **138 npm packages** and **422 Cargo packages**. Zero npm advisories; two Rust advisory records, neither reachable from application code (§5.6, §5.7).

**This report does not conclude the project is secure.** See §8 for everything that was not tested.

## 2. Findings Summary

| # | Severity | Title | CWE | OpenCRE | OWASP Ref | Status |
|---|----------|-------|-----|---------|-----------|--------|
| 1 | **MEDIUM** | Silent overwrite in transfer publish fallback on hard-link-less filesystems | CWE-367 | [CRE-380-540](https://www.opencre.org/cre/380-540) | A04:2021, ASVS V5.3 | **Fixed** (§13.1) |
| 2 | **MEDIUM** | Webview CSP disabled while privileged file-write IPC is exposed | CWE-693 | — | A05:2021 | **Fixed** (§13.2) |
| 3 | **LOW** | Insecure temporary file for preview download (predictable path, no exclusive create) | CWE-377 | — | A04:2021 | **Fixed** (§13.3) |
| 4 | **LOW** | Dependency graph not reproducible — both lockfiles are gitignored | CWE-1104 | [CRE-053-751](https://www.opencre.org/cre/053-751) | A06:2021 | **Fixed** (§13.4) |
| 5 | **LOW** | `glib` 0.18.5 unsoundness (RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g) | CWE-937 | [CRE-053-751](https://www.opencre.org/cre/053-751) | A06:2021 | **Accepted — no upstream fix exists** (§13.5) |
| 6 | **INFORMATIONAL** | `proc-macro-error` 1.0.4 unmaintained (RUSTSEC-2024-0370) | CWE-1104 | [CRE-053-751](https://www.opencre.org/cre/053-751) | A06:2021 | **Accepted — unmaintained, build-time only** (§13.6) |
| 7 | **INFORMATIONAL** | Panic on poisoned mutex disables the whole command surface | CWE-248 | — | A04:2021 | **Fixed** (§13.7) |
| 8 | **INFORMATIONAL** | Tool discovery prefers a `tools/` dir beside the executable; no executable-bit check | CWE-426 | — | A08:2021 | **Fixed** (§13.8) |
| 9 | **INFORMATIONAL** | `.gitignore` has no `*.key` / `*.pem` / `credentials*` patterns | CWE-538 | — | A05:2021 | **Fixed** (§13.9) |

Totals: **Critical 0 · High 0 · Medium 2 · Low 3 · Informational 4 · Total 9**. Secrets findings: **0** (§5.10).

Remediation: **7 fixed**, **2 accepted with measurement** (both upstream-only, neither reachable from application code). Statuses were revised in the §13 addendum of 2026-10-14 after the fixes were implemented and re-verified.

## 3. Assessment Scope and Methodology

### 3.1 Target understanding

Establishing scope first (per `code-review-security.md` §1):

| Attribute | Finding |
| --- | --- |
| Language / framework | Rust (Tauri 2 backend) + TypeScript/React 18 (Vite frontend) |
| Trust boundary | Desktop app; the **webview is the untrusted-input boundary**, the Rust side is privileged |
| Exposure | **Local-only.** No server, no listening socket, no external network client |
| Data sensitivity | User photos/videos; device-reported filenames and paths; user's local directory paths. No credentials, PII, or financial data |
| Process boundary | Shell-free invocation of four user-installed `libimobiledevice` binaries |

### 3.2 Methodology

Assessment followed the playbook plays listed in §4, applied in order: scope → automated scanning → manual review by vulnerability class (injection, authn/session, authz, cryptography, data exposure, misconfiguration, deserialisation) → contextual validation of every candidate finding against source.

Every finding below required **either** tool output **or** a source-code path traced end-to-end. Findings requiring a condition I could not exercise locally are labelled and explained; nothing is reported from pattern-matching alone.

### 3.3 Tooling actually used

| Tool | Version | How obtained | Purpose |
| --- | --- | --- | --- |
| `osv-scanner` | 2.7.0 | Official release binary from `github.com/google/osv-scanner` (verified ELF, ran `--version`) | SCA across npm + Cargo lockfiles |
| `cargo-audit` | 0.22.2 | `cargo install cargo-audit --locked` | Independent Rust advisory cross-check |
| `npm audit` | bundled with npm | Already present | Independent npm advisory cross-check |
| `python3` | 3.x | Already present | Lockfile parsing, JSON analysis, OSV/registry API queries |
| `rustc` | system | Already present | Standalone probe to verify rename/hard-link semantics (§5.1) |
| `git`, `gh`, `jq`, `curl` | system | Already present | History scan, repo/registry metadata, release verification |

**Tools not available and not used:** Semgrep (no `pip` on host), Trivy, TruffleHog, Gitleaks, Bandit, CodeQL. The playbook's `sca-audit.md` suggests `npm install -g osv-scanner`, but **no official npm package by that name exists** (registry returns `{"error":"Not found"}`); the npm search results are unrelated third-party wrappers, which I deliberately did not install. I used Google's official release binary instead.

### 3.4 Guardrails observed

- No production systems, live targets, or the user's iPhone were touched.
- No application code, dependency, or configuration was modified.
- `osv-scanner --call-analysis=rust` was **declined** because the tool documents that it "will run build scripts". I substituted a manual reverse-dependency walk (§5.11).
- No untrusted package or script was installed or executed; the only downloads were two signed-out artefacts from official Google/RustSec sources, both version-verified.

## 4. Tools and OWASP Playbook Skills Used

Installed via the documented Claude Code marketplace procedure (verified — see §9):

| Plugin | Skills installed | Agents installed |
| --- | --- | --- |
| `code-security-skills@agent-security-playbook` | 11 (`code-review-security`, `sca-audit`, `secrets-scan`, `api-security-review`, `web-security-review`, `mobile-code-review`, `iac-security-review`, `securability-engineering`, `securability-engineering-review`, `prd-securability-enhancement`, `security-guidance`) | 5 (`code-security-reviewer`, `dependency-auditor`, `api-security-reviewer`, `mobile-security-reviewer`, `security-team-lead`) |
| `ai-security-skills@agent-security-playbook` | 7 (`agent-security-audit`, `llm-risk-assess`, `agentic-ai-risk-assess`, `mcp-server-review`, `prompt-injection-test`, `multi-agentic-threat-model`, `ai-security-verification`) | 1 (`ai-security-assessor`) |

Plays applied to this target:

| Play | Applied? | Notes |
| --- | --- | --- |
| `code-review-security.md` | ✅ Applied | Primary procedure; all classes worked |
| `sca-audit.md` | ✅ Applied | Steps 1–5 incl. dependency health |
| `secrets-scan.md` | ✅ Applied | Manual pattern analysis (no scanner installed) + git-history pass |
| `owasp-top10-web-review.md` | ✅ Partially | A05/A06/A08 applied; A01/A02/A03/A07/A09/A10 largely N/A (see §8) |
| `agent-security-audit.md` | ✅ Applied | Scoped to the repo's dev-tooling agent surface (§6) |
| `api-security-review.md` | ⚠️ Partially | Tauri IPC commands reviewed as the "API"; OWASP API Top 10 largely N/A — no HTTP surface |
| `mcp-server-review.md` | ⚠️ N/A | No MCP server in the project (§6) |
| `prompt-injection-test.md`, `llm-risk-assess.md`, `agentic-ai-risk-assess.md`, `multi-agentic-threat-model.md`, `ai-security-verification.md` | ❌ Not applicable | Product contains no LLM or agent runtime |
| `iac-security-review.md` | ❌ Not applicable | No Terraform, Kubernetes, CloudFormation, Dockerfile, or CI workflow present |
| `mobile-code-review.md` | ❌ Not applicable | Not a native Android/iOS app; the "iPhone" side is a USB peripheral, not reviewed code |

## 5. Findings Detail

Findings are grouped by severity. Each finding records title, severity, confidence, affected file and line, evidence, security impact, CWE and OWASP mapping, remediation, and verification steps.

| Severity group | Count | Findings |
| --- | --- | --- |
| **CRITICAL** | **0** | None identified. No remote code execution, no system compromise, no mass data exposure path was found. |
| **HIGH** | **0** | None identified. No privilege escalation, authentication bypass, or significant data exposure was found. |
| **MEDIUM** | **2** | LIT-2026-001 (silent overwrite), LIT-2026-002 (CSP disabled) |
| **LOW** | **3** | LIT-2026-003 (insecure temp file), LIT-2026-004 (unpinned dependency graph), LIT-2026-005 (glib advisory) |
| **INFORMATIONAL** | **4** | LIT-2026-006 (unmaintained crate), LIT-2026-007 (mutex-poison panic), LIT-2026-008 (tool search path), LIT-2026-009 (gitignore secret patterns) |

> Sections 5.10–5.11 (secrets scan, dependency audit) are supporting analyses rather than separate findings and are grouped under Informational for readability. The agent security audit is §6. Findings LIT-2026-001 … LIT-2026-009 are the nine items summarised in §2.

### Medium

#### 5.1 [MEDIUM] Silent overwrite in transfer publish fallback on hard-link-less filesystems

- **ID**: LIT-2026-001
- **CWE**: [CWE-367](https://cwe.mitre.org/data/definitions/367.html) — Time-of-check Time-of-use (TOCTOU)
- **OpenCRE**: [CRE-380-540](https://www.opencre.org/cre/380-540) — thread safety / resistance to race conditions; [CRE-134-412](https://www.opencre.org/cre/134-412) — protect sensitive functionality against race conditions
- **OWASP Ref**: A04:2021 Insecure Design; ASVS V5.3 (File Storage)
- **Location**: `src-tauri/src/transfer.rs:167-186` (`publish`; `PublishError` declared at `:162`), reached from `run_one` at `src-tauri/src/transfer.rs:266+`
- **Impact**: On a destination filesystem that does not support hard links, a file that appears at the target name after the conflict check is **silently replaced**, permanently destroying the user's existing file. This contradicts the project's documented guarantee in `AGENTS.md` ("Destination filename conflicts must never silently overwrite") and in the README.
- **Evidence**: The no-clobber guarantee is enforced only by the `hard_link` call:

  ```rust
  // src-tauri/src/transfer.rs:167-186
  fn publish(tmp: &Path, target: &Path) -> Result<(), PublishError> {
      // hard_link fails with AlreadyExists when something else won the race —
      // it never clobbers. Same directory, so no cross-device issue.
      match std::fs::hard_link(tmp, target) {
          Ok(()) => { let _ = std::fs::remove_file(tmp); return Ok(()); }
          Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
              return Err(PublishError::Exists)
          }
          Err(_) => { /* filesystems without hard links: fall back to rename */ }
      }
      match std::fs::rename(tmp, target) {   // <-- replaces an existing target
          Ok(()) => Ok(()),
          ...
      }
  }
  ```

  The fallback branch is reached for **any** error other than `AlreadyExists`. On FAT/exFAT/`vfat`, `link(2)` fails with `EPERM`; on many network filesystems it fails with `EOPNOTSUPP`/`ENOTSUP` — none of which is `AlreadyExists`, so control falls through to `rename`.

  **Measured proof of the two semantics** (standalone probe compiled with `rustc` in `/tmp`, not touching project files):

  ```
  hard_link onto existing -> Some(AlreadyExists)
  after hard_link, victim = "ORIGINAL-USER-DATA"     <-- primary path is safe
  rename -> Ok (no error)                            <-- no error is raised
  after rename, victim = "NEW"                       <-- user data silently destroyed
  ```

  The re-check `if target.exists()` further down only runs on the rename *error* path; because `rename` returns `Ok(())` it never executes.

  **Race window**: the window is the entire duration of the file copy — not a narrow instant — because the conflict check happens before the streaming copy begins (`run_one` calls `resolve_conflict` well before `source.copy_to`). For a multi-gigabyte video this is tens of seconds to minutes.

  **Preconditions**: (a) destination filesystem without hard-link support, and (b) a file appearing at the resolved name during the copy. `ConflictPolicy::Replace` is not affected — that branch intends to overwrite.

  > **Not measured**: I could not exercise FAT/exFAT locally — `grep -E 'vfat|exfat|ntfs' /proc/mounts` found no such mount and mounting a test image requires elevated privileges, which I did not request. The `rename`-clobbers behaviour is measured; the hard-link *failure* on those filesystems is reasoned from documented `link(2)` semantics. Confidence in the hard-link-failure trigger is therefore MEDIUM, not HIGH.

- **Remediation**: make the fallback non-clobbering.
  1. **Preferred (Linux):** `renameat2(..., RENAME_NOREPLACE)`, which fails with `EEXIST` instead of replacing. macOS has no direct equivalent; emulate with `link`-then-`unlink` or `O_EXCL` sentinel.
  2. **Portable:** pre-claim the final name with an exclusive-create sentinel before the copy, and treat an existing sentinel as a conflict:
     ```rust
     // claim the name atomically before copying
     match OpenOptions::new().write(true).create_new(true).open(target) {
         Ok(f) => { drop(f); /* we own the name; rename will not hit a stranger */ }
         Err(e) if e.kind() == AlreadyExists => return Err(PublishError::Exists),
         Err(e) => return Err(PublishError::Io(e.into())),
     }
     ```
     then `rename` over our own placeholder.
  3. Always re-verify with `resolve_conflict` immediately before publishing, and on the fallback path compare the target's identity (device+inode, or size+mtime) against the expected pre-copy state; if it changed, fail with `PublishError::Exists` rather than replacing.
- **Verification steps**:
  1. Reproduce the probe on a FAT/exFAT loopback image mounted in a VM (e.g. `mkfs.exfat` on a loop device): confirm `std::fs::hard_link` returns an error whose `kind()` is not `AlreadyExists`.
  2. Add a regression test that creates the target *during* the copy (fake source with a callback) and asserts the pre-existing file's bytes are unchanged and the item is reported as a conflict.
  3. Confirm the reported outcome is `Skipped`/conflict, never `Completed`.
- **Confidence**: HIGH on the clobbering mechanism (measured); MEDIUM on the hard-link-failure trigger on removable media (documented, not measured here).

---

#### 5.2 [MEDIUM] Webview CSP disabled while privileged file-write IPC is exposed

- **ID**: LIT-2026-002
- **CWE**: [CWE-693](https://cwe.mitre.org/data/definitions/693.html) — Protection Mechanism Failure
- **OWASP Ref**: A05:2021 Security Misconfiguration; ASVS V14.4 (HTTP Security Headers) applied to the webview
- **Location**: `src-tauri/tauri.conf.json` → `app.security.csp`
- **Impact**: The webview runs with **no Content-Security-Policy**. Tauri exposes the entire IPC command surface to that webview, and `start_transfer` accepts a **client-supplied `destination` path** (`src-tauri/src/lib.rs:333-345`) that the backend then writes arbitrary files into. With no CSP, any future client-side injection (dependency compromise, a stray `innerHTML`, a new template path) escalates directly from script execution to arbitrary local file write, with no second line of defence.
- **Evidence**:

  ```json
  // src-tauri/tauri.conf.json
  "app": { ..., "security": { "csp": null } }
  ```

  Confirmed by direct config parse:
  ```
  $ python3 -c "import json;d=json.load(open('src-tauri/tauri.conf.json'));print(d['app']['security'])"
  {'csp': null}
  ```

  The capability file is already least-privilege, which limits the blast radius of a webview compromise:

  ```json
  // src-tauri/capabilities/default.json
  { "windows": ["main"], "permissions": ["core:default"] }
  ```

  Reviewed and **no injectable sink currently exists**: a grep for `dangerouslySetInnerHTML|innerHTML|outerHTML|eval(|new Function|document.write|insertAdjacentHTML|fetch(|XMLHttpRequest|WebSocket|sendBeacon|import(` across `src/` returned **no matches**. The frontend is static, bundled, and contains no remote content, so this is a **defence-in-depth gap with no known exploit path today** — hence Medium, not High.

- **Remediation**: set a restrictive CSP in `tauri.conf.json`. The app needs no remote origins, so a strict policy is safe:
  ```json
  "security": {
    "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: asset: http://asset.localhost; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'; frame-ancestors 'none'"
  }
  ```
  (`img-src ... data:` is required because previews are served as `data:` URLs — see §5.3. `style-src 'unsafe-inline'` may be avoidable if the stylesheet has no inline style attributes; verify before relaxing it.)
  Additionally, reject non-absolute / unexpected destination paths server-side in `start_transfer` rather than trusting the webview value.
- **Verification steps**: 1) rebuild and confirm the app still renders previews and transfers; 2) confirm DevTools reports zero CSP violations on load and during a transfer; 3) deliberately inject `console.log` + an inline `<script>` in a scratch build and confirm the CSP logs a violation and blocks it.
- **Confidence**: HIGH that CSP is absent; HIGH that no XSS sink currently exists (grep evidence). Exploitability today: none demonstrated.

---

### Low

#### 5.3 [LOW] Insecure temporary file for preview download

- **ID**: LIT-2026-003
- **CWE**: [CWE-377](https://cwe.mitre.org/data/definitions/377.html) — Insecure Temporary File
- **OWASP Ref**: A04:2021 Insecure Design
- **Location**: `src-tauri/src/provider_libimobile.rs:79-90` (`temp_preview_path`), consumed at `provider_libimobile.rs:844` and `:899-900`
- **Impact**: The predicted path is written by a *child process*, and the file is not pre-created exclusively, so on a multi-user machine a local attacker who wins the race can place a symlink at the predicted path and have `afcclient` write JPEG bytes **through** it — destroying any file the victim can write (for example a dotfile in the victim's home directory). Requires local shell access as another user and precise timing.
- **Evidence**:

  ```rust
  // provider_libimobile.rs:79-90 — predictable name, no O_EXCL, not pre-created
  fn temp_preview_path() -> Result<PathBuf, AppError> {
      let nanos = SystemTime::now().duration_since(UNIX_EPOCH)...;
      let mut p = std::env::temp_dir();
      p.push(format!("lit-prev-{}-{nanos}.img", std::process::id()));
      Ok(p)
  }

  // provider_libimobile.rs:899-900 — child writes to that path
  let mut child = Command::new(&afc)
      .args(["-u", udid, "get", source_path, dest_str])
  ```

  **Contrast — the same file handles this correctly elsewhere**: `temp_log_path()` results are opened with `.create_new(true)` before use (`run_logged`, `provider_libimobile.rs:106-112`), which defeats pre-creation and symlink attacks. The preview path lacks that protection.

  Confidence is MEDIUM rather than HIGH because the `nanos` component makes exact prediction hard, and `afcclient`'s behaviour when the destination already exists (refuse vs. truncate-through) was not verified — no physical device or `afcclient` was available.
- **Remediation**: pre-create the destination with `create_new(true)` (and on Unix, `O_NOFOLLOW`) immediately before spawning the child, or create a private `tempfile::TempDir` per call and write inside it. Pass the already-created path to `afcclient`.
- **Verification steps**: 1) unit test that a pre-existing file at the chosen preview path is never written through; 2) on Linux, `strace -f -e trace=openat` a preview fetch and confirm the destination is opened with `O_CREAT|O_EXCL|O_NOFOLLOW`.
- **Confidence**: MEDIUM.

---

#### 5.4 [LOW] Dependency graph not reproducible — both lockfiles are gitignored

- **ID**: LIT-2026-004
- **CWE**: [CWE-1104](https://cwe.mitre.org/data/definitions/1104.html) — Use of Unmaintained Third Party Components (pinning/SCVS concern)
- **OpenCRE**: [CRE-053-751](https://www.opencre.org/cre/053-751) — force build pipeline to check outdated/insecure components
- **OWASP Ref**: A06:2021 Vulnerable and Outdated Components; OWASP SCVS
- **Location**: `.gitignore` (entries `package-lock.json`, `Cargo.lock`)
- **Impact**: The published repository pins **no** dependency versions. Anyone auditing or rebuilding the project gets today's (or a future) dependency resolution, not the graph audited here; the SCA results in §5.6/§5.7 apply to the **local** lockfiles and cannot be reproduced from a fresh clone. It also blocks any future CI dependency-scanning gate.
- **Evidence**:

  ```
  $ git ls-files --error-unmatch package-lock.json      -> UNTRACKED (gitignored)
  $ git ls-files --error-unmatch src-tauri/Cargo.lock   -> UNTRACKED (gitignored)

  $ git ls-files | grep -E 'Cargo.lock|package-lock.json'
  (no output)
  ```

  Note the inverted case one directory down: `.gitignore` contains a stray, ineffective entry `/.src-tauri/target/` (leading slash makes it an absolute repo-root path that never matches), while the real ignore comes from `target/`. Cosmetic, but it indicates the ignore list has not been reviewed as a whole.

- **Remediation**: track both lockfiles (`git add -f package-lock.json src-tauri/Cargo.lock`, then remove the ignore entries). For a binary application — as opposed to a library — committing lockfiles is the recommended default and is what makes reproducible builds and dependency gates possible.
- **Verification steps**: 1) fresh clone into a new directory; 2) confirm both lockfiles are present; 3) `osv-scanner scan source -L package-lock.json -L src-tauri/Cargo.lock` reproduces the same 137/422 package counts and the same two advisory records.
- **Confidence**: HIGH.

---

#### 5.5 [LOW] `glib` 0.18.5 unsoundness

- **ID**: LIT-2026-005
- **CWE**: [CWE-937](https://cwe.mitre.org/data/definitions/937.html) — Using Components with Known Vulnerabilities
- **OpenCRE**: [CRE-053-751](https://www.opencre.org/cre/053-751)
- **OWASP Ref**: A06:2021 Vulnerable and Outdated Components
- **Package / version**: `glib` 0.18.5 (transitive) · **Advisory**: RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g · **CVSS**: 6.9 (CVSS 4.0 `AV:N/AC:L/AT:N/PR:N/UI:N/VC:N/VI:L/VA:N/...`) · **Fixed in**: `0.20.0`
- **Location**: `src-tauri/Cargo.lock`
- **Impact**: Unsoundness in the `Iterator` and `DoubleEndedIterator` implementations for `glib::VariantStrIter` (as published upstream).
- **Evidence** (reachability, manual — call-analysis deliberately not run):
  - Application code contains **no direct reference** to `glib` (grep across `src-tauri/src/` and `src/` returned no matches).
  - Reverse-walk of `Cargo.lock` shows `glib` is required only by the Linux GTK/WebKit desktop stack:
    ```
    glib <- required by: ['atk', 'cairo-rs', 'gdk', 'gdk-pixbuf', 'gdkx11', 'gio',
                          'gtk', 'javascriptcore-rs', 'libappindicator', 'pango',
                          'soup3', 'webkit2gtk']
    ```
    These are pulled in by Tauri's Linux window/webview backend and are absent on macOS (WKWebView) and Windows (WebView2).
  - `VariantStrIter` is a GVariant string-iteration helper; the app performs no GVariant string iteration. **No call path from application code to the affected API was identified.**
  - Therefore the CVSS 6.9 score is **not adjusted for reachability in practice**; I rate this **LOW** rather than Medium because no reachable path exists in this codebase, and label that rating as reasoning-based (see Limitations).
- **Remediation**: upgrade the `glib`/`gtk-rs` family to `0.20.0`+ by bumping the Tauri/GTK stack, or wait for Tauri to advance it. Track with `cargo audit`/Dependabot. Do not force-pin `glib` independently — it is version-locked to the rest of `gtk-rs`.
- **Verification steps**: 1) `cargo update -p glib` (or a Tauri bump) and re-run `cargo audit` — expect the advisory to clear; 2) re-run `cargo test` (59 tests) and `npm run build`; 3) smoke-test the Linux window on a physical machine to confirm the GTK bump did not break rendering.
- **Confidence**: HIGH that the advisory is present; MEDIUM that it is unexploitable (reasoned, not proven by call-graph tooling).

---

### Informational

#### 5.6 [INFORMATIONAL] `proc-macro-error` 1.0.4 is unmaintained

- **ID**: LIT-2026-006
- **CWE**: [CWE-1104](https://cwe.mitre.org/data/definitions/1104.html) · **OpenCRE**: [CRE-053-751](https://www.opencre.org/cre/053-751) · **OWASP Ref**: A06:2021
- **Package / version**: `proc-macro-error` 1.0.4 · **Advisory**: RUSTSEC-2024-0370 (no patched version — unmaintained)
- **Location**: `src-tauri/Cargo.lock`
- **Impact**: Build-time proc-macro helper with no maintainer. Not shipped in the binary's runtime path, so no direct runtime impact; risk is long-term (unpatched future issues, unmaintained transitive chain).
- **Evidence**: reverse-walk of `Cargo.lock` shows it is required only by `glib-macros` and `gtk3-macros` — the same Linux GTK stack. No application code references it.
- **Remediation**: no action available at this layer; resolve by advancing the Tauri/GTK stack when it migrates to `proc-macro-error2` or drops the dependency. Record an accepted-risk note if it persists.
- **Verification steps**: re-run `cargo audit --json` after the next Tauri bump; expect the `warnings.unmaintained` entry to disappear.
- **Confidence**: HIGH (tool-reported, reverse-walked).

---

#### 5.7 [INFORMATIONAL] Panic on poisoned mutex disables the whole command surface

- **ID**: LIT-2026-007
- **CWE**: [CWE-248](https://cwe.mitre.org/data/definitions/248.html) — Uncaught Exception · **OWASP Ref**: A04:2021 Insecure Design (availability)
- **Location**: `src-tauri/src/lib.rs` — 25 `unwrap`/`expect` sites in non-test code, all of the form `.lock().expect("... lock")`; plus `src-tauri/src/lib.rs:549` `.expect("error while running Local iPhone Transfer")` in `run()`
- **Impact**: If any thread panics while holding `mode`, `last_snapshot`, `media_cache`, `preview`, or `transfer_cancel`, the mutex is poisoned and **every subsequent command panics** on `.expect(...)`. A single unexpected panic therefore converts into a permanently unusable app (requires restart), and the transfer-cancel flag can never be cleared. No confidentiality or integrity impact.
- **Evidence**:
  ```
  $ for f in src-tauri/src/*.rs; do ... count unwrap/expect before #[cfg(test)] ...; done
    device.rs: 0    environment.rs: 0   errors.rs: 0        lib.rs: 25
    main.rs: 0      media.rs: 0         naming.rs: 0         platform.rs: 0
    preview.rs: 0   progress.rs: 0      provider_libimobile.rs: 0
    provider_mock.rs: 0                 transfer.rs: 0
  ```
  All 25 are in `lib.rs` and all are lock acquisitions. Notably, every performance-critical module is panic-free.
- **Remediation**: use `parking_lot::Mutex` (no poisoning) or recover with `unwrap_or_else(|e| e.into_inner())`; additionally wrap command bodies so an internal panic returns `ErrorCode::Internal` instead of tearing down the handler thread. Treat poisoned state as "reinitialise" rather than "abort".
- **Verification steps**: add a test that poisons each mutex and asserts the command surface still responds with an `Internal` error rather than panicking; confirm cancel-transfers still works after a simulated panic mid-transfer.
- **Confidence**: HIGH (source-level; the poisoning behaviour is standard `std::sync::Mutex` semantics).

---

#### 5.8 [INFORMATIONAL] Tool discovery prefers a directory beside the executable; no executable-bit check

- **ID**: LIT-2026-008
- **CWE**: [CWE-426](https://cwe.mitre.org/data/definitions/426.html) — Untrusted Search Path · **OWASP Ref**: A08:2021 Software and Data Integrity Failures
- **Location**: `src-tauri/src/platform.rs:66-96` (`candidate_dirs` at `:66`, `find_tool` at `:85`)
- **Impact**: Search order is `<exe dir>/tools` → `<exe dir>` → `$PATH` → macOS Homebrew prefixes. In user-writable install layouts (Windows zip, extracted AppImage, a copied `target/release` binary), a planted `tools/afcclient` or `tools/idevice_id` is executed in preference to the system tool. `find_tool` also only checks `.is_file()` and never verifies the executable bit, so a non-executable file causes a spawn failure instead of falling through to `PATH`. Requires the attacker to already write into the install directory as the same user, so real-world impact is low.
- **Evidence**:
  ```rust
  // platform.rs:66-82
  fn candidate_dirs() -> Vec<PathBuf> {
      let mut dirs = Vec::new();
      if let Ok(exe) = std::env::current_exe() {
          if let Some(dir) = exe.parent() {
              dirs.push(dir.join("tools"));   // checked before PATH
              dirs.push(dir.to_path_buf());
          }
      }
      ...
  // platform.rs:85-96
      for dir in candidate_dirs() {
          let candidate = dir.join(format!("{name}{exe_ext}"));
          if candidate.is_file() { return Some(candidate); }   // no exec-bit check
      }
  ```
  **Positive counterpoint**: the same module never interpolates user input into a command — tool names are compile-time constants (`TOOL_ID`, `TOOL_INFO`, `TOOL_PAIR`, `TOOL_AFC`) and paths are joined, so this is a search-path concern, not injection.
- **Remediation**: search `$PATH` before the executable directory, or only consult the bundled `tools/` directory when it is not user-writable; verify the executable bit (Unix `mode & 0o111`, or a `Command::new(...).arg("--version")` probe) and continue searching on failure.
- **Verification steps**: place a non-executable `idevice_id` beside the binary and confirm discovery falls through to `PATH`; place an executable impostor in `tools/` and confirm it is not preferred over `$PATH`.
- **Confidence**: HIGH for the code behaviour; impact assessment conservative.

---

#### 5.9 [INFORMATIONAL] `.gitignore` lacks common secret-file patterns

- **ID**: LIT-2026-009
- **CWE**: [CWE-538](https://cwe.mitre.org/data/definitions/538.html) — Insertion of Sensitive Information into Externally-Accessible File or Directory
- **OWASP Ref**: A05:2021 Security Misconfiguration (preventive control gap)
- **Location**: `.gitignore:25-27`
- **Impact**: Only `.env`, `.env.local`, `.env.*.local` are ignored. There are no patterns for `*.key`, `*.pem`, `*.p12`, `credentials.json`, `service-account*.json`, or `*.kubeconfig`. If any such file is ever introduced, it would be committed by default. **No such file exists today** and the secrets scan found nothing (§5.10) — this is a preventive-control gap only.
- **Evidence**: `grep -nE '\.env|\.key|\.pem|secret|credential|tfstate' .gitignore` returns only the three `.env` lines.
- **Remediation**: add the patterns above, and add a pre-commit secret scan (`gitleaks`/`trufflehog`) plus a CI job once a workflow exists.
- **Verification steps**: `git check-ignore -v secrets.pem credentials.json` returns a match after the change.
- **Confidence**: HIGH.

---

#### 5.10 Secrets scan — result: none found

Per `secrets-scan.md` §1, no scanner (`trufflehog`, `gitleaks`, `detect-secrets`) is installed, so I proceeded with the play's manual pattern analysis and extended it to full git history.

| Check | Result |
| --- | --- |
| Cloud/API key patterns (`AKIA…`, `sk-…`, `sk-ant-…`, `ghp_…`/`gho_…`, `xox[bps]-…`, `SG.…`, `sk_live_`/`pk_live_`) | **No matches** |
| Private key headers (`-----BEGIN … PRIVATE KEY-----`) | **No matches** |
| Credential assignments (`password`/`secret`/`api_key`/`token`/`credential` = "…") | **No matches** |
| Connection strings with embedded credentials (`scheme://user:pass@host`) | **No matches** |
| Git history (all 3 commits, all blobs, `git log --all -p`) | **No matches** |
| Tracked env/credential files (`git ls-files` filtered) | **None** — only `AGENTS.md` and `skills-lock.json` match the filter |

**Secret values redacted:** there are none to redact. No rotation is required.

**Preventive controls:**

| Control | Status |
| --- | --- |
| `.gitignore` covers `.env` variants | ✅ Present |
| `.gitignore` covers keys/certs/credentials | ⚠️ Missing (LIT-2026-009) |
| Pre-commit secret scanning hook | ❌ Not configured |
| CI secret scanning | ❌ Not configured (no CI) |
| Documented secret-management guidance | ❌ Not present (and no secrets are needed by design) |

---

#### 5.11 Dependency audit — full results

Two independent scanners, same conclusion.

**Ecosystem coverage**

| Ecosystem | Manifest | Packages |
| --- | --- | --- |
| npm (frontend) | `package-lock.json` | 137 scanned (138 entries incl. root) |
| Rust (backend) | `src-tauri/Cargo.lock` | 422 scanned |

**Tool agreement**

| Tool | npm result | Rust result |
| --- | --- | --- |
| `osv-scanner` 2.7.0 | no advisories returned | 3 records (glib ×2 aliases, proc-macro-error ×1) |
| `npm audit` | 0 total (0 critical/high/moderate/low/info) | n/a |
| `cargo-audit` 0.22.2 | n/a | `vulnerabilities: found=false, count=0`; 2 `warnings` (unsound `glib`, unmaintained `proc-macro-error`) |

Rust advisory detail:

| Package | Version | Advisory | Kind | Severity | Fixed in | Reachable? |
| --- | --- | --- | --- | --- | --- | --- |
| `glib` | 0.18.5 | RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g | unsound | MODERATE, CVSS 6.9 | 0.20.0 | **No** — GTK stack only, no app callsite |
| `proc-macro-error` | 1.0.4 | RUSTSEC-2024-0370 | unmaintained | — | none | **No** — build-time macro only |

**Reachability:** `--call-analysis` was **not** run (it executes build scripts), so reachability is established by reverse-dependency walking plus a negative grep for direct use. Both advisories sit exclusively in Tauri's Linux desktop backend; macOS and Windows builds do not compile them.

**Dependency health:** no typosquat-like package names were observed in either tree; the frontend dependency list is small (React, ReactDOM, Tauri API, Lucide React) and all are long-established projects. No packages with declared missing licenses were flagged in this pass — license texts were audited separately in `THIRD_PARTY_NOTICES.md`.

## 6. Agent Security Audit (Scoped)

`agent-security-audit.md` asks for a permission inventory, injection-surface map, excessive-agency check, and exfiltration-path analysis. Applied honestly to *this* repository:

| Category | Finding | Justified? |
| --- | --- | --- |
| Product agent runtime | **None** — the shipped app contains no LLM, agent loop, or tool-calling | N/A |
| MCP servers | **None** — no `.mcp.json`, no MCP dependency, no server source | N/A |
| Dev-tooling instructions | `AGENTS.md` (99 lines) — build brief and acceptance criteria; contains no credentials, no network calls, and instructs the agent *not* to run `sudo` or shell out with user-controlled strings | Yes |
| Third-party skill content | `apple-design` from `dickwu/apple-design-skill`, hash-pinned in `skills-lock.json`, 133 documentation files, **no executable content** | Yes, with the caveat below |

Caveat worth recording: `.agents/` is gitignored while `skills-lock.json` is tracked, so the recorded hash cannot be verified from a fresh clone without re-fetching the skill from GitHub. Effectively inert today, but a supply-chain pin is only as good as the ability to re-verify it.

**Excessive agency / exfiltration:** not applicable — no agent executes tools on the user's behalf at runtime. `prompt-injection-test`, `llm-risk-assess`, `agentic-ai-risk-assess`, and `multi-agentic-threat-model` were therefore **not run**; running them against a product with no LLM would have produced fabricated findings.

## 7. Positive Observations (controls verified present)

These are material strengths and should not be lost in later refactors:

1. **Device-path confinement is enforced in two layers.** `validate_source_path` (`media.rs`) requires a `/DCIM/` prefix and rejects `..`, NUL, and control characters, *and* `start_transfer` resolves items by id against a server-side `media_cache` (`lib.rs:366-379`) so the frontend cannot inject an arbitrary source path at all. Unit-tested (`media.rs` tests).
2. **No shell is ever used.** All four external tools are invoked via `Command::new(path).args([...])` with fixed argument vectors — never a shell string. `POSIXLY_CORRECT=1` is set deliberately to stop getopt permuting options. `AGENTS.md`'s "do not shell out with user-controlled strings" rule is genuinely upheld.
3. **Filenames are treated as hostile.** `sanitize_filename` (`naming.rs`) replaces separators/illegal characters and control characters, strips `..` in a loop, trims leading/trailing dots-spaces-underscores, mirrors Windows reserved device names, and bounds length to 200 bytes while preserving the extension. Well covered by tests, including `../../etc/passwd` and `"..."`.
4. **Temp log files use exclusive create.** `temp_log_path()` is opened with `create_new(true)`, defeating pre-creation and symlink races in `/tmp` — the correct pattern (contrast LIT-2026-003).
5. **Preview images are validated by magic bytes, not extension.** `sniff_image_mime` requires JPEG/PNG/GIF/WEBP signatures, sizes are capped at 8 MiB, the cache is byte-bounded (16 MiB, FIFO eviction), preview directories are derived only from validated `/DCIM/` paths, and `is_preview_file_name` rejects hidden names, traversal, and control characters. There is no path from device-controlled data to SVG/HTML execution, and `base64_encode` has RFC 4648 test vectors.
6. **Transfer integrity is carefully handled.** Content is streamed to `.<name>.part-<pid>` with a bounded 1 MiB buffer, `sync_all`-ed, verified against the expected byte count, and only then published. A size mismatch is never reported as complete (`transfer.rs` test `size_mismatch_is_never_reported_complete`); partial files are cleaned up; failures never surface as `Completed`.
7. **Destination is validated before work starts.** `check_destination` verifies existence, directory-ness, and real writability via a `create_new` probe, and the transfer thread re-checks so a race cannot slip a bad folder through.
8. **Mock/demo mode cannot leak into production.** Every mock path is `#[cfg(debug_assertions)]`, and `set_mock_mode`/`set_mock_device_state` explicitly `#[cfg(not(debug_assertions))]`-refuse. Release builds carry no demo code.
9. **Least-privilege webview capability.** `capabilities/default.json` grants only `core:default` — no `fs`, no `shell`, no `http`. The dialog plugin is driven from Rust, so the webview never holds filesystem or process permissions.
10. **The offline-only claim holds.** `Cargo.toml` declares no HTTP/TLS client crate; the frontend contains no `fetch`/`XMLHttpRequest`/`WebSocket`/`sendBeacon`. The only socket in the codebase is `UnixStream::connect("/run/usbmuxd")` (Unix) or `TcpStream::connect_timeout("127.0.0.1:27015")` (Windows) — a local USB-multiplexer liveness probe, not an outbound network request.
11. **Error handling avoids leakage.** `errors.rs` maps every failure to a stable `ErrorCode` plus a non-technical message; `io::Error` conversion emits generic text (`"A file operation failed unexpectedly (…)"`) with no embedded path. The only filesystem paths surfaced to the UI are tool locations in the intentional "Details for support" diagnostics panel (`platform.rs:102-105`).
12. **Agent-tooling surface is inert.** The only AI-agent artefacts are `AGENTS.md` (a development brief) and the `apple-design` skill pinned by hash in `skills-lock.json`. `find` shows **no** executable content (`.sh`/`.js`/`.py`/`.ts`) among the skill's 133 files — it is documentation only. There are no MCP servers, no tool definitions, and no LLM runtime in the product.

## 8. Areas Not Assessed and Testing Limitations

Nothing in this section was verified. Absence of a finding here is **not** evidence of absence.

**Not tested — no live target or device**

1. **No physical iPhone was available.** USB enumeration, pairing/Trust handling, AFC listing, and `afcclient` get semantics are reviewed as code only. Reported behaviour (e.g. whether `afcclient` truncates through an existing destination file) could not be confirmed.
2. **No DAST, fuzzing, or active testing** was performed. This was a static assessment by design (and no web/network surface exists to test dynamically).
3. **No runtime memory-safety testing.** Rust's guarantees were taken as given; `unsafe` blocks were not exhaustively enumerated.
4. **`cargo clippy` and `cargo fmt --check` were not re-run** in this pass — this was a security review, not a quality gate. `cargo test` (59 tests) had passed in the prior session and is cited as prior evidence, not re-verified here.

**Not measurable on this host**

5. **The FAT/exFAT trigger in LIT-2026-001 is reasoned, not measured.** No `vfat`/`exfat`/`ntfs` filesystem was mounted (`/proc/mounts` check was empty) and mounting a loopback image needs elevated privileges, which I did not request. The `rename`-clobbers-result *is* measured.
6. **Reachability is manual.** `osv-scanner --call-analysis=rust` was declined because it executes build scripts. Both advisories were reasoned unreachable via reverse-dependency walking + negative grep — strong, but not equivalent to a call-graph proof.
7. **`afcclient`'s destination-overwrite behaviour is unverified**, which caps LIT-2026-003 at MEDIUM confidence.

**Not applicable**

8. **IaC review** — no Terraform, Kubernetes, Helm, CloudFormation, Dockerfile, or docker-compose files exist.
9. **CI/CD pipeline review** — there is no `.github/workflows/` directory. Consequently there is also no automated dependency gate, no secrets gate, and no build provenance.
10. **Authn/authz/session/crypto review** — the application has no users, no accounts, no sessions, no authentication, no authorization model, no server, and no cryptography of its own (it relies solely on iOS's own pairing/Trust flow, which it does not bypass or attempt to bypass). OWASP A01/A02/A03/A07/A09/A10 and most of the API Top 10 are therefore genuinely not applicable rather than skipped. Manual inspection found no custom crypto, no hardcoded keys, and no `MD5`/`SHA1`/`DES`/`RC4`/ECB use (no crypto primitives are used at all).
11. **XSS/CSRF review** — no `dangerouslySetInnerHTML`, `innerHTML`, `eval`, `new Function`, or `document.write` anywhere in `src/`; no cookies, no CSRF-capable state-changing web endpoints. The only rendering of device-derived data is through React's auto-escaped text nodes and `img src` with a validated `data:` URL.
12. **Mobile MASVS review** — this is not a native Android/iOS application.
13. **Secrets scanning tools** — `trufflehog`/`gitleaks`/`detect-secrets` unavailable; pattern analysis is weaker than entropy+verification-based scanning, though it was extended across all git history.
14. **macOS and Windows code paths** — reviewed statically but never built or executed. Windows-specific behaviour (`MoveFileEx` clobber semantics, loopback usbmuxd probe) is documented reasoning only.
15. **`dist/` and `src-tauri/target/` build outputs were excluded** from scanning. The generated Tauri schema files under `src-tauri/gen/` were not audited.
16. **Third-party licence/legal review** is out of scope (covered separately in `THIRD_PARTY_NOTICES.md`).

## 9. Playbook Installation Verification

Recorded because the task required it, and because "cloned" is not "installed".

**Repository cloned:** `/home/chandanjainhp/coding/owasp-secure-agent-playbook` (sibling of the project, so the target repo was not polluted), v0.2.9 at the time of cloning.

**Installation method:** the repository's documented Claude Code marketplace procedure (README §Quick Start), using the `claude` CLI (v2.1.289) rather than interactive slash commands:

```
claude plugin marketplace add /home/chandanjainhp/coding/owasp-secure-agent-playbook --json
  -> {"outcome":"ok","marketplace":"agent-security-playbook","message":"Successfully added marketplace ... (declared in user settings)"}

claude plugin install code-security-skills@agent-security-playbook --json
  -> {"outcome":"ok","scope":"user","message":"Successfully installed plugin: code-security-skills@agent-security-playbook"}

claude plugin install ai-security-skills@agent-security-playbook --json
  -> {"outcome":"ok","scope":"user","message":"Successfully installed plugin: ai-security-skills@agent-security-playbook"}
```

**Verified installed and enabled** (`claude plugin list`):

| Plugin | Version | Status | Loaded from |
| --- | --- | --- | --- |
| `ai-security-skills@agent-security-playbook` | 0.2.9 | ✔ enabled (user scope) | `.../plugins/ai-security-skills/` |
| `code-security-skills@agent-security-playbook` | 0.2.9 | ✔ enabled (user scope) | `.../plugins/code-security-skills/` |

**Component inventory confirmed** (`claude plugin details`): 11 skills + 5 agents + 0 MCP servers in `code-security-skills`; 7 skills + 1 agent + 0 MCP servers in `ai-security-skills`. No hooks and no LSP servers are installed by either plugin — noted because hooks would be an executable trust-boundary addition.

**Note on scope:** installation declared the marketplace and both plugins at **user scope**, i.e. outside the project directory. Nothing was written into the project repository. To remove them: `claude plugin uninstall <plugin>` and `claude plugin marketplace remove agent-security-playbook`.

## 10. Standards Coverage

| CRE ID | Requirement | CWE | OWASP Ref | Findings |
| --- | --- | --- | --- | --- |
| [CRE-380-540](https://www.opencre.org/cre/380-540) | Ensure business flows' thread safety / resistance to race conditions | CWE-367 | A04:2021 | 1 |
| [CRE-134-412](https://www.opencre.org/cre/134-412) | Protect sensitive functionality against race conditions | CWE-367 | A04:2021 | 1 |
| [CRE-053-751](https://www.opencre.org/cre/053-751) | Force build pipeline to check outdated/insecure components | CWE-1104, CWE-937 | A06:2021 | 4, 5, 6 |

Cross-references resolved live against the OpenCRE API (`/rest/v1/standard/CWE/sectionid/{n}`). CWEs 377, 693, 426, 248, 538 and 248 returned no CRE mapping from that endpoint and are cited directly to MITRE.

## 11. Prioritized Remediation Plan

Ordered by risk reduction per unit of effort.

| Priority | Action | Addresses | Effort | Effect |
| --- | --- | --- | --- | --- |
| **1** | Make the publish fallback non-clobbering: claim the final name with an exclusive-create sentinel (or `renameat2` + `RENAME_NOREPLACE` on Linux), and add a regression test that races the target during a copy. | LIT-2026-001 | Low | Restores the app's headline data-safety guarantee on FAT/exFAT — the most likely destination for this app |
| **2** | Set an explicit restrictive CSP in `tauri.conf.json`; verify previews still render via `img-src … data:`. | LIT-2026-002 | Low | Restores webview defence-in-depth behind the privileged `start_transfer` IPC |
| **3** | Commit both lockfiles (`git add -f`) and drop their `.gitignore` entries. | LIT-2026-004 | Low | Makes the audited graph reproducible and unblocks any future dependency gate |
| **4** | Pre-create the preview destination with `create_new` (+ `O_NOFOLLOW` on Unix). | LIT-2026-003 | Low | Closes the `/tmp` symlink race |
| **5** | Add `.github/workflows/` with a dependency scan (`cargo audit` + `osv-scanner`/`npm audit`) and a secrets scan; add `*.key`/`*.pem`/`credentials*` ignore patterns and a pre-commit hook. | LIT-2026-004, -009, and the un-gated state of §8.9 | Medium | Converts one-off checks into permanent gates |
| **6** | Advance the Tauri/GTK stack to pull `glib ≥ 0.20.0`; re-run `cargo audit`. | LIT-2026-005, -006 | Medium | Clears both Rust advisories |
| **7** | Replace `Mutex` poisoning panics with recovery (or `parking_lot`); wrap commands so internal panics return `Internal`. | LIT-2026-007 | Medium | Prevents one panic from bricking the app |
| **8** | Prefer `$PATH` over the executable directory (or restrict the bundled `tools/` lookup) and verify the executable bit. | LIT-2026-008 | Low | Hardens installs in user-writable locations |
| **9** | Validate the `destination` argument server-side (absolute path, existing directory) instead of trusting the webview. | LIT-2026-002 (defence in depth) | Low | Hardens the privileged IPC boundary |
| **10** | Add a physical-device test pass on Linux, then macOS and Windows, and record `afcclient` destination-overwrite semantics. | §8.1, -5, -7, -14 | High | Closes the largest evidence gaps in this report |

### Suggested sequencing

1. **Before any wider release:** actions 1, 2, 3 (all low-effort, and action 1 restores a stated product guarantee).
2. **Next maintenance cycle:** 4, 5, 6, 7, 8, 9.
3. **Before claiming cross-platform readiness:** 10, plus the per-OS validation already required by `AGENTS.md`.

---

## 12. Report Integrity Statement

- Every finding above is tied to a file and line, a code excerpt, or recorded tool output. No speculative issue is presented as confirmed.
- Findings whose exploitability depends on a condition I could not exercise are explicitly labelled as such (LIT-2026-001 trigger, LIT-2026-003 window, LIT-2026-005 reachability) and carry reduced confidence.
- No finding was fabricated for an area that was not assessed; §8 lists those areas instead.
- **This report does not establish that the project is secure.** It establishes that, within a static, host-limited scope, two Medium and three Low issues exist and a set of specific controls are correctly implemented. Runtime behaviour against real hardware remains unverified.

*Generated with the OWASP Secure Agent Playbook (v0.2.9) — plays `code-review-security`, `sca-audit`, `secrets-scan`, `owasp-top10-web-review`, `agent-security-audit`.*

---

## 13. Remediation Record (addendum, 2026-10-14)

All nine findings were addressed. **Seven are fixed and re-verified; two are accepted with a measurement rather than a guess**, because no fix exists upstream. Section numbers below match §5.

### 13.1 LIT-2026-001 — silent overwrite on hard-link-less filesystems — FIXED

`publish()` in [transfer.rs](src-tauri/src/transfer.rs) no longer falls back to a bare `rename()`. It is now two explicit stages:

- `publish_via_hard_link()` — the fast path. A hard link can never replace an existing file. It returns `Ok(false)` **only** when the filesystem has no hard-link support, so the caller falls back.
- `publish_via_claim()` — claims the final name with `OpenOptions::new().write(true).create_new(true)`, which fails with `AlreadyExists` instead of replacing, and only then renames the verified temp file over the placeholder it now owns. Any other error (read-only volume, permissions, quota) is reported as a real error; the previous code reinterpreted a generic failure as "target exists", and the placeholder is removed on failure so no empty file is left behind.

The decisive evidence is a paired probe rather than an assertion:

| | Result | Pre-existing file |
| --- | --- | --- |
| Old fallback | `Ok(())` | **overwritten** |
| New fallback | `Err(PublishError::Exists)` | **bytes intact** |

Three regression tests added: `publish_fallback_never_replaces_an_existing_file`, `publish_fallback_publishes_when_the_name_is_free`, `publish_refuses_an_existing_target_end_to_end`.

### 13.2 LIT-2026-002 — webview CSP disabled — FIXED

[tauri.conf.json](src-tauri/tauri.conf.json) now sets a production policy, a separate looser `devCsp` (Vite injects inline styles in development), and `dangerousDisableAssetCspModification: false` so Tauri still applies its own script hashes:

```
default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:;
font-src 'self'; connect-src 'self' ipc: http://ipc.localhost; object-src 'none';
base-uri 'self'; frame-ancestors 'none'; form-action 'none'
```

`connect-src` keeps `ipc:` / `http://ipc.localhost` because Tauri's IPC needs them; `img-src … data:` keeps preview thumbnails working. Verified live rather than assumed — see §13.11.

### 13.3 LIT-2026-003 — insecure temporary file for preview download — FIXED

[provider_libimobile.rs](src-tauri/src/provider_libimobile.rs) now creates a **private per-download scratch directory** (`DirBuilder` with mode `0700` on Unix, fresh name including a nanosecond component) with `create` rather than `create_dir_all`, so a pre-existing path is a hard error rather than a silent reuse, and removes the whole directory afterwards.

This deliberately differs from the report's suggestion to `create_new` the file itself: the file is written by the `afcclient` **child process**, and whether that tool tolerates a pre-existing destination is unverified (§8.7). Securing the parent directory closes the same pre-creation/symlink race without depending on that unknown.

### 13.4 LIT-2026-004 — lockfiles gitignored — FIXED

`package-lock.json` and `src-tauri/Cargo.lock` are now tracked, and their `.gitignore` entries were removed with a comment explaining why an *application* must commit them. A dead `.src-tauri/target/` entry was also corrected to `src-tauri/target/`.

### 13.5 LIT-2026-005 — `glib` 0.18.5 unsoundness — ACCEPTED (measured, no upstream fix)

Attempting the fix rather than asserting it was impossible:

| Check | Result |
| --- | --- |
| `cargo update -p glib --dry-run` | *Locking 0 packages* — cannot move |
| Newest published `tauri` | **2.12.3** (already in use); requires `gtk ^0.18` |
| `gtk 0.18.2` requires | `glib ^0.18` |
| `tauri 3.0.0-alpha.4` requires | `gtk ^0.18` — the alpha does **not** fix it either |
| Advisory patched versions | `[">=0.20.0"]` (read from the local RustSec DB) |

**Reachability:** the unsound API is `glib::VariantStrIter::{next, nth, last, next_back, nth_back}`. `grep -rn 'glib\|Variant' src-tauri/src/` returns **no matches** — this application never calls it, and no Tauri API used here exposes it. `cargo audit` classifies it as a non-failing *warning* (`informational = "unsound"`) and exits 0.

**Decision:** accept and monitor. Re-evaluate when Tauri adopts the gtk-rs 0.20 stack. The new CI gate (§13.10) shows this warning on every run so it cannot be forgotten, and fails the build if a genuine vulnerability ever appears.

### 13.6 LIT-2026-006 — `proc-macro-error` unmaintained — ACCEPTED

The advisory lists `patched = []` — there is no fixed version to move to. It is a build-time proc-macro, absent from the shipped binary's runtime path. Same visibility and monitoring as §13.5.

### 13.7 LIT-2026-007 — panic on poisoned mutex — FIXED

[lib.rs](src-tauri/src/lib.rs) gained three helpers — `lock`, `read`, `write` — that recover with `unwrap_or_else(|e| e.into_inner())`, and **all 24** `.expect("… lock")` sites now use them. The state behind these locks is plain data (a mode flag, a device snapshot, caches, a cancel flag), so the last value remains usable. Previously a single panic while holding a lock made every later command panic; now the app degrades instead of bricking. A grep confirms zero `.expect("… lock")` remain.

### 13.8 LIT-2026-008 — tool discovery order and executability — FIXED

[platform.rs](src-tauri/src/platform.rs): `candidate_dirs()` now searches `$PATH` and the Homebrew prefixes **before** the executable's own directory and its `tools/` subfolder, and `find_tool()` requires a regular file with at least one execute bit on Unix. The bundled lookup is retained as a fallback so self-contained installs keep working. Three tests added, including one asserting the PATH entries precede the executable directory.

### 13.9 LIT-2026-009 — `.gitignore` secret patterns — FIXED

[.gitignore](.gitignore) now covers `*.key`, `*.pem`, `*.p12`, `*.pfx`, `*.jks`, `credentials.json`, `service-account*.json`, `*.kubeconfig`, `*.tfstate`, `*.tfvars`, `.npmrc`, `.pypirc`. These are preventive: §5.10 found no secret material in the repository.

### 13.9a Destination validation (remediation #9) — FIXED

`start_transfer` now rejects an empty or **relative** destination before any device work begins, so a relative path can never silently resolve against the process working directory. Existence, directory-ness and writability continue to be checked by `check_destination`, on the invoker thread and again on the transfer thread.

### 13.10 New CI gate (remediation #5)

[.github/workflows/security.yml](.github/workflows/security.yml) adds three jobs:

| Job | Gate |
| --- | --- |
| `dependency-advisories` | `cargo audit` (fails on real vulnerabilities; the two accepted warnings stay visible) and `npm audit --omit=dev --audit-level=high` |
| `secret-scan` | [scripts/scan-secrets.sh](scripts/scan-secrets.sh) |
| `verify` | `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `npm ci && npm run build` |

It also runs weekly, so an advisory published *after* the last commit still surfaces.

Hardening choices, and why:

- `permissions: contents: read` — a compromised step cannot write to the repository.
- Only **one** third-party action (`actions/checkout`), pinned to commit `11d5960a326750d5838078e36cf38b85af677262` instead of a mutable tag, so a re-pointed tag cannot change what runs. Rust and Node come from the runner image rather than extra actions.
- `cargo-audit` is built from source at the version verified locally (`0.22.2`) rather than fetched as a binary, so crates.io checksums apply.
- No cache step, avoiding a cache-poisoning surface — accepted cost is a slower job.

The scanner ([scripts/scan-secrets.sh](scripts/scan-secrets.sh)) is dependency-free and reports **only** the rule name and `file:line`; it never prints matched text, so the scanner cannot itself leak a credential into a CI log. `.githooks/pre-commit` reuses the same script (opt in with `git config core.hooksPath .githooks`) so the two cannot drift.

### 13.11 Verification performed after the changes

| Check | Command / method | Result |
| --- | --- | --- |
| Rust formatting | `cargo fmt --all --check` | **pass** |
| Rust lints | `cargo clippy --all-targets -- -D warnings` | **pass, 0 warnings** |
| Rust tests | `cargo test` | **65 passed, 0 failed** (up from 59) |
| Frontend build | `npm run build` (`tsc -b && vite build`) | **pass** |
| Rust advisories | `cargo audit` | **0 vulnerabilities**; 2 informational warnings (§13.5, §13.6) |
| npm advisories | `npm audit --omit=dev` | **0 vulnerabilities** (138 packages) |
| Secret scan | `scripts/scan-secrets.sh` | **clean** — 103 tracked files, 11 rules |
| Secret-scan detection | planted AWS key + URL credential in a throwaway repo | detected; reported rule + `file:line` only; matched value **absent from the output**; `scan-secrets:allow` honoured; no false positive on benign text |
| CSP enforcement | built app served with the exact production CSP, loaded in a real browser | inline script **blocked**; `securitypolicyviolation` fired with directive `script-src-elem` |
| CSP app compatibility | same page, driven through the sidebar and filters | **0 violations**; app rendered (`#root` populated); 198 CSS rules loaded; UI responded to clicks |
| Workflow validity | parsed with a YAML parser | valid; 3 jobs; `permissions` read-only |

Initial `cargo fmt --all --check` **failed** on the first attempt (my own edits had introduced two formatting deviations); `cargo fmt --all` was applied and all three Rust gates were re-run afterwards, not just the formatter.

**Limitations of this verification, stated plainly:**

1. The workflow has **not** been executed on GitHub Actions — no runner is available locally. Each command was run locally on Linux (the same OS as the jobs), but the apt package list and action resolution are unverified.
2. The CSP test used a plain Chromium tab, not the Tauri webview. It proves the policy is enforced and that the built frontend needs no inline script or style, but it does not exercise Tauri's own injected script hashes. One earlier attempt to test enforcement via `eval` was **inconclusive** (automation evaluates in an isolated world that page CSP does not govern) and was discarded in favour of the inline-injection test that is world-independent.
3. Screenshot capture of the preview webview was unavailable ("no frames / not composited"), so the evidence is DOM-, accessibility-tree- and violation-event based.
4. No physical iPhone was involved. §5.1's FAT/exFAT trigger remains reasoned rather than measured on real removable media, and §13.3's device-facing behaviour is unexercised.
5. The pre-existing `vite` dev server already listening on port 1420 was left untouched; the temporary CSP harness ran on 4599 and was stopped and removed afterwards.

### 13.12 Findings that remain open

Only the **testing gaps**, not code defects: §8.1, §8.5, §8.7 and §8.14 (physical-device validation on Linux/macOS/Windows, low-disk-space behaviour, `afcclient` destination-overwrite semantics, and the fact that neither the workflow nor the packaged artifacts have been exercised on CI). Nothing in this addendum closes those; they still require hardware.
