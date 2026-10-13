//! Device model and the narrow provider seam.
//!
//! `DeviceProvider` (detection) and `MediaSource` (listing + byte access) are
//! separate traits so tests can fake either half, but a real adapter
//! implements both. Platform-specific USB details never cross this boundary —
//! only `DeviceSnapshot`, `MediaItem` and `AppError` do.

use crate::errors::AppError;
use crate::media::MediaItem;
use crate::preview::PreviewImage;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    /// Nothing is attached over USB.
    NoDevice,
    /// A phone is attached but locked or waiting for the Trust dialog.
    NeedsTrust,
    /// Attached, trusted and reachable — safe to scan.
    Ready,
    /// A device is attached but the access path is not supported here.
    Unsupported,
    /// Native tooling/service failure; see `message`.
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    /// Device identifier as reported by usbmuxd (used for `-u` targeting).
    pub udid: String,
    pub name: Option<String>,
    /// Hardware model, e.g. `iPhone14,2`.
    pub model: Option<String>,
    /// iOS version, e.g. `17.5.1`.
    pub ios_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceSnapshot {
    pub state: DeviceState,
    pub device: Option<DeviceInfo>,
    /// Short, user-facing status sentence.
    pub message: String,
    /// Optional longer guidance ("Unlock your iPhone and tap Trust…").
    pub hint: Option<String>,
    /// `true` when this snapshot comes from the development-only mock adapter.
    pub demo: bool,
    /// Which adapter produced this snapshot (`libimobiledevice` / `mock`).
    pub source: String,
}

impl DeviceSnapshot {
    /// Is the phone attached but unusable right now?
    pub fn is_actionable(&self) -> bool {
        matches!(self.state, DeviceState::Ready)
    }
}

/// Detection seam: connected / trusted / unavailable — nothing else.
pub trait DeviceProvider: Send + Sync {
    fn label(&self) -> &'static str;
    fn detect(&self) -> Result<DeviceSnapshot, AppError>;
    fn media(&self) -> &dyn MediaSource;
}

/// Media seam: enumerate what iOS actually exposes, and read bytes out of it.
///
/// `copy_to` streams into `dest_file` (a temporary path chosen by the caller),
/// reports cumulative bytes through `progress`, and honors `cancel`.
pub trait MediaSource: Send + Sync {
    fn list_media(&self, udid: &str) -> Result<Vec<MediaItem>, AppError>;

    /// Best-effort preview thumbnail for a listed item, as raw image bytes.
    ///
    /// `Ok(None)` means the source exposes no preview for this item — the UI
    /// falls back to an icon placeholder. Implementations must never invent
    /// an image. `source_path` is the device path of an already-listed item;
    /// implementations must still validate it before use.
    fn preview(&self, _udid: &str, _source_path: &str) -> Result<Option<PreviewImage>, AppError> {
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
    ) -> Result<u64, AppError>;
}
