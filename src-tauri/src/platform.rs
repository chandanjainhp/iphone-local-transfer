//! Platform-specific discovery: locate the libimobiledevice toolchain without
//! shelling out, and explain — per OS — what is missing.

use serde::Serialize;
use std::path::PathBuf;

pub const TOOL_ID: &str = "idevice_id";
pub const TOOL_INFO: &str = "ideviceinfo";
pub const TOOL_PAIR: &str = "idevicepair";
pub const TOOL_AFC: &str = "afcclient";

#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolReport {
    /// Absolute path when found; `None` when not on PATH.
    pub idevice_id: Option<String>,
    pub ideviceinfo: Option<String>,
    pub idevicepair: Option<String>,
    pub afcclient: Option<String>,
}

impl ToolReport {
    pub fn all_found(&self) -> bool {
        self.idevice_id.is_some()
            && self.ideviceinfo.is_some()
            && self.idevicepair.is_some()
            && self.afcclient.is_some()
    }

    pub fn missing(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.idevice_id.is_none() {
            v.push(TOOL_ID);
        }
        if self.ideviceinfo.is_none() {
            v.push(TOOL_INFO);
        }
        if self.idevicepair.is_none() {
            v.push(TOOL_PAIR);
        }
        if self.afcclient.is_none() {
            v.push(TOOL_AFC);
        }
        v
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    /// `linux` / `macos` / `windows` / other.
    pub os: &'static str,
    pub tools: ToolReport,
    /// Tools that could not be located, by binary name.
    pub missing_tools: Vec<&'static str>,
    /// `true` when the full toolchain is present.
    pub toolchain_ready: bool,
    /// Human-readable, per-OS guidance for installing the toolchain.
    pub install_hint: String,
    /// The mock adapter is compiled in development builds only.
    pub mock_available: bool,
}

/// Candidate directories for the toolchain, most specific first:
/// a `tools/` folder bundled next to the executable, the executable's own
/// directory (common for Windows zip installs), then PATH, then the usual
/// Homebrew prefixes on macOS.
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("tools"));
            dirs.push(dir.to_path_buf());
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&paths));
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs
}

/// Resolve a tool name to an executable path, or `None`.
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let exe_ext = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    for dir in candidate_dirs() {
        let candidate = dir.join(format!("{name}{exe_ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn tool_report() -> ToolReport {
    ToolReport {
        idevice_id: find_tool(TOOL_ID).map(|p| p.display().to_string()),
        ideviceinfo: find_tool(TOOL_INFO).map(|p| p.display().to_string()),
        idevicepair: find_tool(TOOL_PAIR).map(|p| p.display().to_string()),
        afcclient: find_tool(TOOL_AFC).map(|p| p.display().to_string()),
    }
}

pub fn install_hint() -> String {
    if cfg!(target_os = "macos") {
        "Install the tools with Homebrew: brew install libimobiledevice. \
             Also make sure your iPhone is unlocked and trusted by this Mac."
            .to_string()
    } else if cfg!(target_os = "windows") {
        "Windows support is experimental and not yet validated on a physical \
             iPhone. Install iTunes (Apple Mobile Device Support) plus a \
             libimobiledevice build that includes afcclient, and add its folder \
             to PATH."
            .to_string()
    } else {
        "Install libimobiledevice with your package manager, for example: \
             sudo pacman -S libimobiledevice (Arch), \
             sudo apt install libimobiledevice-utils (Debian/Ubuntu), \
             sudo dnf install libimobiledevice (Fedora). \
             Also make sure usbmuxd is running."
            .to_string()
    }
}

pub fn diagnostics() -> Diagnostics {
    let tools = tool_report();
    let missing_tools = tools.missing();
    let toolchain_ready = tools.all_found();
    Diagnostics {
        os: if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "other"
        },
        tools,
        missing_tools,
        toolchain_ready,
        install_hint: install_hint(),
        mock_available: cfg!(debug_assertions),
    }
}
