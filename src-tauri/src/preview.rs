//! Preview (thumbnail) support.
//!
//! iOS generates thumbnails for the photo library under
//! `/PhotoData/Thumbnails`, keyed by the media's path inside the library:
//!
//! * `/PhotoData/Thumbnails/V2/DCIM/103APPLE/IMG_3794.HEIC/5005.JPG` — photos
//!   and most videos (~100–150 KB JPEGs, verified on a physical device).
//! * `/PhotoData/Thumbnails/VideoKeyFrames/DCIM/103APPLE/IMG_3562.MOV/LocalVideoKeyFrame.jpg`
//!   — still frames for videos without a V2 entry.
//!
//! Candidate paths are derived from a *validated* `/DCIM/…` source path only,
//! so a preview can never be requested outside the accessible library. When no
//! preview exists we return `None`; the UI shows an icon placeholder instead
//! of an invented image.

use crate::errors::AppError;
use crate::media::validate_source_path;
use std::collections::{HashMap, VecDeque};

/// Hard cap on a single preview payload. Previews are thumbnails; anything
/// larger is not worth the memory or the transfer time.
pub const MAX_PREVIEW_BYTES: usize = 8 * 1024 * 1024;

/// A decoded-and-validated preview image ready to be served to the UI.
pub struct PreviewImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

/// Device directories that may hold a preview for `source_path`, most
/// preferred first. Fails when the path is not an allowed library path.
pub fn preview_dirs(source_path: &str) -> Result<Vec<String>, AppError> {
    validate_source_path(source_path)?;
    Ok(vec![
        format!("/PhotoData/Thumbnails/V2{source_path}"),
        format!("/PhotoData/Thumbnails/VideoKeyFrames{source_path}"),
    ])
}

/// Extensions we are willing to read as a preview thumbnail.
pub fn is_preview_file_name(name: &str) -> bool {
    if name.is_empty()
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.chars().any(|c| c.is_control())
    {
        return false;
    }
    let Some(ext) = name.rsplit('.').next() else {
        return false;
    };
    if ext.len() == name.len() {
        // No dot at all.
        return false;
    }
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp"
    )
}

/// Identify a browser-renderable image from its magic bytes. Returns `None`
/// for anything we cannot vouch for (never guesses from a file name).
pub fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some("image/jpeg");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Standard base64 with padding (RFC 4648 §4). Hand-rolled to keep the
/// dependency tree empty; covered by the RFC test vectors below.
pub fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// `data:` URL for a validated preview image.
pub fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", base64_encode(bytes))
}

/// Turn raw preview bytes into a `PreviewImage`, enforcing the size cap and
/// requiring magic bytes we recognize. `None` = no usable preview.
pub fn prepare_preview(bytes: Vec<u8>) -> Option<PreviewImage> {
    if bytes.is_empty() || bytes.len() > MAX_PREVIEW_BYTES {
        return None;
    }
    let mime = sniff_image_mime(&bytes)?;
    Some(PreviewImage { bytes, mime })
}

/// Byte-bounded FIFO cache of encoded previews (`key` → data URL).
/// Keeps scrolling through a large library from growing memory without limit.
pub struct PreviewCache {
    entries: HashMap<String, String>,
    order: VecDeque<String>,
    bytes: usize,
}

impl PreviewCache {
    /// Total size of all cached data URLs. One entry can be at most
    /// `MAX_PREVIEW_BYTES` base64-encoded (~10.7 MB), so two of them can
    /// never exceed this budget twice over.
    pub const MAX_BYTES: usize = 16 * 1024 * 1024;

    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    pub fn insert(&mut self, key: String, value: String) {
        if self.entries.contains_key(&key) {
            return;
        }
        self.bytes = self.bytes.saturating_add(value.len());
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
        while self.bytes > Self::MAX_BYTES {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(v) = self.entries.remove(&old) {
                self.bytes = self.bytes.saturating_sub(v.len());
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_thumbnail_dirs_under_photo_data() {
        let dirs = preview_dirs("/DCIM/103APPLE/IMG_3794.HEIC").expect("valid");
        assert_eq!(
            dirs,
            vec![
                "/PhotoData/Thumbnails/V2/DCIM/103APPLE/IMG_3794.HEIC".to_string(),
                "/PhotoData/Thumbnails/VideoKeyFrames/DCIM/103APPLE/IMG_3794.HEIC".to_string(),
            ]
        );
    }

    #[test]
    fn rejects_paths_outside_the_library() {
        assert!(preview_dirs("/DCIM/../../etc/passwd").is_err());
        assert!(preview_dirs("/PhotoData/Photos.sqlite").is_err());
        assert!(preview_dirs("/DCIM/").is_err());
        assert!(preview_dirs("/DCIM/a\nb").is_err());
    }

    #[test]
    fn filters_preview_file_names() {
        assert!(is_preview_file_name("5005.JPG"));
        assert!(is_preview_file_name("LocalVideoKeyFrame.jpg"));
        assert!(is_preview_file_name("thumb.png"));
        assert!(!is_preview_file_name("thumbnailConfiguration"));
        assert!(!is_preview_file_name(".hidden.jpg"));
        assert!(!is_preview_file_name("../escape.jpg"));
        assert!(!is_preview_file_name("sub/thumb.jpg"));
        assert!(!is_preview_file_name("evil\r\n.jpg"));
        assert!(!is_preview_file_name(""));
    }

    #[test]
    fn sniffs_supported_images() {
        assert_eq!(
            sniff_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("image/jpeg")
        );
        assert_eq!(
            sniff_image_mime(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
            Some("image/png")
        );
        assert_eq!(sniff_image_mime(b"GIF89a.."), Some("image/gif"));
        assert_eq!(
            sniff_image_mime(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(sniff_image_mime(b"not an image"), None);
        assert_eq!(sniff_image_mime(b""), None);
        assert_eq!(sniff_image_mime(&[0xFF]), None);
    }

    #[test]
    fn prepare_enforces_cap_and_magic() {
        assert!(prepare_preview(Vec::new()).is_none());
        let mut huge = vec![0xFF, 0xD8, 0xFF];
        huge.resize(MAX_PREVIEW_BYTES + 1, 0);
        assert!(prepare_preview(huge).is_none());
        // Right size, but not an image we can render.
        assert!(prepare_preview(vec![b'H', b'i', b'!']).is_none());
        let ok = prepare_preview(vec![0xFF, 0xD8, 0xFF, 0x00]);
        assert!(ok.is_some());
        assert_eq!(ok.unwrap().mime, "image/jpeg");
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        // 0x00 and 0xFF round-trip through the alphabet correctly.
        assert_eq!(base64_encode(&[0x00, 0xFF, 0x10]), "AP8Q");
    }

    #[test]
    fn data_url_has_expected_shape() {
        assert_eq!(
            data_url(&[0xFF, 0xD8], "image/jpeg"),
            "data:image/jpeg;base64,/9g="
        );
    }

    #[test]
    fn cache_evicts_oldest_when_over_budget() {
        let mut cache = PreviewCache::new();
        let big = "x".repeat(PreviewCache::MAX_BYTES / 2 + 1);
        cache.insert("a".into(), big.clone());
        cache.insert("b".into(), big.clone());
        // Two entries over budget: the oldest ("a") must go first.
        assert_eq!(cache.len(), 1);
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        // Small entries stay put.
        cache.insert("c".into(), "tiny".into());
        cache.insert("d".into(), "tiny".into());
        assert!(cache.get("b").is_some());
        assert_eq!(cache.len(), 3);
        // Re-inserting an existing key is a no-op, not a duplicate.
        cache.insert("c".into(), "tiny".into());
        assert_eq!(cache.len(), 3);
    }
}
