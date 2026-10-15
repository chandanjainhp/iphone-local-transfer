//! Platform-specific discovery: locate the libimobiledevice toolchain without
//! shelling out, and explain — per OS — what is missing.

use serde::Serialize;
use std::path::{Path, PathBuf};

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

/// True when `path` is a regular file we could plausibly execute.
///
/// On Unix at least one execute bit must be set, so a non-executable stray file
/// cannot shadow a valid tool later in the search order. On Windows the
/// `.exe` suffix already decided it.
fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

/// Candidate directories for the toolchain, most trusted first.
///
/// PATH and the usual Homebrew prefixes come first, then a `tools/` folder
/// bundled next to the executable, then the executable's own directory (common
/// for Windows zip installs). The directory next to the executable can be
/// user-writable, so it must never take precedence over the tool the user
/// actually installed — but it is still honoured as a fallback, which keeps
/// self-contained installs working.
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(paths) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&paths));
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.join("tools"));
            dirs.push(dir.to_path_buf());
        }
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
        if is_executable(&candidate) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lit-platform-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn non_executable_files_are_not_usable_tools() {
        let dir = scratch("mode");
        let plain = dir.join("idevice_id");
        std::fs::write(&plain, b"#!/bin/sh\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // 0644: readable but not executable — must not be accepted.
            std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(
                !is_executable(&plain),
                "a non-executable file is not a tool"
            );

            std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(is_executable(&plain), "an executable file is accepted");
        }
        #[cfg(not(unix))]
        {
            assert!(is_executable(&plain));
        }

        // Directories and missing paths are never tools.
        assert!(!is_executable(&dir));
        assert!(!is_executable(&dir.join("does-not-exist")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The directory beside the executable can be user-writable (zip installs,
    /// extracted AppImages), so the user's PATH must be searched first.
    #[test]
    fn path_is_searched_before_the_executable_directory() {
        let dirs = candidate_dirs();
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf));
        let Some(exe_dir) = exe_dir else {
            return; // no executable path in this environment
        };
        let tools_dir = exe_dir.join("tools");
        let bundled_pos = dirs.iter().position(|d| *d == tools_dir || *d == exe_dir);
        let path_entries = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).count())
            .unwrap_or(0);
        if let Some(pos) = bundled_pos {
            assert!(
                pos >= path_entries,
                "bundled tools dir ({pos}) must not precede the {path_entries} PATH entries"
            );
        }
    }

    #[test]
    fn a_bundled_fallback_is_still_available() {
        let dirs = candidate_dirs();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                assert!(
                    dirs.contains(&dir.join("tools")) || dirs.contains(&dir.to_path_buf()),
                    "the bundled tools directory stays as a fallback"
                );
            }
        }
    }
}
