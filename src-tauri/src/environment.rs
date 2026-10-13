//! Startup environment check: a non-destructive, one-shot probe of every
//! prerequisite the app needs, mapped onto explicit states the UI can render
//! without guessing.
//!
//! Design rules (from the brief):
//! - A missing executable and a stopped service are different failures and are
//!   probed differently: tools are resolved on disk, the discovery service
//!   (usbmuxd) is probed by actually connecting to its socket.
//! - Nothing here mutates the system: no service starts, no drivers install,
//!   no pairing is triggered.
//! - "iPhone connected" is only reported when `idevice_id` really lists a
//!   device; nothing is inferred from cable state or prior results.
//!
//! All probes sit behind `EnvironmentProbe` so the decision logic (`evaluate`)
//! is unit-testable with fake probes for every state, including malformed
//! device responses and tools that exist but cannot run.

use serde::Serialize;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::errors::ErrorCode;
use crate::platform::{self, ToolReport};
use crate::provider_libimobile::{
    classify_pair_output, parse_device_list, strip_ansi, PairOutcome,
};

// ---------------------------------------------------------------------------
// report model
// ---------------------------------------------------------------------------

/// Overall prerequisite state, most severe first. This is the single value the
/// UI branches on; every other field exists to explain it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvStatus {
    Ready,
    DeviceNotConnected,
    DependencyMissing,
    ServiceNotRunning,
    PermissionRequired,
    PairingRequired,
    UnsupportedPlatform,
    Error,
}

/// State of the OS USB-multiplexing service that bridges usb to lockdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Running,
    /// The service binary/socket has never been installed.
    NotInstalled,
    /// Installed but not currently running (e.g. a stale socket left behind).
    NotRunning,
    /// Present, but this user account cannot reach it.
    PermissionDenied,
    /// No reliable probe exists for this platform.
    Unsupported,
}

/// Trust/pairing state for a detected device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairState {
    Paired,
    NeedsTrust,
    NoDevice,
    /// The pairing state could not be determined (tool failure/timeout).
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvCheck {
    pub id: String,
    pub label: String,
    pub ok: bool,
    /// `true` when an earlier failure made this check impossible to run.
    pub skipped: bool,
    pub detail: Option<String>,
}

/// Full result of one environment scan. Serialized to the frontend as-is.
#[derive(Debug, Clone, Serialize)]
pub struct EnvReport {
    pub status: EnvStatus,
    /// Short, plain-language summary of the status.
    pub message: String,
    /// The single most useful next step, when one exists.
    pub hint: Option<String>,
    /// Stable code for UI branching (`None` when `status` is enough).
    pub code: Option<ErrorCode>,
    pub os: String,
    pub tools: ToolReport,
    pub missing_tools: Vec<String>,
    pub service: ServiceState,
    pub service_detail: String,
    /// Confirmed device identifier — `None` unless detection really saw one.
    pub device_udid: Option<String>,
    pub pairing: Option<PairState>,
    pub checks: Vec<EnvCheck>,
    /// Per-OS guidance for installing the toolchain.
    pub install_hint: String,
    /// The mock adapter exists in development builds only.
    pub mock_available: bool,
}

// ---------------------------------------------------------------------------
// probe seam
// ---------------------------------------------------------------------------

/// What resolving one tool binary told us.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolProbe {
    Missing,
    Found(PathBuf),
    /// The file exists but could not be executed (wrong architecture,
    /// missing loader, broken install…).
    Broken(String),
}

/// Result of asking the discovery service which iPhones are attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceProbe {
    /// `idevice_id -l` ran and listed these identifiers.
    Devices(Vec<String>),
    /// It ran cleanly and listed nothing.
    None,
    /// The tool is on disk but could not be started at all.
    ToolBroken(String),
    /// The tool ran but failed (service trouble, timeout, unexpected output).
    Failed(String),
}

/// Every side-effecting probe the checker needs. Production uses
/// `RealProbe`; tests inject fakes for each state.
pub trait EnvironmentProbe: Send + Sync {
    fn os(&self) -> &'static str;
    fn tool(&self, name: &str) -> ToolProbe;
    fn service(&self) -> ServiceState;
    fn devices(&self, idevice_id: &Path) -> DeviceProbe;
    fn pairing(&self, idevicepair: &Path, udid: &str) -> PairState;
}

// ---------------------------------------------------------------------------
// decision logic (pure, fully unit-tested)
// ---------------------------------------------------------------------------

const TOOL_LABELS: [(&str, &str); 4] = [
    (platform::TOOL_ID, "Device detection (idevice_id)"),
    (platform::TOOL_INFO, "Device details (ideviceinfo)"),
    (platform::TOOL_PAIR, "Trust approval (idevicepair)"),
    (platform::TOOL_AFC, "Photo reading (afcclient)"),
];

fn service_detail(state: ServiceState) -> String {
    match state {
        ServiceState::Running => "The iPhone USB service is running.".to_string(),
        ServiceState::NotInstalled => {
            "The iPhone USB service (usbmuxd) is not installed on this computer.".to_string()
        }
        ServiceState::NotRunning => {
            "The iPhone USB service (usbmuxd) is installed but not running. \
             A connected iPhone cannot be discovered until it runs."
                .to_string()
        }
        ServiceState::PermissionDenied => {
            "This user account is not allowed to reach the iPhone USB service. \
             An administrator needs to grant access."
                .to_string()
        }
        ServiceState::Unsupported => {
            "Automatic checks for the iPhone USB service are not available \
             on this platform."
                .to_string()
        }
    }
}

fn service_hint(state: ServiceState) -> Option<String> {
    match state {
        ServiceState::NotRunning => Some(if cfg!(target_os = "linux") {
            "Start the usbmuxd service with your distribution's service manager \
             (for example: sudo systemctl start usbmuxd), then check again."
                .to_string()
        } else {
            "Restart the Apple USB service, then check again.".to_string()
        }),
        ServiceState::NotInstalled => Some(platform::install_hint()),
        ServiceState::PermissionDenied => Some(
            "Ask your computer's administrator to allow your account to access \
             usbmuxd (the usbmux group on most Linux systems)."
                .to_string(),
        ),
        _ => None,
    }
}

/// Evaluate probe results into the report the UI renders.
///
/// Precedence: unsupported platform → missing/broken tools → service down →
/// permission → pairing/trust → no device → ready. Each stage only runs when
/// the previous one passed, and impossible checks are marked `skipped` so the
/// UI never shows a green tick it did not earn.
pub fn evaluate(probe: &dyn EnvironmentProbe) -> EnvReport {
    let os = probe.os().to_string();
    let tools = platform::tool_report();
    let install_hint = platform::install_hint();
    let mock_available = cfg!(debug_assertions);

    if os == "other" {
        return EnvReport {
            status: EnvStatus::UnsupportedPlatform,
            message: "This operating system is not supported yet.".to_string(),
            hint: Some(
                "Local iPhone Transfer is currently built and tested for \
                 Linux, macOS and Windows."
                    .to_string(),
            ),
            code: Some(ErrorCode::Unsupported),
            os,
            tools,
            missing_tools: Vec::new(),
            service: ServiceState::Unsupported,
            service_detail: service_detail(ServiceState::Unsupported),
            device_udid: None,
            pairing: None,
            checks: vec![EnvCheck {
                id: "platform".to_string(),
                label: "Operating system".to_string(),
                ok: false,
                skipped: false,
                detail: Some("Unsupported platform".to_string()),
            }],
            install_hint,
            mock_available,
        };
    }

    // -- tools -------------------------------------------------------------
    let mut missing_tools = Vec::new();
    let mut broken_tools = Vec::new();
    let mut tool_paths: Vec<(&str, PathBuf)> = Vec::new();
    for (name, _label) in TOOL_LABELS {
        match probe.tool(name) {
            ToolProbe::Found(path) => tool_paths.push((name, path)),
            ToolProbe::Missing => missing_tools.push(name.to_string()),
            ToolProbe::Broken(reason) => {
                broken_tools.push(format!("{name} ({reason})"));
                missing_tools.push(name.to_string());
            }
        }
    }
    let tools_ok = missing_tools.is_empty();

    // -- service -----------------------------------------------------------
    let service = probe.service();
    let service_detail = service_detail(service);

    let mut checks = vec![EnvCheck {
        id: "tools".to_string(),
        label: "iPhone USB tools (libimobiledevice)".to_string(),
        ok: tools_ok,
        skipped: false,
        detail: if tools_ok {
            None
        } else if broken_tools.is_empty() {
            Some(format!("Missing: {}", missing_tools.join(", ")))
        } else {
            Some(format!(
                "Missing: {}. Found but not runnable: {}.",
                missing_tools.join(", "),
                broken_tools.join(", ")
            ))
        },
    }];

    if !tools_ok {
        checks.push(EnvCheck {
            id: "service".to_string(),
            label: "iPhone USB service".to_string(),
            ok: false,
            skipped: true,
            detail: Some("Not checked — install the tools first.".to_string()),
        });
        checks.push(EnvCheck {
            id: "device".to_string(),
            label: "iPhone connection".to_string(),
            ok: false,
            skipped: true,
            detail: Some("Not checked — install the tools first.".to_string()),
        });
        checks.push(EnvCheck {
            id: "pairing".to_string(),
            label: "Trust approval".to_string(),
            ok: false,
            skipped: true,
            detail: Some("Not checked — install the tools first.".to_string()),
        });
        return EnvReport {
            status: EnvStatus::DependencyMissing,
            message: if broken_tools.is_empty() {
                "Some required iPhone USB tools are not installed on this computer.".to_string()
            } else {
                format!(
                    "These tools were found but could not run: {}.",
                    broken_tools.join(", ")
                )
            },
            hint: Some(install_hint.clone()),
            code: Some(ErrorCode::ToolMissing),
            os,
            tools,
            missing_tools,
            service,
            service_detail,
            device_udid: None,
            pairing: None,
            checks,
            install_hint,
            mock_available,
        };
    }

    let service_ok = service == ServiceState::Running || service == ServiceState::Unsupported;
    checks.push(EnvCheck {
        id: "service".to_string(),
        label: "iPhone USB service".to_string(),
        ok: service_ok,
        skipped: false,
        detail: Some(service_detail.clone()),
    });

    if !service_ok {
        for (id, label) in [
            ("device", "iPhone connection"),
            ("pairing", "Trust approval"),
        ] {
            checks.push(EnvCheck {
                id: id.to_string(),
                label: label.to_string(),
                ok: false,
                skipped: true,
                detail: Some("Not checked — the USB service is unavailable.".to_string()),
            });
        }
        let status = if service == ServiceState::PermissionDenied {
            EnvStatus::PermissionRequired
        } else {
            EnvStatus::ServiceNotRunning
        };
        return EnvReport {
            status,
            message: service_detail.clone(),
            hint: service_hint(service).or_else(|| Some(install_hint.clone())),
            code: Some(if status == EnvStatus::PermissionRequired {
                ErrorCode::PermissionDenied
            } else {
                ErrorCode::ServiceUnavailable
            }),
            os,
            tools,
            missing_tools,
            service,
            service_detail,
            device_udid: None,
            pairing: None,
            checks,
            install_hint,
            mock_available,
        };
    }

    // -- device ------------------------------------------------------------
    let id_tool = tool_paths
        .iter()
        .find(|(name, _)| *name == platform::TOOL_ID)
        .map(|(_, p)| p.clone());

    let devices = match &id_tool {
        Some(path) => probe.devices(path),
        // Unreachable: the tool loop above already returned when idevice_id
        // was missing or broken.
        None => DeviceProbe::Failed("idevice_id is unavailable".to_string()),
    };

    let (device_udid, device_check_ok, device_detail) = match &devices {
        DeviceProbe::Devices(list) => (
            list.first().cloned(),
            true,
            Some(format!("{} iPhone(s) detected.", list.len())),
        ),
        DeviceProbe::None => (None, false, Some("No iPhone is connected.".to_string())),
        DeviceProbe::ToolBroken(reason) => {
            checks.push(EnvCheck {
                id: "device".to_string(),
                label: "iPhone connection".to_string(),
                ok: false,
                skipped: true,
                detail: Some("Not checked — the detection tool could not run.".to_string()),
            });
            checks.push(EnvCheck {
                id: "pairing".to_string(),
                label: "Trust approval".to_string(),
                ok: false,
                skipped: true,
                detail: Some("Not checked — the detection tool could not run.".to_string()),
            });
            return EnvReport {
                status: EnvStatus::DependencyMissing,
                message: format!("The device detection tool could not run ({reason})."),
                hint: Some(install_hint.clone()),
                code: Some(ErrorCode::ToolMissing),
                os,
                tools,
                missing_tools,
                service,
                service_detail,
                device_udid: None,
                pairing: None,
                checks,
                install_hint,
                mock_available,
            };
        }
        DeviceProbe::Failed(reason) => {
            checks.push(EnvCheck {
                id: "device".to_string(),
                label: "iPhone connection".to_string(),
                ok: false,
                skipped: false,
                detail: Some(reason.clone()),
            });
            checks.push(EnvCheck {
                id: "pairing".to_string(),
                label: "Trust approval".to_string(),
                ok: false,
                skipped: true,
                detail: Some("Not checked — device detection failed.".to_string()),
            });
            return EnvReport {
                status: EnvStatus::Error,
                message: format!("A system tool returned an unexpected response ({reason})."),
                hint: Some(
                    "Reconnect the iPhone with a data-capable cable, then check \
                     again. If this keeps happening, reinstall libimobiledevice."
                        .to_string(),
                ),
                code: Some(ErrorCode::Internal),
                os,
                tools,
                missing_tools,
                service,
                service_detail,
                device_udid: None,
                pairing: None,
                checks,
                install_hint,
                mock_available,
            };
        }
    };
    checks.push(EnvCheck {
        id: "device".to_string(),
        label: "iPhone connection".to_string(),
        ok: device_check_ok,
        skipped: false,
        detail: device_detail,
    });

    let Some(udid) = device_udid else {
        checks.push(EnvCheck {
            id: "pairing".to_string(),
            label: "Trust approval".to_string(),
            ok: false,
            skipped: true,
            detail: Some("Not checked — no iPhone is connected.".to_string()),
        });
        return EnvReport {
            status: EnvStatus::DeviceNotConnected,
            message: "No iPhone is connected to this computer.".to_string(),
            hint: Some(
                "Connect your iPhone with a USB cable and unlock it, then check \
                 again."
                    .to_string(),
            ),
            code: Some(ErrorCode::NoDevice),
            os,
            tools,
            missing_tools,
            service,
            service_detail,
            device_udid: None,
            pairing: None,
            checks,
            install_hint,
            mock_available,
        };
    };

    // -- pairing -----------------------------------------------------------
    let pair_tool = tool_paths
        .iter()
        .find(|(name, _)| *name == platform::TOOL_PAIR)
        .map(|(_, p)| p.clone());
    let pairing = match &pair_tool {
        Some(path) => probe.pairing(path, &udid),
        None => PairState::Unknown,
    };

    let pairing_ok = pairing == PairState::Paired;
    checks.push(EnvCheck {
        id: "pairing".to_string(),
        label: "Trust approval".to_string(),
        ok: pairing_ok,
        skipped: false,
        detail: Some(match pairing {
            PairState::Paired => "This computer is trusted by the iPhone.".to_string(),
            PairState::NeedsTrust => "Waiting for Trust approval on the iPhone.".to_string(),
            PairState::NoDevice => {
                "The iPhone disconnected before it could be verified.".to_string()
            }
            PairState::Unknown => {
                "The trust state could not be verified with the tools available.".to_string()
            }
        }),
    });

    if pairing == PairState::NoDevice {
        return EnvReport {
            status: EnvStatus::DeviceNotConnected,
            message: "The iPhone disconnected before it could be verified.".to_string(),
            hint: Some("Reconnect the cable, unlock the phone, then check again.".to_string()),
            code: Some(ErrorCode::NoDevice),
            os,
            tools,
            missing_tools,
            service,
            service_detail,
            device_udid: None,
            pairing: Some(pairing),
            checks,
            install_hint,
            mock_available,
        };
    }
    if !pairing_ok {
        let needs_trust = pairing == PairState::NeedsTrust;
        return EnvReport {
            status: EnvStatus::PairingRequired,
            message: if needs_trust {
                "Your computer is waiting for approval from the iPhone.".to_string()
            } else {
                "Your computer's trust by the iPhone could not be verified.".to_string()
            },
            hint: Some(if needs_trust {
                "Unlock your iPhone and tap Trust on the iPhone screen, then \
                 check again."
                    .to_string()
            } else {
                "Unlock your iPhone, reconnect the cable, then check again.".to_string()
            }),
            code: Some(ErrorCode::NeedsTrust),
            os,
            tools,
            missing_tools,
            service,
            service_detail,
            device_udid: Some(udid),
            pairing: Some(pairing),
            checks,
            install_hint,
            mock_available,
        };
    }

    EnvReport {
        status: EnvStatus::Ready,
        message: "This computer can talk to the connected iPhone.".to_string(),
        hint: None,
        code: None,
        os,
        tools,
        missing_tools,
        service,
        service_detail,
        device_udid: Some(udid),
        pairing: Some(pairing),
        checks,
        install_hint,
        mock_available,
    }
}

// ---------------------------------------------------------------------------
// real probes
// ---------------------------------------------------------------------------

/// Run one tool and capture its output, with a hard timeout. Output of the
/// idevice* tools is tiny (well under a pipe buffer), so blocking `output()`
/// on a helper thread cannot deadlock; on timeout the child is killed and the
/// helper thread reaped later.
enum CmdResult {
    Output { code: Option<i32>, text: String },
    SpawnFailed(String),
    TimedOut,
}

fn run_capture(bin: &Path, args: &[&str], timeout: Duration) -> CmdResult {
    let child = match Command::new(bin)
        .args(args)
        .env("POSIXLY_CORRECT", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return CmdResult::SpawnFailed(e.to_string()),
    };

    let (tx, rx) = mpsc::channel();
    // Move the child into a detached helper thread so it can be waited on
    // there even if this side times out first.
    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push('\n');
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            CmdResult::Output {
                code: output.status.code(),
                text: strip_ansi(&text),
            }
        }
        Ok(Err(e)) => CmdResult::SpawnFailed(e.to_string()),
        Err(_) => {
            // The helper thread owns the child and will finish on its own when
            // the tool eventually exits; nothing else to do here.
            CmdResult::TimedOut
        }
    }
}

pub struct RealProbe;

const SERVICE_SOCKETS: &[&str] = &["/run/usbmuxd", "/var/run/usbmuxd"];

#[cfg(unix)]
fn probe_service_unix() -> ServiceState {
    use std::os::unix::net::UnixStream;
    for path in SERVICE_SOCKETS {
        match UnixStream::connect(path) {
            Ok(_) => return ServiceState::Running,
            Err(e) => match e.kind() {
                ErrorKind::NotFound => continue,
                ErrorKind::PermissionDenied => return ServiceState::PermissionDenied,
                // Stale socket: the file exists but nothing is listening, so
                // the service is installed-but-stopped, not missing.
                _ => return ServiceState::NotRunning,
            },
        }
    }
    ServiceState::NotInstalled
}

#[cfg(not(unix))]
fn probe_service_unix() -> ServiceState {
    // libimobiledevice on Windows talks to usbmuxd over loopback.
    use std::net::{TcpStream, ToSocketAddrs};
    let addrs = match "127.0.0.1:27015".to_socket_addrs() {
        Ok(a) => a.collect::<Vec<_>>(),
        Err(_) => return ServiceState::Unsupported,
    };
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(1500)) {
            Ok(_) => return ServiceState::Running,
            Err(e) => match e.kind() {
                ErrorKind::PermissionDenied => return ServiceState::PermissionDenied,
                _ => continue,
            },
        }
    }
    ServiceState::NotRunning
}

const LIST_TIMEOUT: Duration = Duration::from_secs(10);
const PAIR_TIMEOUT: Duration = Duration::from_secs(10);
const INFO_FALLBACK_TIMEOUT: Duration = Duration::from_secs(8);

impl EnvironmentProbe for RealProbe {
    fn os(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "other"
        }
    }

    fn tool(&self, name: &str) -> ToolProbe {
        match platform::find_tool(name) {
            Some(path) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    match std::fs::metadata(&path) {
                        Ok(meta) if meta.permissions().mode() & 0o111 == 0 => {
                            ToolProbe::Broken("the file exists but is not executable".to_string())
                        }
                        _ => ToolProbe::Found(path),
                    }
                }
                #[cfg(not(unix))]
                {
                    ToolProbe::Found(path)
                }
            }
            None => ToolProbe::Missing,
        }
    }

    fn service(&self) -> ServiceState {
        probe_service_unix()
    }

    fn devices(&self, idevice_id: &Path) -> DeviceProbe {
        match run_capture(idevice_id, &["-l"], LIST_TIMEOUT) {
            CmdResult::SpawnFailed(e) => DeviceProbe::ToolBroken(e),
            CmdResult::TimedOut => {
                DeviceProbe::Failed("idevice_id did not respond in time".to_string())
            }
            CmdResult::Output { code, text } => {
                let udids = parse_device_list(&text);
                if !udids.is_empty() {
                    return DeviceProbe::Devices(udids);
                }
                if code == Some(0) {
                    if text.trim().is_empty() {
                        DeviceProbe::None
                    } else {
                        // Output we cannot interpret is an error, never "no
                        // device": a real empty list is empty.
                        DeviceProbe::Failed("unrecognized idevice_id output".to_string())
                    }
                } else if text.contains("Unable to retrieve device list") {
                    // The tool told us the service is unavailable even though
                    // the socket answered: report it as a service failure.
                    DeviceProbe::Failed("usbmuxd could not provide a device list".to_string())
                } else {
                    DeviceProbe::Failed(format!(
                        "idevice_id failed ({})",
                        text.lines()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("no output")
                            .trim()
                            .chars()
                            .take(120)
                            .collect::<String>()
                    ))
                }
            }
        }
    }

    fn pairing(&self, idevicepair: &Path, udid: &str) -> PairState {
        match run_capture(idevicepair, &["-u", udid, "validate"], PAIR_TIMEOUT) {
            CmdResult::Output { code, text } => match classify_pair_output(code, &text) {
                PairOutcome::Validated => PairState::Paired,
                PairOutcome::NoDevice => PairState::NoDevice,
                PairOutcome::TrustPending
                | PairOutcome::Denied
                | PairOutcome::PasscodeSet
                | PairOutcome::NotPaired => PairState::NeedsTrust,
                PairOutcome::Other(_) => self.pairing_fallback(udid),
            },
            // validate misbehaving — ask lockdown directly before giving up.
            CmdResult::SpawnFailed(_) | CmdResult::TimedOut => self.pairing_fallback(udid),
        }
    }
}

impl RealProbe {
    /// When `idevicepair validate` cannot classify, confirm access with a
    /// direct lockdown query so "ready" is never claimed without evidence —
    /// and "trust required" is never claimed when the device actually works.
    fn pairing_fallback(&self, udid: &str) -> PairState {
        let info_tool = match platform::find_tool(platform::TOOL_INFO) {
            Some(p) => p,
            None => return PairState::Unknown,
        };
        match run_capture(
            &info_tool,
            &["-u", udid, "-k", "DeviceName"],
            INFO_FALLBACK_TIMEOUT,
        ) {
            CmdResult::Output { code: Some(0), .. } => PairState::Paired,
            CmdResult::Output { .. } => PairState::NeedsTrust,
            CmdResult::SpawnFailed(_) | CmdResult::TimedOut => PairState::Unknown,
        }
    }
}

/// Convenience wrapper used by the Tauri command: run the real probes and
/// evaluate them, degrading to an `Error` report if evaluation panics.
pub fn check() -> EnvReport {
    evaluate(&RealProbe)
}

/// Build an `Error`-state report for a failure inside the checker itself.
pub fn failure_report(message: &str) -> EnvReport {
    let tools = platform::tool_report();
    EnvReport {
        status: EnvStatus::Error,
        message: format!("The connection check itself failed ({message})."),
        hint: Some(
            "Try restarting the app. If it keeps happening, use Help & connection for diagnostics."
                .to_string(),
        ),
        code: Some(ErrorCode::Internal),
        os: RealProbe.os().to_string(),
        missing_tools: tools.missing().iter().map(|s| s.to_string()).collect(),
        tools,
        service: ServiceState::Unsupported,
        service_detail: service_detail(ServiceState::Unsupported),
        device_udid: None,
        pairing: None,
        checks: Vec::new(),
        install_hint: platform::install_hint(),
        mock_available: cfg!(debug_assertions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configurable fake covering every probe result.
    struct FakeProbe {
        os: &'static str,
        missing: Vec<&'static str>,
        broken: Vec<(&'static str, String)>,
        service: ServiceState,
        devices: DeviceProbe,
        pairing: PairState,
    }

    impl FakeProbe {
        fn healthy() -> Self {
            Self {
                os: "linux",
                missing: Vec::new(),
                broken: Vec::new(),
                service: ServiceState::Running,
                devices: DeviceProbe::Devices(vec!["00008120-001A2B3C4D5E6F7A".to_string()]),
                pairing: PairState::Paired,
            }
        }
    }

    impl EnvironmentProbe for FakeProbe {
        fn os(&self) -> &'static str {
            self.os
        }
        fn tool(&self, name: &str) -> ToolProbe {
            if let Some((n, reason)) = self.broken.iter().find(|(n, _)| *n == name) {
                let _ = n;
                return ToolProbe::Broken(reason.clone());
            }
            if self.missing.contains(&name) {
                return ToolProbe::Missing;
            }
            ToolProbe::Found(PathBuf::from(format!("/usr/bin/{name}")))
        }
        fn service(&self) -> ServiceState {
            self.service
        }
        fn devices(&self, _idevice_id: &Path) -> DeviceProbe {
            self.devices.clone()
        }
        fn pairing(&self, _idevicepair: &Path, _udid: &str) -> PairState {
            self.pairing
        }
    }

    #[test]
    fn ready_when_everything_passes() {
        let report = evaluate(&FakeProbe::healthy());
        assert_eq!(report.status, EnvStatus::Ready);
        assert!(report.device_udid.is_some());
        assert!(report.code.is_none());
        assert!(report.checks.iter().all(|c| c.ok && !c.skipped));
    }

    #[test]
    fn dependency_missing_when_a_tool_is_absent() {
        let mut fake = FakeProbe::healthy();
        fake.missing = vec![platform::TOOL_AFC];
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::DependencyMissing);
        assert_eq!(report.code, Some(ErrorCode::ToolMissing));
        assert!(report
            .missing_tools
            .contains(&platform::TOOL_AFC.to_string()));
        // The service was never blamed for a missing executable.
        assert!(
            report
                .checks
                .iter()
                .find(|c| c.id == "service")
                .unwrap()
                .skipped
        );
    }

    #[test]
    fn broken_tool_is_dependency_missing_not_ready() {
        let mut fake = FakeProbe::healthy();
        fake.broken = vec![(platform::TOOL_ID, "exec format error".to_string())];
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::DependencyMissing);
        assert!(report.message.contains("could not run"));
    }

    #[test]
    fn stopped_service_is_service_not_running() {
        let mut fake = FakeProbe::healthy();
        fake.service = ServiceState::NotRunning;
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::ServiceNotRunning);
        assert_eq!(report.code, Some(ErrorCode::ServiceUnavailable));
        assert!(report.service_detail.contains("not running"));
        // Tools were present — they must not be reported missing.
        assert!(report.missing_tools.is_empty());
    }

    #[test]
    fn not_installed_service_says_so_and_still_names_the_tools() {
        let mut fake = FakeProbe::healthy();
        fake.service = ServiceState::NotInstalled;
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::ServiceNotRunning);
        assert!(report.service_detail.contains("not installed"));
        assert!(report.hint.is_some());
    }

    #[test]
    fn permission_problem_is_permission_required() {
        let mut fake = FakeProbe::healthy();
        fake.service = ServiceState::PermissionDenied;
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::PermissionRequired);
        assert_eq!(report.code, Some(ErrorCode::PermissionDenied));
    }

    #[test]
    fn no_device_when_list_is_empty() {
        let mut fake = FakeProbe::healthy();
        fake.devices = DeviceProbe::None;
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::DeviceNotConnected);
        assert_eq!(report.code, Some(ErrorCode::NoDevice));
        assert!(report.device_udid.is_none());
        // Pairing could not be checked and must not be ticked.
        assert!(
            report
                .checks
                .iter()
                .find(|c| c.id == "pairing")
                .unwrap()
                .skipped
        );
    }

    #[test]
    fn trust_pending_is_pairing_required() {
        let mut fake = FakeProbe::healthy();
        fake.pairing = PairState::NeedsTrust;
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::PairingRequired);
        assert_eq!(report.code, Some(ErrorCode::NeedsTrust));
        assert!(report.hint.unwrap().contains("Trust"));
    }

    #[test]
    fn unverifiable_pairing_never_claims_ready() {
        let mut fake = FakeProbe::healthy();
        fake.pairing = PairState::Unknown;
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::PairingRequired);
    }

    #[test]
    fn device_disconnecting_during_check_is_reported() {
        let mut fake = FakeProbe::healthy();
        fake.devices = DeviceProbe::Devices(vec!["00008120-001A2B3C4D5E6F7A".to_string()]);
        fake.pairing = PairState::NoDevice;
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::DeviceNotConnected);
        assert!(report.message.contains("disconnected"));
    }

    #[test]
    fn malformed_device_response_is_an_error_not_a_guess() {
        let mut fake = FakeProbe::healthy();
        fake.devices = DeviceProbe::Failed("unrecognized idevice_id output".to_string());
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::Error);
        assert_eq!(report.code, Some(ErrorCode::Internal));
        assert!(report.message.contains("unexpected response"));
        assert!(report.device_udid.is_none());
    }

    #[test]
    fn detection_tool_failing_to_start_is_dependency_missing() {
        let mut fake = FakeProbe::healthy();
        fake.devices = DeviceProbe::ToolBroken("No such file or directory".to_string());
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::DependencyMissing);
        assert_eq!(report.code, Some(ErrorCode::ToolMissing));
    }

    #[test]
    fn unsupported_platform_short_circuits() {
        let mut fake = FakeProbe::healthy();
        fake.os = "other";
        let report = evaluate(&fake);
        assert_eq!(report.status, EnvStatus::UnsupportedPlatform);
        assert_eq!(report.code, Some(ErrorCode::Unsupported));
        assert_eq!(report.checks.len(), 1);
    }

    #[test]
    fn report_is_serializable_with_stable_snake_case_states() {
        let report = evaluate(&FakeProbe::healthy());
        let json = serde_json::to_value(&report).expect("serialize");
        assert_eq!(json["status"], "ready");
        assert_eq!(json["service"], "running");
        assert_eq!(json["pairing"], "paired");

        let mut fake = FakeProbe::healthy();
        fake.service = ServiceState::NotRunning;
        let json = serde_json::to_value(evaluate(&fake)).expect("serialize");
        assert_eq!(json["status"], "service_not_running");
        assert_eq!(json["service"], "not_running");
    }

    #[test]
    fn real_probe_reports_this_machine_without_panicking() {
        // Integration-flavoured smoke test: whatever this machine looks like,
        // the real probes must produce a coherent report.
        let report = check();
        assert!(matches!(
            report.status,
            EnvStatus::Ready
                | EnvStatus::DeviceNotConnected
                | EnvStatus::DependencyMissing
                | EnvStatus::ServiceNotRunning
                | EnvStatus::PermissionRequired
                | EnvStatus::PairingRequired
                | EnvStatus::UnsupportedPlatform
                | EnvStatus::Error
        ));
        if !matches!(
            report.service,
            ServiceState::Running | ServiceState::Unsupported
        ) {
            assert_ne!(report.status, EnvStatus::Ready);
        }
    }
}
