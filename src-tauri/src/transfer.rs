//! Transfer engine: bounded, cancellable, honest copies.
//!
//! Policy (documented behavior):
//! * Every file is copied into a hidden temporary file in the destination
//!   folder (`.<name>.part-<pid>`), flushed with `sync_all`, then published
//!   under its final name only after byte-count verification.
//! * On failure, cancellation or disconnect the temporary file is deleted and
//!   the item is reported as failed/interrupted — never as completed.
//! * Files that already exist are skipped (default) or renamed; nothing is
//!   ever silently overwritten.
//! * Progress, speed and ETA come from real measured samples only.

use crate::device::MediaSource;
use crate::errors::{AppError, ErrorCode};
use crate::media::MediaItem;
use crate::naming::{
    next_available_name, resolve_conflict, sanitize_filename, temporary_name, ConflictPolicy,
};
use crate::progress::SpeedMeter;
use serde::Serialize;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Minimum spacing between progress events for one file.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOutcome {
    Completed,
    Skipped,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransferEvent {
    Started {
        total_files: usize,
        /// `None` when any selected file has an unknown size.
        total_bytes: Option<u64>,
    },
    FileStarted {
        index: usize,
        total_files: usize,
        name: String,
    },
    Progress {
        index: usize,
        total_files: usize,
        name: String,
        file_done: u64,
        file_total: Option<u64>,
        overall_done: u64,
        overall_total: Option<u64>,
        /// Instantaneous (short window) speed in bytes/second.
        speed_bps: Option<u64>,
        /// Average over the longer window in bytes/second.
        avg_bps: Option<u64>,
        /// Seconds remaining; `None` until measurable.
        eta_secs: Option<u64>,
    },
    FileFinished {
        index: usize,
        total_files: usize,
        name: String,
        outcome: FileOutcome,
        /// Final filename in the destination, when something was written.
        saved_as: Option<String>,
        bytes: u64,
        error: Option<AppError>,
        /// Why the file was skipped, in the UI's words (e.g. "Already
        /// transferred"). `None` for outcomes where no explanation is needed.
        note: Option<String>,
    },
    Finished {
        completed: usize,
        skipped: usize,
        failed: usize,
        interrupted: usize,
        bytes_copied: u64,
        cancelled: bool,
        /// Device-level stop reason, if any.
        error: Option<AppError>,
        message: String,
    },
}

pub struct TransferRequest {
    pub items: Vec<MediaItem>,
    pub destination: PathBuf,
    pub conflict: ConflictPolicy,
    /// "Transfer all new": skip files whose destination copy is reliably the
    /// same file (see [`already_transferred`]) instead of consulting the
    /// conflict policy for them.
    pub new_only: bool,
}

/// How far the destination's modification time may drift from the source's
/// and still count as our own preserved copy: FAT/exFAT store 2-second
/// granularity, so an exact match cannot be required.
const MTIME_TOLERANCE_MS: i64 = 2_000;

/// Reliable identity for `new_only`: the destination file must match the
/// source by **name, size, and the modification time we preserved when we
/// copied it**. A filename alone never establishes identity — a same-named
/// file with a different size or date is a *different* file and falls through
/// to the conflict policy.
fn already_transferred(dest: &Path, name: &str, item: &MediaItem) -> bool {
    let meta = match std::fs::metadata(dest.join(name)) {
        Ok(m) if m.is_file() => m,
        _ => return false,
    };
    // Identity needs both knowns; without them we refuse to claim sameness.
    let (Some(size), Some(src_ms)) = (item.size, item.modified_ms) else {
        return false;
    };
    if meta.len() != size {
        return false;
    }
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let dest_ms: i64 = match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    };
    (dest_ms - src_ms).abs() <= MTIME_TOLERANCE_MS
}

/// Validate the destination folder and probe that we can actually write there.
pub fn check_destination(dest: &Path) -> Result<(), AppError> {
    let meta = std::fs::metadata(dest).map_err(|_| {
        AppError::new(
            ErrorCode::DestinationInvalid,
            "The destination folder does not exist. Choose another folder.",
        )
    })?;
    if !meta.is_dir() {
        return Err(AppError::new(
            ErrorCode::DestinationInvalid,
            "The destination is not a folder. Choose a folder.",
        ));
    }
    let probe = dest.join(format!(".lit-write-test-{}", std::process::id()));
    let _ = std::fs::remove_file(&probe);
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(f) => {
            drop(f);
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(e) => Err(AppError::from(e)),
    }
}

/// Atomically-ish publish the temp file under its final name without ever
/// overwriting an existing file.
enum PublishError {
    Exists,
    Io(AppError),
}

fn publish(tmp: &Path, target: &Path) -> Result<(), PublishError> {
    // hard_link fails with AlreadyExists when something else won the race —
    // it never clobbers. Same directory, so no cross-device issue.
    match std::fs::hard_link(tmp, target) {
        Ok(()) => {
            let _ = std::fs::remove_file(tmp);
            return Ok(());
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(PublishError::Exists)
        }
        Err(_) => { /* filesystems without hard links: fall back to rename */ }
    }
    match std::fs::rename(tmp, target) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(PublishError::Exists),
        Err(e) => {
            // Windows reports "file exists" as a generic error sometimes.
            if target.exists() {
                Err(PublishError::Exists)
            } else {
                Err(PublishError::Io(AppError::from(e)))
            }
        }
    }
}

fn sync_then_set_mtime(path: &Path, item: &MediaItem) -> Result<(), AppError> {
    let file = OpenOptions::new().write(true).open(path)?;
    file.sync_all()?;
    drop(file);
    // Preserve the device's modification date only when it was exposed and can
    // be represented here. Never invent a date.
    if let Some(ms) = item.modified_ms {
        let secs = ms.div_euclid(1000);
        let nanos = (ms.rem_euclid(1000) * 1_000_000) as u32;
        let ftime = filetime::FileTime::from_unix_time(secs, nanos);
        let _ = filetime::set_file_mtime(path, ftime);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_one(
    source: &dyn MediaSource,
    udid: &str,
    item: &MediaItem,
    index: usize,
    total_files: usize,
    dest: &Path,
    conflict: ConflictPolicy,
    new_only: bool,
    cancel: &AtomicBool,
    overall_done: u64,
    overall_total: Option<u64>,
    overall_meter: &mut SpeedMeter,
    emit: &mut dyn FnMut(TransferEvent),
) -> OneResult {
    let base_name = sanitize_filename(&item.name);

    // "Transfer all new": identity first, so an already-copied file is never
    // re-downloaded and never misreported as a name conflict.
    if new_only && already_transferred(dest, &base_name, item) {
        emit(TransferEvent::FileFinished {
            index,
            total_files,
            name: item.name.clone(),
            outcome: FileOutcome::Skipped,
            saved_as: Some(base_name),
            bytes: 0,
            error: None,
            note: Some("Already transferred".to_string()),
        });
        return OneResult::Skipped;
    }

    let final_name = match resolve_conflict(dest, &base_name, conflict) {
        Some(name) => name,
        None => {
            emit(TransferEvent::FileFinished {
                index,
                total_files,
                name: item.name.clone(),
                outcome: FileOutcome::Skipped,
                saved_as: Some(base_name),
                bytes: 0,
                error: None,
                note: Some("A different file with this name already exists".to_string()),
            });
            return OneResult::Skipped;
        }
    };

    emit(TransferEvent::FileStarted {
        index,
        total_files,
        name: item.name.clone(),
    });

    let tmp = dest.join(temporary_name(&final_name));
    let _ = std::fs::remove_file(&tmp);

    let mut file_meter = SpeedMeter::new();
    let mut last_emit = Instant::now() - PROGRESS_INTERVAL;
    let base = overall_done;

    {
        let mut cb = |done: u64| {
            let now = Instant::now();
            let finished = item.size == Some(done);
            if !finished && now.duration_since(last_emit) < PROGRESS_INTERVAL {
                return;
            }
            last_emit = now;
            file_meter.record(now, done);
            overall_meter.record(now, base + done);
            let current_overall = base + done;
            let remaining = overall_total.map(|t| t.saturating_sub(current_overall));
            emit(TransferEvent::Progress {
                index,
                total_files,
                name: item.name.clone(),
                file_done: done,
                file_total: item.size,
                overall_done: current_overall,
                overall_total,
                speed_bps: file_meter.instant_bps(now),
                avg_bps: overall_meter.average_bps(now),
                eta_secs: overall_meter.eta_secs(now, remaining),
            });
        };

        let result = source.copy_to(udid, &item.source_path, &tmp, item.size, cancel, &mut cb);
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            let outcome = if e.code == ErrorCode::Cancelled || e.is_device_level() {
                FileOutcome::Interrupted
            } else {
                FileOutcome::Failed
            };
            emit(TransferEvent::FileFinished {
                index,
                total_files,
                name: item.name.clone(),
                outcome,
                saved_as: None,
                bytes: 0,
                error: Some(e.clone()),
                note: None,
            });
            return OneResult::Error(e);
        }
    }

    let bytes = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);

    // Verify against the advertised size before anything is published.
    if let Some(expected) = item.size {
        if bytes != expected {
            let _ = std::fs::remove_file(&tmp);
            let err = AppError::new(
                ErrorCode::TransferFailed,
                "The copy did not finish, so the file was not saved.",
            );
            emit(TransferEvent::FileFinished {
                index,
                total_files,
                name: item.name.clone(),
                outcome: FileOutcome::Failed,
                saved_as: None,
                bytes: 0,
                error: Some(err.clone()),
                note: None,
            });
            return OneResult::Error(err);
        }
    }

    // Publish under the final name, never over an existing file.
    let mut target_name = final_name.clone();
    let target = dest.join(&target_name);
    let mut publish_result = publish(&tmp, &target);
    if matches!(publish_result, Err(PublishError::Exists)) {
        match conflict {
            ConflictPolicy::Skip => {
                let _ = std::fs::remove_file(&tmp);
                emit(TransferEvent::FileFinished {
                    index,
                    total_files,
                    name: item.name.clone(),
                    outcome: FileOutcome::Skipped,
                    saved_as: Some(target_name),
                    bytes: 0,
                    error: None,
                    note: Some("A different file with this name already exists".to_string()),
                });
                return OneResult::Skipped;
            }
            ConflictPolicy::Rename => {
                target_name = next_available_name(dest, &base_name);
                publish_result = publish(&tmp, &dest.join(&target_name));
            }
            ConflictPolicy::Replace => {
                // Explicitly confirmed by the user. The new bytes are already
                // written and verified in `tmp`; only now does the old file
                // give way, so a failed copy never leaves a half-replaced file.
                match std::fs::remove_file(&target) {
                    Ok(()) => publish_result = publish(&tmp, &target),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        publish_result = publish(&tmp, &target);
                    }
                    Err(e) => publish_result = Err(PublishError::Io(AppError::from(e))),
                }
            }
        }
    }
    if matches!(publish_result, Err(PublishError::Exists)) {
        // An external race created the name twice; never overwrite.
        let _ = std::fs::remove_file(&tmp);
        emit(TransferEvent::FileFinished {
            index,
            total_files,
            name: item.name.clone(),
            outcome: FileOutcome::Skipped,
            saved_as: Some(target_name),
            bytes: 0,
            error: None,
            note: None,
        });
        return OneResult::Skipped;
    }

    if let Err(PublishError::Io(e)) = publish_result {
        let _ = std::fs::remove_file(&tmp);
        emit(TransferEvent::FileFinished {
            index,
            total_files,
            name: item.name.clone(),
            outcome: FileOutcome::Failed,
            saved_as: None,
            bytes: 0,
            error: Some(e.clone()),
            note: None,
        });
        return OneResult::Error(e);
    }

    if let Err(e) = sync_then_set_mtime(&dest.join(&target_name), item) {
        // The bytes are already verified and published; report the file as
        // completed but surface the metadata problem.
        emit(TransferEvent::FileFinished {
            index,
            total_files,
            name: item.name.clone(),
            outcome: FileOutcome::Completed,
            saved_as: Some(target_name),
            bytes,
            error: Some(e),
            note: None,
        });
        return OneResult::Completed(bytes);
    }

    emit(TransferEvent::FileFinished {
        index,
        total_files,
        name: item.name.clone(),
        outcome: FileOutcome::Completed,
        saved_as: Some(target_name),
        bytes,
        error: None,
        note: None,
    });
    OneResult::Completed(bytes)
}

enum OneResult {
    Completed(u64),
    Skipped,
    Error(AppError),
}

fn summarise(
    completed: usize,
    skipped: usize,
    failed: usize,
    interrupted: usize,
    bytes: u64,
    cancelled: bool,
    error: Option<&AppError>,
) -> String {
    if let Some(e) = error {
        return e.message.clone();
    }
    if cancelled {
        return format!(
            "Cancelled. {completed} file{} copied, {skipped} skipped.",
            if completed == 1 { "" } else { "s" }
        );
    }
    let mut parts = vec![format!(
        "{completed} file{} copied.",
        if completed == 1 { "" } else { "s" }
    )];
    if skipped > 0 {
        parts.push(format!("{skipped} skipped (already in the folder)."));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed."));
    }
    if interrupted > 0 {
        parts.push(format!("{interrupted} interrupted."));
    }
    let _ = bytes;
    parts.join(" ")
}

/// Run the batch. Never blocks forever: honors `cancel`, and only `emit`
/// (which is called from the transfer thread) delivers events.
pub fn run_transfer(
    source: &dyn MediaSource,
    udid: &str,
    request: &TransferRequest,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(TransferEvent),
) {
    let total_files = request.items.len();
    let dest = &request.destination;

    if let Err(e) = check_destination(dest) {
        emit(TransferEvent::Finished {
            completed: 0,
            skipped: 0,
            failed: 0,
            interrupted: 0,
            bytes_copied: 0,
            cancelled: false,
            error: Some(e.clone()),
            message: e.message,
        });
        return;
    }
    if total_files == 0 {
        emit(TransferEvent::Finished {
            completed: 0,
            skipped: 0,
            failed: 0,
            interrupted: 0,
            bytes_copied: 0,
            cancelled: false,
            error: None,
            message: "No files were selected.".to_string(),
        });
        return;
    }

    let all_sizes_known = request.items.iter().all(|i| i.size.is_some());
    let mut overall_total: u64 = request.items.iter().filter_map(|i| i.size).sum();
    emit(TransferEvent::Started {
        total_files,
        total_bytes: if all_sizes_known {
            Some(overall_total)
        } else {
            None
        },
    });

    let mut completed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut interrupted = 0usize;
    let mut bytes_copied = 0u64;
    let mut overall_done = 0u64;
    let mut overall_meter = SpeedMeter::new();
    let mut stop: Option<AppError> = None;

    for (index, item) in request.items.iter().enumerate() {
        if stop.is_some() {
            break;
        }
        if cancel.load(Ordering::SeqCst) {
            stop = Some(AppError::new(
                ErrorCode::Cancelled,
                "The transfer was cancelled.",
            ));
            break;
        }

        let outcome = run_one(
            source,
            udid,
            item,
            index,
            total_files,
            dest,
            request.conflict,
            request.new_only,
            cancel,
            overall_done,
            if all_sizes_known {
                Some(overall_total)
            } else {
                None
            },
            &mut overall_meter,
            emit,
        );

        match outcome {
            OneResult::Completed(bytes) => {
                completed += 1;
                bytes_copied += bytes;
                overall_done += bytes;
            }
            OneResult::Skipped => {
                skipped += 1;
                if let Some(size) = item.size {
                    overall_total = overall_total.saturating_sub(size);
                }
            }
            OneResult::Error(e) => {
                if let Some(size) = item.size {
                    overall_total = overall_total.saturating_sub(size);
                }
                if e.code == ErrorCode::Cancelled || e.is_device_level() {
                    interrupted += 1;
                    stop = Some(e);
                } else {
                    failed += 1;
                }
            }
        }
    }

    let cancelled = matches!(stop, Some(ref e) if e.code == ErrorCode::Cancelled);
    let message = summarise(
        completed,
        skipped,
        failed,
        interrupted,
        bytes_copied,
        cancelled,
        stop.as_ref(),
    );
    emit(TransferEvent::Finished {
        completed,
        skipped,
        failed,
        interrupted,
        bytes_copied,
        cancelled,
        error: stop,
        message,
    });
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaKind;
    use std::collections::HashMap;

    #[derive(Clone)]
    enum Behavior {
        Normal,
        /// Fail with `code` after writing `after` bytes.
        FailAfter(u64, ErrorCode),
        /// Cancel the batch once `after` bytes have been handed to the app.
        CancelAfter(u64),
    }

    struct FakeSource {
        files: HashMap<String, Vec<u8>>,
        behavior: HashMap<String, Behavior>,
        cancel_flag: Option<Arc<AtomicBool>>,
    }

    use std::sync::Arc;

    impl FakeSource {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
                behavior: HashMap::new(),
                cancel_flag: None,
            }
        }

        fn add(&mut self, path: &str, content: Vec<u8>) {
            self.files.insert(path.to_string(), content);
        }
    }

    impl MediaSource for FakeSource {
        fn list_media(&self, _udid: &str) -> Result<Vec<MediaItem>, AppError> {
            Ok(vec![])
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
            let content = self
                .files
                .get(source_path)
                .ok_or_else(|| AppError::new(ErrorCode::TransferFailed, "missing"))?;
            let behavior = self
                .behavior
                .get(source_path)
                .cloned()
                .unwrap_or(Behavior::Normal);
            let mut out = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(dest_file)
                .map_err(AppError::from)?;
            use std::io::Write;
            let mut written = 0u64;
            progress(0);
            for chunk in content.chunks(64) {
                if cancel.load(Ordering::SeqCst) {
                    return Err(AppError::new(ErrorCode::Cancelled, "cancelled"));
                }
                out.write_all(chunk).map_err(AppError::from)?;
                written += chunk.len() as u64;
                progress(written);
                if let Behavior::FailAfter(after, code) = &behavior {
                    if written >= *after {
                        return Err(AppError::new(*code, "simulated failure"));
                    }
                }
                if let Behavior::CancelAfter(after) = &behavior {
                    if written >= *after {
                        if let Some(flag) = &self.cancel_flag {
                            flag.store(true, Ordering::SeqCst);
                        }
                    }
                }
            }
            drop(out);
            if let Some(expected) = expected_size {
                assert_eq!(expected, content.len() as u64);
            }
            Ok(written)
        }
    }

    fn item(path: &str, name: &str, content_len: usize) -> MediaItem {
        MediaItem {
            id: path.to_string(),
            name: name.to_string(),
            kind: MediaKind::Photo,
            size: Some(content_len as u64),
            modified_ms: Some(1_700_000_000_000),
            source_path: path.to_string(),
        }
    }

    fn temp_dest(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lit-transfer-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".part-") || n.starts_with(".lit-"))
            .collect()
    }

    #[test]
    fn happy_path_copies_and_cleans_up() {
        let dest = temp_dest("happy");
        let mut fake = FakeSource::new();
        let data = vec![7u8; 1000];
        fake.add("/DCIM/100APPLE/IMG_1.HEIC", data.clone());

        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/IMG_1.HEIC", "IMG_1.HEIC", data.len())],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        let saved = dest.join("IMG_1.HEIC");
        assert_eq!(std::fs::read(&saved).unwrap(), data, "bytes identical");
        assert!(leftovers(&dest).is_empty(), "no temp files left");
        let finished = events
            .iter()
            .find_map(|e| match e {
                TransferEvent::Finished {
                    completed,
                    skipped,
                    failed,
                    interrupted,
                    error,
                    ..
                } => Some((*completed, *skipped, *failed, *interrupted, error.is_some())),
                _ => None,
            })
            .expect("finished event");
        assert_eq!(finished, (1, 0, 0, 0, false));
        // The file is only reported completed after it exists under its name.
        let completed_idx = events
            .iter()
            .position(|e| {
                matches!(
                    e,
                    TransferEvent::FileFinished {
                        outcome: FileOutcome::Completed,
                        ..
                    }
                )
            })
            .expect("completed event");
        let file_started_idx = events
            .iter()
            .position(|e| matches!(e, TransferEvent::FileStarted { .. }))
            .unwrap();
        assert!(completed_idx > file_started_idx);
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn conflict_skip_never_overwrites() {
        let dest = temp_dest("skip");
        std::fs::write(dest.join("IMG_1.HEIC"), b"original").unwrap();
        let mut fake = FakeSource::new();
        fake.add("/DCIM/100APPLE/IMG_1.HEIC", vec![1u8; 100]);

        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/IMG_1.HEIC", "IMG_1.HEIC", 100)],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert_eq!(
            std::fs::read(dest.join("IMG_1.HEIC")).unwrap(),
            b"original",
            "existing file untouched"
        );
        assert!(leftovers(&dest).is_empty());
        assert!(events.iter().any(|e| matches!(
            e,
            TransferEvent::FileFinished {
                outcome: FileOutcome::Skipped,
                ..
            }
        )));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn conflict_rename_creates_new_name() {
        let dest = temp_dest("rename");
        std::fs::write(dest.join("IMG_1.HEIC"), b"original").unwrap();
        let mut fake = FakeSource::new();
        let data = vec![9u8; 64];
        fake.add("/DCIM/100APPLE/IMG_1.HEIC", data.clone());

        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/IMG_1.HEIC", "IMG_1.HEIC", data.len())],
            destination: dest.clone(),
            conflict: ConflictPolicy::Rename,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert_eq!(std::fs::read(dest.join("IMG_1.HEIC")).unwrap(), b"original");
        assert_eq!(std::fs::read(dest.join("IMG_1 (1).HEIC")).unwrap(), data);
        assert!(leftovers(&dest).is_empty());
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn hostile_filename_is_contained_in_destination() {
        let dest = temp_dest("sanitize");
        let parent = dest.parent().unwrap().to_path_buf();
        let mut fake = FakeSource::new();
        let data = vec![3u8; 16];
        fake.add("/DCIM/100APPLE/../../etc/x.HEIC", data.clone());

        let request = TransferRequest {
            items: vec![item(
                "/DCIM/100APPLE/../../etc/x.HEIC",
                "../../etc/x.HEIC",
                data.len(),
            )],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        let escaped = parent.join("etc");
        assert!(!escaped.exists(), "nothing escaped the destination folder");
        let entries: Vec<String> = std::fs::read_dir(&dest)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries.len(), 1, "one file saved: {entries:?}");
        assert!(!entries[0].contains("..") || !entries[0].contains('/'));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn cancel_mid_copy_cleans_partial_and_stops() {
        let dest = temp_dest("cancel");
        let mut fake = FakeSource::new();
        fake.add("/DCIM/100APPLE/A.HEIC", vec![1u8; 512]);
        fake.add("/DCIM/100APPLE/B.HEIC", vec![2u8; 512]);
        fake.behavior.insert(
            "/DCIM/100APPLE/A.HEIC".to_string(),
            Behavior::CancelAfter(64),
        );
        let cancel = Arc::new(AtomicBool::new(false));
        fake.cancel_flag = Some(cancel.clone());

        let request = TransferRequest {
            items: vec![
                item("/DCIM/100APPLE/A.HEIC", "A.HEIC", 512),
                item("/DCIM/100APPLE/B.HEIC", "B.HEIC", 512),
            ],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert!(!dest.join("A.HEIC").exists(), "no partial published");
        assert!(!dest.join("B.HEIC").exists(), "second file not attempted");
        assert!(leftovers(&dest).is_empty(), "partial removed");
        let finished = events
            .iter()
            .find_map(|e| match e {
                TransferEvent::Finished {
                    cancelled,
                    completed,
                    interrupted,
                    ..
                } => Some((*cancelled, *completed, *interrupted)),
                _ => None,
            })
            .expect("finished");
        assert_eq!(finished, (true, 0, 1));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn disconnect_stops_batch_and_reports_interrupted() {
        let dest = temp_dest("disconnect");
        let mut fake = FakeSource::new();
        fake.add("/DCIM/100APPLE/A.HEIC", vec![1u8; 4096]);
        fake.add("/DCIM/100APPLE/B.HEIC", vec![2u8; 64]);
        fake.behavior.insert(
            "/DCIM/100APPLE/A.HEIC".to_string(),
            Behavior::FailAfter(1024, ErrorCode::DeviceDisconnected),
        );

        let request = TransferRequest {
            items: vec![
                item("/DCIM/100APPLE/A.HEIC", "A.HEIC", 4096),
                item("/DCIM/100APPLE/B.HEIC", "B.HEIC", 64),
            ],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert!(!dest.join("A.HEIC").exists());
        assert!(!dest.join("B.HEIC").exists(), "batch stopped");
        assert!(leftovers(&dest).is_empty());
        let finished = events
            .iter()
            .find_map(|e| match e {
                TransferEvent::Finished {
                    completed,
                    interrupted,
                    error,
                    ..
                } => Some((*completed, *interrupted, error.as_ref().map(|e| e.code))),
                _ => None,
            })
            .expect("finished");
        assert_eq!(finished.0, 0);
        assert_eq!(finished.1, 1);
        assert_eq!(
            finished.2,
            Some(ErrorCode::DeviceDisconnected),
            "device-level error surfaced"
        );
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn per_file_failure_continues_batch() {
        let dest = temp_dest("perfile");
        let mut fake = FakeSource::new();
        fake.add("/DCIM/100APPLE/A.HEIC", vec![1u8; 512]);
        fake.add("/DCIM/100APPLE/B.HEIC", vec![2u8; 512]);
        fake.behavior.insert(
            "/DCIM/100APPLE/A.HEIC".to_string(),
            Behavior::FailAfter(128, ErrorCode::OutOfSpace),
        );

        let request = TransferRequest {
            items: vec![
                item("/DCIM/100APPLE/A.HEIC", "A.HEIC", 512),
                item("/DCIM/100APPLE/B.HEIC", "B.HEIC", 512),
            ],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert!(!dest.join("A.HEIC").exists(), "failed file not published");
        assert!(dest.join("B.HEIC").exists(), "batch continued");
        assert!(leftovers(&dest).is_empty());
        let finished = events
            .iter()
            .find_map(|e| match e {
                TransferEvent::Finished {
                    completed, failed, ..
                } => Some((*completed, *failed)),
                _ => None,
            })
            .unwrap();
        assert_eq!(finished, (1, 1));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn size_mismatch_is_never_reported_complete() {
        let dest = temp_dest("mismatch");
        // Advertise a bigger size than the source can deliver.
        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/A.HEIC", "A.HEIC", 4096)],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        // FakeSource asserts expected == content, so override behavior first:
        // use FailAfter normal path but mismatched expectation by hand.
        struct Liar;
        impl MediaSource for Liar {
            fn list_media(&self, _: &str) -> Result<Vec<MediaItem>, AppError> {
                Ok(vec![])
            }
            fn copy_to(
                &self,
                _: &str,
                _: &str,
                dest_file: &Path,
                _: Option<u64>,
                cancel: &AtomicBool,
                progress: &mut dyn FnMut(u64),
            ) -> Result<u64, AppError> {
                let _ = cancel;
                std::fs::write(dest_file, vec![0u8; 100]).unwrap();
                progress(100);
                Ok(100)
            }
        }
        run_transfer(&Liar, "udid", &request, &cancel, &mut |e| events.push(e));

        assert!(!dest.join("A.HEIC").exists(), "mismatch not published");
        assert!(leftovers(&dest).is_empty(), "partial cleaned");
        assert!(events.iter().any(|e| matches!(
            e,
            TransferEvent::Finished {
                failed: 1,
                completed: 0,
                ..
            }
        )));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    /// A read-only destination is refused up front with an actionable error;
    /// nothing is written and no file is reported as completed.
    /// (Vacuous when running as root — root ignores permission bits.)
    #[cfg(unix)]
    #[test]
    fn unwritable_destination_reports_permission_error() {
        use std::os::unix::fs::PermissionsExt;
        let dest = temp_dest("readonly");
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::File::create(dest.join(".probe")).is_ok() {
            let _ = std::fs::remove_file(dest.join(".probe"));
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::remove_dir_all(&dest).unwrap();
            return;
        }
        let fake = FakeSource::new();
        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/A.HEIC", "A.HEIC", 10)],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));
        assert_eq!(events.len(), 1, "fails before any progress starts");
        match &events[0] {
            TransferEvent::Finished {
                error, completed, ..
            } => {
                assert_eq!(*completed, 0);
                let err = error.as_ref().expect("destination error");
                assert!(
                    matches!(
                        err.code,
                        ErrorCode::PermissionDenied | ErrorCode::DestinationInvalid
                    ),
                    "unexpected error {err:?}"
                );
            }
            other => panic!("unexpected event {other:?}"),
        }
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn missing_destination_reports_error_without_touching_anything() {
        let dest = std::env::temp_dir().join(format!("lit-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        let fake = FakeSource::new();
        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/A.HEIC", "A.HEIC", 10)],
            destination: dest,
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));
        assert_eq!(events.len(), 1);
        match &events[0] {
            TransferEvent::Finished {
                error, completed, ..
            } => {
                assert_eq!(*completed, 0);
                assert!(error.is_some());
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn empty_selection_is_a_clean_noop() {
        let dest = temp_dest("empty");
        let fake = FakeSource::new();
        let request = TransferRequest {
            items: vec![],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events: Vec<TransferEvent> = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));
        assert!(matches!(
            events.as_slice(),
            [TransferEvent::Finished { completed: 0, message, .. }] if message.contains("selected")
        ));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn progress_events_carry_measured_values_only() {
        let dest = temp_dest("progress");
        let mut fake = FakeSource::new();
        fake.add("/DCIM/100APPLE/A.HEIC", vec![1u8; 40 * 1024]);
        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/A.HEIC", "A.HEIC", 40 * 1024)],
            destination: dest.clone(),
            conflict: ConflictPolicy::Skip,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));
        // Speeds/ETA may legitimately be None on a tiny fast copy — but every
        // progress event must carry truthful byte counts.
        for e in &events {
            if let TransferEvent::Progress {
                file_done,
                file_total,
                overall_done,
                overall_total,
                ..
            } = e
            {
                assert!(file_total.is_some());
                assert_eq!(*file_total, Some(40 * 1024));
                assert!(*file_done <= 40 * 1024);
                assert!(*overall_done <= 40 * 1024);
                assert_eq!(*overall_total, Some(40 * 1024));
            }
        }
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn conflict_replace_overwrites_only_after_a_verified_copy() {
        let dest = temp_dest("replace");
        std::fs::write(dest.join("IMG_1.HEIC"), b"original").unwrap();
        let mut fake = FakeSource::new();
        let data = vec![4u8; 128];
        fake.add("/DCIM/100APPLE/IMG_1.HEIC", data.clone());

        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/IMG_1.HEIC", "IMG_1.HEIC", data.len())],
            destination: dest.clone(),
            conflict: ConflictPolicy::Replace,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert_eq!(
            std::fs::read(dest.join("IMG_1.HEIC")).unwrap(),
            data,
            "existing file replaced"
        );
        assert!(leftovers(&dest).is_empty());
        assert!(events.iter().any(|e| matches!(
            e,
            TransferEvent::FileFinished {
                outcome: FileOutcome::Completed,
                ..
            }
        )));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn replace_keeps_the_original_when_the_copy_fails() {
        let dest = temp_dest("replace-fail");
        std::fs::write(dest.join("IMG_1.HEIC"), b"original").unwrap();
        let mut fake = FakeSource::new();
        fake.add("/DCIM/100APPLE/IMG_1.HEIC", vec![9u8; 128]);
        fake.behavior.insert(
            "/DCIM/100APPLE/IMG_1.HEIC".to_string(),
            Behavior::FailAfter(32, ErrorCode::TransferFailed),
        );

        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/IMG_1.HEIC", "IMG_1.HEIC", 128)],
            destination: dest.clone(),
            conflict: ConflictPolicy::Replace,
            new_only: false,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert_eq!(
            std::fs::read(dest.join("IMG_1.HEIC")).unwrap(),
            b"original",
            "a failed copy never replaces the existing file"
        );
        assert!(leftovers(&dest).is_empty());
        assert!(events.iter().any(|e| matches!(
            e,
            TransferEvent::FileFinished {
                outcome: FileOutcome::Failed,
                ..
            }
        )));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn new_only_skips_a_copy_it_already_made() {
        let dest = temp_dest("newonly-same");
        let data = vec![5u8; 96];
        let saved = dest.join("IMG_1.HEIC");
        std::fs::write(&saved, &data).unwrap();
        // Same size and the modification time we preserve when copying.
        filetime::set_file_mtime(&saved, filetime::FileTime::from_unix_time(1_700_000_000, 0))
            .unwrap();

        // FakeSource holds NO content for this path: any re-download attempt
        // would fail the run, so a pass proves the file was never fetched.
        let fake = FakeSource::new();
        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/IMG_1.HEIC", "IMG_1.HEIC", data.len())],
            destination: dest.clone(),
            conflict: ConflictPolicy::Replace,
            new_only: true,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert_eq!(
            std::fs::read(&saved).unwrap(),
            data,
            "destination untouched"
        );
        assert!(events.iter().any(|e| matches!(
            e,
            TransferEvent::FileFinished {
                outcome: FileOutcome::Skipped,
                note: Some(note),
                ..
            } if note == "Already transferred"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            TransferEvent::Finished {
                completed: 0,
                skipped: 1,
                error: None,
                ..
            }
        )));
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn new_only_never_trusts_the_filename_alone() {
        let dest = temp_dest("newonly-diff");
        // Same name AND same size — but a different file: the destination's
        // date does not match the source's preserved date.
        let other = vec![0u8; 96];
        let saved = dest.join("IMG_1.HEIC");
        std::fs::write(&saved, &other).unwrap();
        filetime::set_file_mtime(&saved, filetime::FileTime::from_unix_time(1_600_000_000, 0))
            .unwrap();

        let mut fake = FakeSource::new();
        let data = vec![6u8; 96];
        fake.add("/DCIM/100APPLE/IMG_1.HEIC", data.clone());

        let request = TransferRequest {
            items: vec![item("/DCIM/100APPLE/IMG_1.HEIC", "IMG_1.HEIC", data.len())],
            destination: dest.clone(),
            conflict: ConflictPolicy::Rename,
            new_only: true,
        };
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        run_transfer(&fake, "udid", &request, &cancel, &mut |e| events.push(e));

        assert_eq!(
            std::fs::read(&saved).unwrap(),
            other,
            "a same-named different file is never claimed as transferred"
        );
        assert_eq!(
            std::fs::read(dest.join("IMG_1 (1).HEIC")).unwrap(),
            data,
            "the new file lands under the conflict policy"
        );
        assert!(leftovers(&dest).is_empty());
        std::fs::remove_dir_all(&dest).unwrap();
    }
}
