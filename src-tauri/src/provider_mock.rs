//! Development-only mock adapter.
//!
//! Compiled **only into debug builds** (`cfg(debug_assertions)`), activated
//! only through an explicit `set_mock_mode` command, and every snapshot it
//! produces carries `demo: true` so the UI can label it. It must never be
//! mistaken for — or reported as — a real device.

use crate::device::{DeviceInfo, DeviceProvider, DeviceSnapshot, DeviceState, MediaSource};
use crate::errors::{AppError, ErrorCode};
use crate::media::{MediaItem, MediaKind};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Mock device lifecycle, switchable from the UI in development builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockDeviceState {
    NoDevice,
    NeedsTrust,
    Ready,
}

pub struct MockProvider {
    pub state: MockDeviceState,
}

const DEMO_FILES: &[(&str, MediaKind, u64, i64)] = &[
    (
        "IMG_0001.HEIC",
        MediaKind::Photo,
        3_145_728,
        1_735_689_600_000,
    ),
    (
        "IMG_0002.HEIC",
        MediaKind::Photo,
        4_194_304,
        1_735_776_000_000,
    ),
    (
        "IMG_0003.JPG",
        MediaKind::Photo,
        2_097_152,
        1_735_862_400_000,
    ),
    (
        "IMG_0004.MOV",
        MediaKind::Video,
        41_943_040,
        1_735_948_800_000,
    ),
    (
        "IMG_0005.MOV",
        MediaKind::Video,
        94_371_840,
        1_736_035_200_000,
    ),
    (
        "IMG_0006.PNG",
        MediaKind::Photo,
        1_048_576,
        1_736_121_600_000,
    ),
];

pub const DEMO_UDID: &str = "DEMO-00000000000000000000000000000001";

fn demo() -> DeviceSnapshot {
    DeviceSnapshot {
        state: DeviceState::Ready,
        device: Some(DeviceInfo {
            udid: DEMO_UDID.to_string(),
            name: Some("Demo iPhone".to_string()),
            model: Some("iPhone15,2".to_string()),
            ios_version: Some("17.5".to_string()),
        }),
        message: "Demo iPhone is ready (DEMO DATA)".to_string(),
        hint: None,
        demo: true,
        source: "mock (demo only)".to_string(),
    }
}

impl DeviceProvider for MockProvider {
    fn label(&self) -> &'static str {
        "mock (demo only)"
    }

    fn detect(&self) -> Result<DeviceSnapshot, AppError> {
        let mut snap = match self.state {
            MockDeviceState::NoDevice => DeviceSnapshot {
                state: DeviceState::NoDevice,
                device: None,
                message: "No iPhone detected (demo)".to_string(),
                hint: Some("This is a demo state.".to_string()),
                demo: true,
                source: "mock (demo only)".to_string(),
            },
            MockDeviceState::NeedsTrust => DeviceSnapshot {
                state: DeviceState::NeedsTrust,
                device: None,
                message: "Waiting for Trust (demo)".to_string(),
                hint: Some("This is a demo state.".to_string()),
                demo: true,
                source: "mock (demo only)".to_string(),
            },
            MockDeviceState::Ready => demo(),
        };
        snap.demo = true;
        snap.source = "mock (demo only)".to_string();
        Ok(snap)
    }

    fn media(&self) -> &dyn MediaSource {
        self
    }
}

impl MediaSource for MockProvider {
    fn list_media(&self, _udid: &str) -> Result<Vec<MediaItem>, AppError> {
        if self.state != MockDeviceState::Ready {
            return Err(AppError::new(
                ErrorCode::NeedsTrust,
                "The demo device is not ready.",
            ));
        }
        Ok(DEMO_FILES
            .iter()
            .map(|(name, kind, size, modified_ms)| MediaItem {
                id: format!("/DCIM/100APPLE/{name}"),
                name: (*name).to_string(),
                kind: *kind,
                size: Some(*size),
                modified_ms: Some(*modified_ms),
                source_path: format!("/DCIM/100APPLE/{name}"),
            })
            .collect())
    }

    fn copy_to(
        &self,
        _udid: &str,
        source_path: &str,
        dest_file: &Path,
        expected_size: Option<u64>,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64),
    ) -> Result<u64, AppError> {
        let item = DEMO_FILES
            .iter()
            .find(|(name, ..)| source_path.ends_with(name))
            .ok_or_else(|| AppError::new(ErrorCode::TransferFailed, "Unknown demo file."))?;
        let total = item.2;

        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dest_file)
            .map_err(AppError::from)?;

        const CHUNK: u64 = 1024 * 1024;
        let mut written: u64 = 0;
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        progress(0);
        while written < total {
            if cancel.load(Ordering::SeqCst) {
                return Err(AppError::new(
                    ErrorCode::Cancelled,
                    "The transfer was cancelled.",
                ));
            }
            let n = CHUNK.min(total - written);
            let mut chunk = vec![0u8; n as usize];
            for b in chunk.iter_mut() {
                // Cheap deterministic filler — no real device data involved.
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                *b = (seed >> 33) as u8;
            }
            file.write_all(&chunk).map_err(AppError::from)?;
            written += n;
            progress(written);
            // Simulate a USB-ish pace so the demo progress UI is visible.
            std::thread::sleep(Duration::from_millis(40));
        }
        file.sync_all().map_err(AppError::from)?;
        drop(file);
        if let Some(expected) = expected_size {
            if written != expected {
                return Err(AppError::new(
                    ErrorCode::TransferFailed,
                    "Demo copy size mismatch.",
                ));
            }
        }
        Ok(written)
    }
}
