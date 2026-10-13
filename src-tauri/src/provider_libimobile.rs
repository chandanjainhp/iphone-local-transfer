//! Real USB adapter: drives the libimobiledevice toolchain
//! (`idevice_id`, `ideviceinfo`, `idevicepair`, `afcclient`) through argv-only
//! `std::process::Command` calls — never a shell — and classifies their output
//! into UI-safe states.
//!
//! Rationale for subprocesses instead of FFI: the lockdown pairing/TLS stack is
//! owned by libimobiledevice; shelling out (documented in AGENTS.md as an
//! allowed integration style) reuses the maintained, audited implementation,
//! keeps the build free of C dependencies, and lets each OS ship its own
//! validated tool build. Argument lists are static or internally generated, so
//! no user-controlled string ever reaches a shell.

use crate::device::{DeviceInfo, DeviceProvider, DeviceSnapshot, DeviceState, MediaSource};
use crate::errors::{AppError, ErrorCode};
use crate::media::{kind_for_name, validate_source_path, MediaItem};
use crate::platform::{self, TOOL_AFC, TOOL_ID, TOOL_INFO, TOOL_PAIR};
use crate::preview::{self, PreviewImage, MAX_PREVIEW_BYTES};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const INFO_TIMEOUT: Duration = Duration::from_secs(12);
const PAIR_VALIDATE_TIMEOUT: Duration = Duration::from_secs(12);
/// `idevicepair pair` waits while the Trust dialog is on screen.
const PAIR_TRIGGER_TIMEOUT: Duration = Duration::from_secs(25);
const PAIR_COOLDOWN: Duration = Duration::from_secs(60);
const AFC_LIST_TIMEOUT: Duration = Duration::from_secs(25);
/// Preview fetches read a thumbnail (typically 100–150 KB, keyframes ~1.5 MB).
const AFC_PREVIEW_TIMEOUT: Duration = Duration::from_secs(30);
const AFC_POLL: Duration = Duration::from_millis(150);
/// If a copy writes zero bytes this long, treat the link as gone.
const STALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Safety cap on DCIM subfolders walked during a scan.
const MAX_DCIM_DIRS: usize = 500;
const MAX_LOG_BYTES: usize = 128 * 1024;

pub struct LibimobileProvider;

// ---------------------------------------------------------------------------
// subprocess plumbing
// ---------------------------------------------------------------------------

struct CmdOutput {
    code: Option<i32>,
    text: String,
}

impl CmdOutput {
    fn success(&self) -> bool {
        self.code == Some(0)
    }
}

fn temp_log_path() -> Result<PathBuf, AppError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut p = std::env::temp_dir();
    p.push(format!("lit-{}-{nanos}.log", std::process::id()));
    Ok(p)
}

fn read_limited(path: &Path) -> String {
    let mut buf = Vec::new();
    if let Ok(f) = File::open(path) {
        let _ = f.take(MAX_LOG_BYTES as u64).read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Unique scratch path for a preview download (`.img` so image tooling that
/// sniffs extensions sees a neutral suffix).
fn temp_preview_path() -> Result<PathBuf, AppError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut p = std::env::temp_dir();
    p.push(format!("lit-prev-{}-{nanos}.img", std::process::id()));
    Ok(p)
}

/// Read a downloaded preview with a hard cap: one extra byte past the limit
/// tells us the file overshot without buffering it whole.
fn read_preview_bytes(path: &Path) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    let mut buf = Vec::new();
    let mut limited = file.take(MAX_PREVIEW_BYTES as u64 + 1);
    limited.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn remove_quietly(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Run a tool with stdout+stderr merged into a temp file (no pipe deadlock),
/// with a hard timeout. The log file is always cleaned up.
fn run_logged(bin: &Path, args: &[&str], timeout: Duration) -> Result<CmdOutput, AppError> {
    let log_path = temp_log_path()?;
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&log_path)
        .map_err(AppError::from)?;
    let file2 = file.try_clone().map_err(AppError::from)?;

    let mut child = Command::new(bin)
        .args(args)
        // GNU getopt permutes argv by default, so afcclient parses a *command's*
        // flag (`ls -l`) as one of its own options and aborts with
        // "invalid option -- 'l'" (verified against afcclient 1.4.0). Setting
        // POSIXLY_CORRECT stops option parsing at the first operand, so flags
        // after the subcommand reach the command layer exactly as
        // `afcclient help` documents. Harmless for the idevice* tools: their
        // options always precede operands.
        .env("POSIXLY_CORRECT", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(file2))
        .spawn()
        .map_err(|e| {
            let name = bin.file_name().unwrap_or_default().to_string_lossy();
            AppError::new(
                ErrorCode::ToolMissing,
                format!("Could not start {name} ({e})."),
            )
        })?;

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    remove_quietly(&log_path);
                    return Err(AppError::new(
                        ErrorCode::ServiceUnavailable,
                        format!(
                            "{} did not respond in time. Check that the cable is connected \
                             and the iPhone is unlocked.",
                            bin.file_name().unwrap_or_default().to_string_lossy()
                        ),
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                remove_quietly(&log_path);
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("Failed while waiting for a system tool ({e})."),
                ));
            }
        }
    };
    let text = read_limited(&log_path);
    remove_quietly(&log_path);
    Ok(CmdOutput {
        code: status.code(),
        text,
    })
}

// ---------------------------------------------------------------------------
// output classification (pure helpers, unit-tested)
// ---------------------------------------------------------------------------

pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip until a final byte in the CSI sequence.
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() || next == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Extract non-empty, plausible UDIDs from `idevice_id -l` output.
pub fn parse_device_list(text: &str) -> Vec<String> {
    strip_ansi(text)
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("ERROR"))
        .filter(|l| {
            let len = l.len();
            (8..=64).contains(&len)
                && l.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':')
        })
        .map(|l| l.to_string())
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairOutcome {
    Validated,
    NoDevice,
    TrustPending,
    Denied,
    PasscodeSet,
    NotPaired,
    Other(String),
}

/// Map `idevicepair validate` / `pair` output onto UI states.
pub fn classify_pair_output(code: Option<i32>, text: &str) -> PairOutcome {
    let lower = text.to_lowercase();
    if code == Some(0) && !lower.contains("error:") {
        return PairOutcome::Validated;
    }
    if lower.contains("no device found") {
        return PairOutcome::NoDevice;
    }
    if lower.contains("denied the trust dialog") {
        return PairOutcome::Denied;
    }
    if lower.contains("passcode is set") {
        return PairOutcome::PasscodeSet;
    }
    if lower.contains("accept the trust dialog") {
        return PairOutcome::TrustPending;
    }
    if lower.contains("is not paired") || lower.contains("pairing dialog") {
        return PairOutcome::NotPaired;
    }
    let first_error = text
        .lines()
        .find(|l| l.to_lowercase().contains("error"))
        .unwrap_or("")
        .trim()
        .chars()
        .take(200)
        .collect::<String>();
    if first_error.is_empty() {
        PairOutcome::Other(format!("Pairing check failed (exit code {:?}).", code))
    } else {
        PairOutcome::Other(first_error)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LockdownIssue {
    Locked,
    TrustPending,
    NotPaired,
    NoDevice,
    Other(String),
}

/// Map an `ideviceinfo` failure onto a device state.
pub fn classify_info_failure(text: &str) -> LockdownIssue {
    let lower = text.to_lowercase();
    if lower.contains("no device found") || lower.contains("not found!") && lower.contains("device")
    {
        return LockdownIssue::NoDevice;
    }
    if lower.contains("password protected") || lower.contains("invalidpasscode") {
        return LockdownIssue::Locked;
    }
    if lower.contains("pairingdialogresponsepending") || lower.contains("please accept the trust") {
        return LockdownIssue::TrustPending;
    }
    if lower.contains("not paired")
        || lower.contains("invalidhostid")
        || lower.contains("pairing record")
        || lower.contains("ssl error")
    {
        return LockdownIssue::NotPaired;
    }
    if lower.contains("could not connect to lockdownd") {
        // Lockdown reachable but refused; without a more specific reason the
        // safest truthful guidance is unlock + trust.
        return LockdownIssue::Locked;
    }
    let first_error = text
        .lines()
        .find(|l| l.contains("ERROR") || l.to_lowercase().contains("error"))
        .unwrap_or("Device information could not be read.")
        .trim()
        .chars()
        .take(200)
        .collect::<String>();
    LockdownIssue::Other(first_error)
}

pub fn parse_info(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in strip_ansi(text).lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                map.insert(key.to_string(), value.to_string());
            }
        }
    }
    map
}

#[derive(Debug, PartialEq, Eq)]
pub struct LsEntry {
    pub is_dir: bool,
    pub name: String,
    pub size: Option<u64>,
    pub mtime: Option<String>,
}

/// Parse one `afcclient ls -l` line:
/// `drwxr-xr-x 2 mobile mobile      4096 05 Mar 2024 12:00:00 100APPLE`
pub fn parse_ls_l_line(line: &str) -> Option<LsEntry> {
    let clean = strip_ansi(line);
    let tokens: Vec<&str> = clean.split_whitespace().collect();
    if tokens.len() < 10 {
        return None;
    }
    let perms = tokens[0];
    let perm_chars: Vec<char> = perms.chars().collect();
    if perm_chars.len() != 10 || !matches!(perm_chars[0], '-' | 'd' | 'l') {
        return None;
    }
    let is_dir = perm_chars[0] == 'd';
    let size = tokens[4].parse::<u64>().ok();
    let mtime = format!("{} {} {} {}", tokens[5], tokens[6], tokens[7], tokens[8]);
    let name = tokens[9..].join(" ");
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    Some(LsEntry {
        is_dir,
        name,
        size,
        mtime: Some(mtime),
    })
}

pub fn parse_ls_l(text: &str) -> Vec<LsEntry> {
    strip_ansi(text)
        .lines()
        .filter_map(parse_ls_l_line)
        .collect()
}

/// Names from a plain `afcclient ls` (one basename per line, colors stripped).
pub fn parse_ls_names(text: &str) -> Vec<String> {
    strip_ansi(text)
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && *l != "." && *l != "..")
        .filter(|l| !l.starts_with("Error") && !l.starts_with("ERROR"))
        .map(|l| l.to_string())
        .collect()
}

/// Convert afcclient's local-time rendering back into an epoch second.
/// Host-local timezone, so it round-trips the device's displayed time exactly
/// on Unix. Returns `None` when it cannot be represented (and on Windows v1).
pub fn parse_device_time(text: &str) -> Option<i64> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() != 4 {
        return None;
    }
    let day: i32 = parts[0].parse().ok()?;
    let month = month_number(parts[1])?;
    let year: i32 = parts[2].parse().ok()?;
    let time: Vec<&str> = parts[3].split(':').collect();
    if time.len() != 3 {
        return None;
    }
    let hour: i32 = time[0].parse().ok()?;
    let min: i32 = time[1].parse().ok()?;
    let sec: i32 = time[2].parse().ok()?;
    if !(1..=31).contains(&day) || !(0..=23).contains(&hour) || !(0..=59).contains(&min) {
        return None;
    }
    #[cfg(unix)]
    {
        use std::mem;
        let mut tm: libc::tm = unsafe { mem::zeroed() };
        tm.tm_year = year - 1900;
        tm.tm_mon = month - 1;
        tm.tm_mday = day;
        tm.tm_hour = hour;
        tm.tm_min = min;
        tm.tm_sec = sec;
        tm.tm_isdst = -1;
        let epoch = unsafe { libc::mktime(&mut tm) };
        if epoch == -1 {
            None
        } else {
            Some(epoch)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (day, month, year, hour, min, sec);
        None
    }
}

fn month_number(name: &str) -> Option<i32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let lower = name.to_lowercase();
    MONTHS
        .iter()
        .position(|m| *m == lower)
        .map(|i| i as i32 + 1)
}

/// Map afcclient / AFC failure text to a UI-safe error. afcclient exits 0 even
/// on command failures, so text inspection is the authoritative signal.
pub fn classify_afc_failure(text: &str) -> AppError {
    let lower = text.to_lowercase();
    if lower.contains("no device found") || lower.contains("no device with") {
        return AppError::new(
            ErrorCode::NoDevice,
            "Your iPhone is no longer connected. Reconnect the cable and unlock it.",
        );
    }
    if lower.contains("lockdownd")
        || lower.contains("password protected")
        || lower.contains("trust dialog")
        || lower.contains("not paired")
    {
        return AppError::new(
            ErrorCode::NeedsTrust,
            "Unlock your iPhone and tap Trust on the iPhone screen, then try again.",
        );
    }
    if lower.contains("mux error")
        || lower.contains("service not connected")
        || lower.contains("read error")
        || lower.contains("end of data")
        || lower.contains("connection closed")
        || lower.contains("broken pipe")
    {
        return AppError::new(
            ErrorCode::DeviceDisconnected,
            "The connection to your iPhone was lost. Reconnect the cable and try again.",
        );
    }
    if lower.contains("no space left") {
        return AppError::new(
            ErrorCode::OutOfSpace,
            "There is not enough free space on the destination drive.",
        );
    }
    if lower.contains("permission denied") {
        return AppError::new(
            ErrorCode::PermissionDenied,
            "Access was refused. Unlock the iPhone and check this computer's access.",
        );
    }
    if lower.contains("not found") {
        return AppError::new(
            ErrorCode::TransferFailed,
            "That file is no longer available on the iPhone.",
        );
    }
    let snippet = text
        .lines()
        .find(|l| l.contains("Error") || l.contains("ERROR"))
        .unwrap_or("The device could not complete the request.")
        .trim()
        .trim_start_matches("ERROR:")
        .trim_start_matches("Error:")
        .trim()
        .chars()
        .take(200)
        .collect::<String>();
    AppError::new(ErrorCode::TransferFailed, snippet)
}

fn tool_missing(name: &str) -> AppError {
    AppError::new(
        ErrorCode::ToolMissing,
        format!("The required tool '{name}' was not found on this computer."),
    )
}

static LAST_PAIR_ATTEMPT_MS: AtomicU64 = AtomicU64::new(0);

/// Ask lockdownd to start pairing (this is what pops the Trust dialog), but at
/// most once per `PAIR_COOLDOWN` so the poller never spams the device.
fn maybe_trigger_pair(pair_tool: &Path, udid: &str) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let last = LAST_PAIR_ATTEMPT_MS.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) < PAIR_COOLDOWN.as_millis() as u64 {
        return;
    }
    if LAST_PAIR_ATTEMPT_MS
        .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let _ = run_logged(pair_tool, &["-u", udid, "pair"], PAIR_TRIGGER_TIMEOUT);
}

// ---------------------------------------------------------------------------
// detection
// ---------------------------------------------------------------------------

fn needs_trust(message: &str, hint: &str) -> DeviceSnapshot {
    DeviceSnapshot {
        state: DeviceState::NeedsTrust,
        device: None,
        message: message.to_string(),
        hint: Some(hint.to_string()),
        demo: false,
        source: platform_label().to_string(),
    }
}

fn platform_label() -> &'static str {
    "libimobiledevice"
}

impl DeviceProvider for LibimobileProvider {
    fn label(&self) -> &'static str {
        platform_label()
    }

    fn detect(&self) -> Result<DeviceSnapshot, AppError> {
        let tools = platform::tool_report();
        let id_tool = tools
            .idevice_id
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| tool_missing(TOOL_ID))?;

        let out = run_logged(&id_tool, &["-l"], DETECT_TIMEOUT)?;
        if !out.success() && out.text.contains("Unable to retrieve device list") {
            return Err(AppError::new(
                ErrorCode::ServiceUnavailable,
                "This computer's iPhone discovery service (usbmuxd) is not running.",
            ));
        }
        let udid = match parse_device_list(&out.text).into_iter().next() {
            Some(u) => u,
            None => {
                return Ok(DeviceSnapshot {
                    state: DeviceState::NoDevice,
                    device: None,
                    message: "No iPhone detected".to_string(),
                    hint: Some("Connect your iPhone with a USB cable, then unlock it.".to_string()),
                    demo: false,
                    source: platform_label().to_string(),
                })
            }
        };

        let info_tool = tools
            .ideviceinfo
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| tool_missing(TOOL_INFO))?;
        let info = run_logged(&info_tool, &["-u", &udid], INFO_TIMEOUT)?;

        if info.success() {
            let map = parse_info(&info.text);
            let device = DeviceInfo {
                udid: udid.clone(),
                name: map.get("DeviceName").cloned(),
                model: map.get("ProductType").cloned(),
                ios_version: map.get("ProductVersion").cloned(),
            };
            if tools.afcclient.is_none() {
                // Detection works but media access cannot: say so plainly.
                return Ok(DeviceSnapshot {
                    state: DeviceState::Unsupported,
                    device: Some(device),
                    message: "The iPhone is connected, but this computer is missing \
                              the tool needed to read photos (afcclient)."
                        .to_string(),
                    hint: Some(platform::install_hint()),
                    demo: false,
                    source: platform_label().to_string(),
                });
            }
            let message = format!(
                "{} is ready",
                device.name.as_deref().unwrap_or("Your iPhone")
            );
            return Ok(DeviceSnapshot {
                state: DeviceState::Ready,
                device: Some(device),
                message,
                hint: None,
                demo: false,
                source: platform_label().to_string(),
            });
        }

        match classify_info_failure(&info.text) {
            LockdownIssue::NoDevice => Ok(DeviceSnapshot {
                state: DeviceState::NoDevice,
                device: None,
                message: "No iPhone detected".to_string(),
                hint: Some("Connect your iPhone with a USB cable, then unlock it.".into()),
                demo: false,
                source: platform_label().to_string(),
            }),
            LockdownIssue::Locked => Ok(needs_trust(
                "Your iPhone is locked",
                "Unlock your iPhone with Face ID, Touch ID, or your passcode, then keep it unlocked while transferring.",
            )),
            LockdownIssue::TrustPending | LockdownIssue::NotPaired => {
                if let Some(pair_tool) = tools.idevicepair.as_deref().map(PathBuf::from) {
                    // Distinguish "waiting for tap" from "not paired yet", and
                    // start pairing (which shows the Trust prompt) when needed.
                    let pair_out = run_logged(
                        &pair_tool,
                        &["-u", &udid, "validate"],
                        PAIR_VALIDATE_TIMEOUT,
                    )
                    .ok();
                    let outcome = pair_out
                        .as_ref()
                        .map(|o| classify_pair_output(o.code, &o.text));
                    match outcome {
                        Some(PairOutcome::Validated) => {
                            // Pairing is fine; info failed for another reason.
                            Ok(DeviceSnapshot {
                                state: DeviceState::Error,
                                device: None,
                                message: "The iPhone is connected but its details could not be read."
                                    .into(),
                                hint: Some("Unplug the cable, plug it back in, and unlock the iPhone.".into()),
                                demo: false,
                                source: platform_label().to_string(),
                            })
                        }
                        Some(PairOutcome::NoDevice) => Ok(DeviceSnapshot {
                            state: DeviceState::NoDevice,
                            device: None,
                            message: "No iPhone detected".to_string(),
                            hint: Some(
                                "Connect your iPhone with a USB cable, then unlock it.".into(),
                            ),
                            demo: false,
                            source: platform_label().to_string(),
                        }),
                        Some(PairOutcome::Denied) => Ok(needs_trust(
                            "Trust was declined on this iPhone",
                            "On the iPhone, tap Allow or Trust when asked, or reconnect the cable to see the prompt again.",
                        )),
                        Some(PairOutcome::PasscodeSet) => Ok(needs_trust(
                            "Enter the passcode on your iPhone",
                            "Unlock the iPhone first — a passcode is required before this computer can access it.",
                        )),
                        Some(PairOutcome::TrustPending) => Ok(needs_trust(
                            "Waiting for you to trust this computer",
                            "Unlock your iPhone and tap Trust on the iPhone screen.",
                        )),
                        Some(PairOutcome::NotPaired) | Some(PairOutcome::Other(_)) | None => {
                            maybe_trigger_pair(&pair_tool, &udid);
                            Ok(needs_trust(
                                "Unlock your iPhone and tap Trust",
                                "Keep the iPhone unlocked and tap Trust on its screen when the prompt appears.",
                            ))
                        }
                    }
                } else {
                    Err(tool_missing(TOOL_PAIR))
                }
            }
            LockdownIssue::Other(msg) => Err(AppError::new(
                ErrorCode::ServiceUnavailable,
                format!("The iPhone could not be read: {msg}"),
            )),
        }
    }

    fn media(&self) -> &dyn MediaSource {
        self
    }
}

// ---------------------------------------------------------------------------
// media listing + byte access
// ---------------------------------------------------------------------------

fn afc_tool() -> Result<PathBuf, AppError> {
    platform::find_tool(TOOL_AFC)
        .ok_or_else(|| tool_missing(TOOL_AFC))
        .map_err(|e| {
            AppError::new(
                e.code,
                format!("{} Install the libimobiledevice tools first.", e.message),
            )
        })
}

/// afcclient exits 0 even on failures, so inspect the text first. Match on
/// line starts so a filename containing "error" can never trip this.
fn afc_output_failed(out: &CmdOutput) -> bool {
    !out.success()
        || out
            .text
            .lines()
            .any(|l| l.starts_with("Error") || l.starts_with("ERROR"))
}

fn ensure_afc_ok(out: &CmdOutput) -> Result<(), AppError> {
    if afc_output_failed(out) {
        return Err(classify_afc_failure(&out.text));
    }
    Ok(())
}

impl MediaSource for LibimobileProvider {
    fn list_media(&self, udid: &str) -> Result<Vec<MediaItem>, AppError> {
        let afc = afc_tool()?;

        let root = run_logged(&afc, &["-u", udid, "ls", "/"], AFC_LIST_TIMEOUT)?;
        ensure_afc_ok(&root)?;
        let root_entries = parse_ls_names(&root.text);
        let dcim = root_entries
            .iter()
            .find(|n| n.eq_ignore_ascii_case("DCIM"))
            .cloned();
        let dcim = match dcim {
            Some(d) => d,
            None => {
                return Err(AppError::new(
                    ErrorCode::MediaUnavailable,
                    "This iPhone does not expose a photo library over USB right now. \
                     Make sure it is unlocked and iCloud Photos has finished syncing.",
                ))
            }
        };

        let mut items = Vec::new();
        let mut subdirs: Vec<String> = Vec::new();

        // `dcim` is the entry name found in the AFC root listing, so it is
        // already the top-level folder ("DCIM" on stock iOS). Prefixing it
        // again would probe "/DCIM/DCIM" and fail with "Not found".
        let dcim_l = format!("/{dcim}");
        let top = run_logged(&afc, &["-u", udid, "ls", "-l", &dcim_l], AFC_LIST_TIMEOUT)?;
        ensure_afc_ok(&top)?;
        for entry in parse_ls_l(&top.text) {
            let path = format!("{dcim_l}/{}", entry.name);
            if entry.is_dir {
                if subdirs.len() < MAX_DCIM_DIRS {
                    subdirs.push(entry.name);
                }
            } else if let Some(item) =
                build_item(&path, &entry.name, entry.size, entry.mtime.as_deref())
            {
                items.push(item);
            }
        }

        for dir in subdirs {
            let dir_path = format!("{dcim_l}/{dir}");
            let out = match run_logged(&afc, &["-u", udid, "ls", "-l", &dir_path], AFC_LIST_TIMEOUT)
            {
                Ok(o) => o,
                // A single unreadable subfolder must not abort the whole scan.
                Err(_) => continue,
            };
            if afc_output_failed(&out) {
                continue;
            }
            for entry in parse_ls_l(&out.text) {
                if entry.is_dir {
                    continue;
                }
                let path = format!("{dir_path}/{}", entry.name);
                if let Some(item) =
                    build_item(&path, &entry.name, entry.size, entry.mtime.as_deref())
                {
                    items.push(item);
                }
            }
        }

        // Newest first; items without a date sort last but keep name order.
        items.sort_by(|a, b| match (b.modified_ms, a.modified_ms) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        Ok(items)
    }

    /// Read the thumbnail iOS pre-generated for this file (never the original
    /// media): `preview_dirs` derives candidates from a validated `/DCIM/…`
    /// path, so nothing outside the library can be requested. A missing
    /// thumbnail is `Ok(None)`; only device-level failures become errors.
    fn preview(&self, udid: &str, source_path: &str) -> Result<Option<PreviewImage>, AppError> {
        let afc = afc_tool()?;
        for dir in preview::preview_dirs(source_path)? {
            let listing =
                match run_logged(&afc, &["-u", udid, "ls", "-l", &dir], AFC_PREVIEW_TIMEOUT) {
                    Ok(out) => out,
                    Err(e) if e.is_device_level() => return Err(e),
                    Err(_) => continue,
                };
            let entry = if afc_output_failed(&listing) {
                // Not found here is normal (e.g. no VideoKeyFrames entry):
                // try the next candidate unless the device itself went away.
                let err = classify_afc_failure(&listing.text);
                if err.is_device_level() {
                    return Err(err);
                }
                continue;
            } else {
                parse_ls_l(&listing.text).into_iter().find(|e| {
                    !e.is_dir
                        && preview::is_preview_file_name(&e.name)
                        && e.size.is_none_or(|s| s <= MAX_PREVIEW_BYTES as u64)
                })
            };
            let Some(entry) = entry else { continue };

            let remote = format!("{dir}/{}", entry.name);
            let local = temp_preview_path()?;
            let fetched = run_logged(
                &afc,
                &["-u", udid, "get", &remote, &local.to_string_lossy()],
                AFC_PREVIEW_TIMEOUT,
            );
            let image = match fetched {
                Ok(out) if !afc_output_failed(&out) => {
                    read_preview_bytes(&local).and_then(preview::prepare_preview)
                }
                Ok(out) => {
                    let err = classify_afc_failure(&out.text);
                    if err.is_device_level() {
                        remove_quietly(&local);
                        return Err(err);
                    }
                    None
                }
                Err(e) if e.is_device_level() => {
                    remove_quietly(&local);
                    return Err(e);
                }
                Err(_) => None,
            };
            remove_quietly(&local);
            if image.is_some() {
                return Ok(image);
            }
        }
        Ok(None)
    }

    fn copy_to(
        &self,
        udid: &str,
        source_path: &str,
        dest_file: &Path,
        expected_size: Option<u64>,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64),
    ) -> Result<u64, AppError> {
        validate_source_path(source_path)?;
        let afc = afc_tool()?;
        let dest_str = dest_file
            .to_str()
            .ok_or_else(|| AppError::new(ErrorCode::Internal, "Destination path is not valid."))?;

        let log_path = temp_log_path()?;
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&log_path)
            .map_err(AppError::from)?;
        let file2 = file.try_clone().map_err(AppError::from)?;

        let mut child = Command::new(&afc)
            .args(["-u", udid, "get", source_path, dest_str])
            .stdin(Stdio::null())
            .stdout(Stdio::from(file))
            .stderr(Stdio::from(file2))
            .spawn()
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ToolMissing,
                    format!("Could not start {TOOL_AFC} ({e})."),
                )
            })?;

        progress(0);
        let mut last_size = 0u64;
        let mut last_change = Instant::now();

        let result = loop {
            if cancel.load(Ordering::SeqCst) {
                break Err(AppError::new(
                    ErrorCode::Cancelled,
                    "The transfer was cancelled.",
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    let size = std::fs::metadata(dest_file).map(|m| m.len()).unwrap_or(0);
                    progress(size);
                    let text = read_limited(&log_path);
                    let has_error = text
                        .lines()
                        .any(|l| l.starts_with("Error") || l.starts_with("ERROR"));
                    if status.code() != Some(0) || has_error {
                        break Err(classify_afc_failure(&text));
                    }
                    if let Some(expected) = expected_size {
                        if size != expected {
                            break Err(AppError::new(
                                ErrorCode::TransferFailed,
                                "The copy stopped before the file was complete, so it was not saved.",
                            ));
                        }
                    } else if size == 0 {
                        break Err(AppError::new(
                            ErrorCode::TransferFailed,
                            "The device returned an empty file, so it was not saved.",
                        ));
                    }
                    break Ok(size);
                }
                Ok(None) => {
                    let size = std::fs::metadata(dest_file).map(|m| m.len()).unwrap_or(0);
                    if size != last_size {
                        last_size = size;
                        last_change = Instant::now();
                        progress(size);
                    } else if last_change.elapsed() > STALL_TIMEOUT {
                        break Err(AppError::new(
                            ErrorCode::DeviceDisconnected,
                            "The transfer stopped because the iPhone stopped sending data. \
                             Reconnect the cable and try again.",
                        ));
                    }
                    std::thread::sleep(AFC_POLL);
                }
                Err(e) => {
                    break Err(AppError::new(
                        ErrorCode::Internal,
                        format!("The copy process failed ({e})."),
                    ));
                }
            }
        };

        // Always reap and clean up the log.
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
        remove_quietly(&log_path);
        result
    }
}

fn build_item(path: &str, name: &str, size: Option<u64>, mtime: Option<&str>) -> Option<MediaItem> {
    let kind = kind_for_name(name)?;
    let modified_ms = mtime
        .and_then(parse_device_time)
        .map(|secs| secs.saturating_mul(1000));
    Some(MediaItem {
        id: path.to_string(),
        name: name.to_string(),
        kind,
        size,
        modified_ms,
        source_path: path.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_list() {
        let out = "00008101-001A2B3C4D5E6F70\n\nERROR: No device\n";
        assert_eq!(parse_device_list(out).len(), 1);
        assert!(parse_device_list("").is_empty());
        assert!(parse_device_list("ERROR: Unable to retrieve device list!\n").is_empty());
        assert!(parse_device_list("spaces in here are invalid\n").is_empty());
    }

    #[test]
    fn classifies_pairing_states() {
        assert_eq!(
            classify_pair_output(Some(0), "Validated with device"),
            PairOutcome::Validated
        );
        assert_eq!(
            classify_pair_output(Some(1), "No device found."),
            PairOutcome::NoDevice
        );
        assert_eq!(
            classify_pair_output(
                Some(1),
                "ERROR: Please accept the trust dialog on the screen of device X, then attempt to pair again."
            ),
            PairOutcome::TrustPending
        );
        assert_eq!(
            classify_pair_output(
                Some(1),
                "ERROR: Device X said that the user denied the trust dialog."
            ),
            PairOutcome::Denied
        );
        assert_eq!(
            classify_pair_output(Some(1), "ERROR: Device X is not paired with this host"),
            PairOutcome::NotPaired
        );
        assert_eq!(
            classify_pair_output(
                Some(1),
                "ERROR: Could not validate with device X because a passcode is set. Please enter the passcode on the device and retry."
            ),
            PairOutcome::PasscodeSet
        );
    }

    #[test]
    fn classifies_lockdown_failures() {
        assert_eq!(
            classify_info_failure(
                "ERROR: Could not connect to lockdownd: Password protected (-17)"
            ),
            LockdownIssue::Locked
        );
        assert_eq!(
            classify_info_failure(
                "ERROR: Could not connect to lockdownd: PairingDialogResponsePending (-16)"
            ),
            LockdownIssue::TrustPending
        );
        assert_eq!(
            classify_info_failure("ERROR: No device found!"),
            LockdownIssue::NoDevice
        );
        assert_eq!(
            classify_info_failure("ERROR: Could not connect to lockdownd: InvalidHostID (-18)"),
            LockdownIssue::NotPaired
        );
        assert!(matches!(
            classify_info_failure("ERROR: something odd happened"),
            LockdownIssue::Other(_)
        ));
    }

    #[test]
    fn parses_afc_long_listing() {
        let line = "drwxr-xr-x  2 mobile mobile      4096 05 Mar 2024 12:00:00 100APPLE";
        let e = parse_ls_l_line(line).expect("parses");
        assert!(e.is_dir);
        assert_eq!(e.name, "100APPLE");
        assert_eq!(e.size, Some(4096));

        let file = "-rw-r--r--  1 mobile mobile   1234567 17 Apr 2025 08:31:59 IMG_0042.HEIC";
        let e = parse_ls_l_line(file).expect("parses");
        assert!(!e.is_dir);
        assert_eq!(e.name, "IMG_0042.HEIC");
        assert_eq!(e.size, Some(1234567));
        assert_eq!(e.mtime.as_deref(), Some("17 Apr 2025 08:31:59"));

        // ANSI colours must not break parsing.
        let colored = "\u{1b}[36mdrwxr-xr-x\u{1b}[0m 2 mobile mobile 4096 05 Mar 2024 12:00:00 \u{1b}[36m100APPLE\u{1b}[0m";
        let e = parse_ls_l_line(colored).expect("parses colored");
        assert_eq!(e.name, "100APPLE");

        assert!(parse_ls_l_line("not a listing line").is_none());
        assert!(parse_ls_l_line("").is_none());
    }

    #[test]
    fn strips_ansi_sequences() {
        assert_eq!(strip_ansi("\u{1b}[35mmagenta\u{1b}[0m"), "magenta");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn parses_device_times() {
        assert_eq!(month_number("Mar"), Some(3));
        assert_eq!(month_number("dec"), Some(12));
        assert_eq!(month_number("Nope"), None);
        assert!(parse_device_time("2024 Mar 05 12:00:00").is_none());
        let epoch = parse_device_time("05 Mar 2024 12:00:00");
        #[cfg(unix)]
        {
            let epoch = epoch.expect("parses on unix");
            // 2024-03-05 00:00 UTC ± one day covers every timezone.
            let lower = 1_709_596_800 - 86_400;
            let upper = 1_709_596_800 + 86_400;
            assert!(
                epoch >= lower && epoch <= upper,
                "epoch {epoch} outside plausible range"
            );
        }
        #[cfg(not(unix))]
        assert!(epoch.is_none());
    }

    #[test]
    fn maps_afc_failures_to_ui_errors() {
        let e = classify_afc_failure("ERROR: No device found!");
        assert_eq!(e.code, ErrorCode::NoDevice);
        let e = classify_afc_failure("Error: Failed to read from file '/DCIM/x': Not found (8)");
        assert_eq!(e.code, ErrorCode::TransferFailed);
        let e = classify_afc_failure("ERROR: Could not connect to lockdownd: Password protected");
        assert_eq!(e.code, ErrorCode::NeedsTrust);
        let e = classify_afc_failure("AFC error: No space left on device");
        assert_eq!(e.code, ErrorCode::OutOfSpace);
        let e = classify_afc_failure("ERROR: MUX error");
        assert_eq!(e.code, ErrorCode::DeviceDisconnected);
        assert!(e.is_device_level());
    }

    #[test]
    fn detects_empty_listing_names() {
        assert_eq!(
            parse_ls_names("IMG_1.HEIC\nIMG_2.MOV\nError: not found\n"),
            vec!["IMG_1.HEIC".to_string(), "IMG_2.MOV".to_string()]
        );
    }
}
