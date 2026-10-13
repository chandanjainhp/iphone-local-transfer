//! Media model: what iOS exposes over the supported AFC media service.

use crate::errors::{AppError, ErrorCode};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Photo,
    Video,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaItem {
    /// Stable identifier = the device-relative AFC path (also used as UI key).
    pub id: String,
    /// Basename exactly as it appears on the device (untrusted input).
    pub name: String,
    pub kind: MediaKind,
    /// `None` when the source does not expose a size. Never invented.
    pub size: Option<u64>,
    /// Epoch milliseconds when the source exposes a modification date.
    pub modified_ms: Option<i64>,
    /// AFC path inside the media area, always rooted at /DCIM/….
    pub source_path: String,
}

const PHOTO_EXTS: &[&str] = &[
    "heic", "heif", "jpg", "jpeg", "png", "gif", "tiff", "tif", "dng", "bmp",
];
const VIDEO_EXTS: &[&str] = &["mov", "mp4", "m4v", "3gp", "3g2", "m4v", "avi", "mkv"];

/// Classify by extension. Returns `None` for files that are neither photos nor
/// videos (thumbnails, .aae sidecars, database files, …).
pub fn kind_for_name(name: &str) -> Option<MediaKind> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    if ext == name.to_ascii_lowercase() {
        // No dot at all.
        return None;
    }
    if PHOTO_EXTS.contains(&ext.as_str()) {
        return Some(MediaKind::Photo);
    }
    if VIDEO_EXTS.contains(&ext.as_str()) {
        return Some(MediaKind::Video);
    }
    None
}

/// Validate a source path coming back from the UI before it reaches a child
/// process: must live under /DCIM, no traversal, no control characters.
pub fn validate_source_path(path: &str) -> Result<(), AppError> {
    let ok = path.starts_with("/DCIM/")
        && !path.contains("..")
        && !path.contains('\0')
        && !path.chars().any(|c| c.is_control())
        && path.len() > "/DCIM/".len();
    if ok {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::MediaUnavailable,
            "That file is not part of the accessible photo library.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_photos_and_videos() {
        assert_eq!(kind_for_name("IMG_0001.HEIC"), Some(MediaKind::Photo));
        assert_eq!(kind_for_name("IMG_0002.jpg"), Some(MediaKind::Photo));
        assert_eq!(kind_for_name("IMG_0003.MOV"), Some(MediaKind::Video));
        assert_eq!(kind_for_name("clip.mp4"), Some(MediaKind::Video));
    }

    #[test]
    fn rejects_non_media() {
        assert_eq!(kind_for_name("IMG_0004.AAE"), None);
        assert_eq!(kind_for_name("Thumbs.db"), None);
        assert_eq!(kind_for_name("README"), None);
    }

    #[test]
    fn source_paths_must_stay_under_dcim() {
        assert!(validate_source_path("/DCIM/100APPLE/IMG_0001.HEIC").is_ok());
        assert!(validate_source_path("/DCIM/../etc/passwd").is_err());
        assert!(validate_source_path("/private/var/mobile/Media/x").is_err());
        assert!(validate_source_path("/DCIM/").is_err());
        assert!(validate_source_path("/DCIM/a\nb").is_err());
    }
}
