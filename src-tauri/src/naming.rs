//! Destination filename policy: sanitize untrusted names and resolve conflicts
//! without ever silently overwriting.

use std::path::Path;

const RESERVED_WINDOWS_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Turn an untrusted device filename into something safe on Windows, macOS and
/// Linux: no path separators, no traversal, no control characters, no Windows-
/// reserved characters or device names, never empty, and bounded in length.
pub fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => out.push('_'),
            c if c.is_control() => out.push('_'),
            c => out.push(c),
        }
    }
    // Remove traversal markers outright; separators are already underscores,
    // so ".." can never act as a directory reference here.
    while out.contains("..") {
        out = out.replace("..", "");
    }
    let trimmed = out
        .trim_matches(|c| c == '.' || c == ' ' || c == '_')
        .to_string();
    let mut base = if trimmed.is_empty() {
        "Untitled".to_string()
    } else {
        trimmed
    };
    // Windows reserved device names (with or without extension).
    let stem = base.split('.').next().unwrap_or("").to_ascii_lowercase();
    if RESERVED_WINDOWS_NAMES.contains(&stem.as_str()) {
        base = format!("_{base}");
    }
    // Keep well under common 255-byte limits while preserving the extension.
    const MAX_BYTES: usize = 200;
    if base.len() > MAX_BYTES {
        let (stem, ext) = split_ext(&base);
        let mut stem = stem.to_string();
        let budget = MAX_BYTES.saturating_sub(ext.len());
        while stem.len() > budget {
            stem.pop();
        }
        base = format!("{stem}{ext}");
    }
    base
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Skip the file, keep the existing one untouched (safe default).
    Skip,
    /// Save as `name (1).ext`, `name (2).ext`, …
    Rename,
    /// Overwrite an existing file with the same name. Only reachable after the
    /// UI has collected an explicit confirmation for this transfer — the
    /// backend still publishes through the same temporary-file path, so a
    /// failed copy never leaves a half-replaced file behind.
    Replace,
}

/// Resolve `dest/name` against an already-existing file. Returns `None` when
/// the policy says skip. Pure name construction is exposed separately so it can
/// be unit-tested without touching the filesystem.
pub fn resolve_conflict(dest: &Path, name: &str, policy: ConflictPolicy) -> Option<String> {
    match policy {
        ConflictPolicy::Skip => {
            if dest.join(name).exists() {
                None
            } else {
                Some(name.to_string())
            }
        }
        // The target may exist; the transfer engine replaces it atomically
        // only after the copy has fully succeeded.
        ConflictPolicy::Rename => Some(next_available_name(dest, name)),
        ConflictPolicy::Replace => Some(name.to_string()),
    }
}

/// `photo.HEIC` -> `photo (1).HEIC` -> `photo (2).HEIC` …
pub fn next_available_name(dest: &Path, name: &str) -> String {
    if !dest.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    for i in 1..10_000 {
        let candidate = format!("{stem} ({i}){ext}");
        if !dest.join(&candidate).exists() {
            return candidate;
        }
    }
    // Practically unreachable; keeps the function total.
    format!("{stem} ({}){ext}", std::process::id())
}

/// Temporary sibling used during a copy: never presented as a real file, and
/// always cleaned up on failure. Leading dot keeps it out of casual listings.
pub fn temporary_name(final_name: &str) -> String {
    format!(".{}.part-{}", final_name, std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_traversal_and_illegal_chars() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "etc_passwd");
        assert_eq!(sanitize_filename("IMG:0001?.HEIC"), "IMG_0001_.HEIC");
        assert_eq!(sanitize_filename("a\u{7}b.txt"), "a_b.txt");
        assert_eq!(sanitize_filename("..."), "Untitled");
        assert_eq!(sanitize_filename(""), "Untitled");
        assert_eq!(sanitize_filename("photo.mov"), "photo.mov");
    }

    #[test]
    fn protects_windows_reserved_names() {
        assert_eq!(sanitize_filename("CON.jpg"), "_CON.jpg");
        assert_eq!(sanitize_filename("nul"), "_nul");
    }

    #[test]
    fn bounds_length_but_keeps_extension() {
        let long = format!("{}.HEIC", "x".repeat(500));
        let safe = sanitize_filename(&long);
        assert!(safe.len() <= 200);
        assert!(safe.ends_with(".HEIC"));
    }

    #[test]
    fn conflict_policies() {
        let dir = std::env::temp_dir().join(format!("lit-naming-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jpg"), b"old").unwrap();

        assert_eq!(
            resolve_conflict(&dir, "b.jpg", ConflictPolicy::Skip),
            Some("b.jpg".to_string())
        );
        assert_eq!(resolve_conflict(&dir, "a.jpg", ConflictPolicy::Skip), None);
        assert_eq!(
            resolve_conflict(&dir, "a.jpg", ConflictPolicy::Rename),
            Some("a (1).jpg".to_string())
        );

        std::fs::write(dir.join("a (1).jpg"), b"old2").unwrap();
        assert_eq!(next_available_name(&dir, "a.jpg"), "a (2).jpg");
        assert_eq!(next_available_name(&dir, "nope.jpg"), "nope.jpg");

        // Replace targets the existing name (confirmed by the UI first).
        assert_eq!(
            resolve_conflict(&dir, "a.jpg", ConflictPolicy::Replace),
            Some("a.jpg".to_string())
        );
        assert_eq!(
            resolve_conflict(&dir, "fresh.jpg", ConflictPolicy::Replace),
            Some("fresh.jpg".to_string())
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
