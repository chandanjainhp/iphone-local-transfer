//! Stable, UI-safe error codes plus user-friendly messages.
//!
//! Every error that can reach the frontend serializes to `{ code, message }`.
//! `code` is stable (used by the UI for branching); `message` is written for a
//! non-technical reader and must never contain file contents or device secrets.

use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// No iPhone is attached over USB.
    NoDevice,
    /// The phone is locked, or the Trust dialog has not been accepted yet.
    NeedsTrust,
    /// libimobiledevice / usbmuxd are not installed or not running.
    ServiceUnavailable,
    /// A required command-line tool could not be found on this machine.
    ToolMissing,
    /// The device does not expose the requested media over the supported service.
    MediaUnavailable,
    /// The chosen destination folder is missing, not a folder, or not writable.
    DestinationInvalid,
    /// The destination volume does not have enough free space.
    OutOfSpace,
    /// The OS refused the file operation (permissions / read-only volume).
    PermissionDenied,
    /// The user cancelled the transfer.
    Cancelled,
    /// The USB connection dropped (or the transfer stalled waiting for data).
    DeviceDisconnected,
    /// A single file failed to copy for a non-device reason.
    TransferFailed,
    /// The requested feature is not available in this build/platform.
    Unsupported,
    /// Another operation is already running.
    Busy,
    /// Unexpected internal failure.
    Internal,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Device-level failures abort the whole transfer; everything else is a
    /// per-file failure that later files can still recover from.
    pub fn is_device_level(&self) -> bool {
        matches!(
            self.code,
            ErrorCode::NoDevice
                | ErrorCode::NeedsTrust
                | ErrorCode::ServiceUnavailable
                | ErrorCode::DeviceDisconnected
        )
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::PermissionDenied => AppError::new(
                ErrorCode::PermissionDenied,
                "This computer refused the file operation. Check the folder permissions and try again.",
            ),
            std::io::ErrorKind::NotFound => AppError::new(
                ErrorCode::DestinationInvalid,
                "The destination folder no longer exists. Choose another folder.",
            ),
            std::io::ErrorKind::StorageFull => AppError::new(
                ErrorCode::OutOfSpace,
                "There is not enough free space on the destination drive.",
            ),
            _ => AppError::new(
                ErrorCode::Internal,
                format!("A file operation failed unexpectedly ({err})."),
            ),
        }
    }
}
