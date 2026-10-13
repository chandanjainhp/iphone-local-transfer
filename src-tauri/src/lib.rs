//! Tauri wiring: typed commands, a background device-status poller, and the
//! transfer thread. All USB/filesystem work happens off the UI thread.

mod device;
mod environment;
mod errors;
mod media;
mod naming;
mod platform;
mod preview;
mod progress;
mod provider_libimobile;
#[cfg(debug_assertions)]
mod provider_mock;
mod transfer;

use crate::device::{DeviceProvider, DeviceSnapshot, DeviceState};
use crate::errors::{AppError, ErrorCode};
use crate::media::MediaItem;
use crate::naming::ConflictPolicy;
use crate::provider_libimobile::LibimobileProvider;
use crate::transfer::{check_destination, run_transfer, TransferEvent, TransferRequest};
use std::path::PathBuf;
#[cfg(debug_assertions)]
use std::sync::atomic::AtomicU8;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Real libimobiledevice adapter (the only mode allowed in production).
    Native,
    /// Development-only mock adapter; every snapshot carries `demo: true`.
    #[allow(dead_code)]
    Mock,
}

/// Mock device lifecycle for demo mode: 0 = no device, 1 = needs trust, 2 = ready.
/// Debug-only, like every other mock path, so release builds carry no demo code.
#[cfg(debug_assertions)]
static MOCK_DEVICE_STATE: AtomicU8 = AtomicU8::new(2);

struct MediaCache {
    udid: String,
    items: Vec<MediaItem>,
}

pub struct AppState {
    mode: RwLock<Mode>,
    last_snapshot: Mutex<DeviceSnapshot>,
    media_cache: Mutex<Option<MediaCache>>,
    /// Encoded preview data URLs, byte-bounded so scrolling cannot grow
    /// memory without limit. Keyed by udid + item id + size + mtime.
    preview_cache: Mutex<preview::PreviewCache>,
    /// `Some` while a transfer runs; the flag requests cancellation.
    transfer_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            mode: RwLock::new(Mode::Native),
            last_snapshot: Mutex::new(DeviceSnapshot {
                state: DeviceState::NoDevice,
                device: None,
                message: "Looking for your iPhone…".to_string(),
                hint: Some("Connect your iPhone with a USB cable, then unlock it.".to_string()),
                demo: false,
                source: String::new(),
            }),
            media_cache: Mutex::new(None),
            preview_cache: Mutex::new(preview::PreviewCache::new()),
            transfer_cancel: Mutex::new(None),
        }
    }
}

fn provider_for(mode: Mode) -> Result<Box<dyn DeviceProvider>, AppError> {
    match mode {
        Mode::Native => Ok(Box::new(LibimobileProvider)),
        Mode::Mock => {
            #[cfg(debug_assertions)]
            {
                use provider_mock::MockDeviceState;
                let state = match MOCK_DEVICE_STATE.load(Ordering::Relaxed) {
                    0 => MockDeviceState::NoDevice,
                    1 => MockDeviceState::NeedsTrust,
                    _ => MockDeviceState::Ready,
                };
                Ok(Box::new(provider_mock::MockProvider { state }))
            }
            #[cfg(not(debug_assertions))]
            {
                Err(AppError::new(
                    ErrorCode::Unsupported,
                    "Demo mode is only available in development builds.",
                ))
            }
        }
    }
}

fn error_snapshot(err: &AppError) -> DeviceSnapshot {
    let hint = match err.code {
        ErrorCode::ToolMissing | ErrorCode::ServiceUnavailable => Some(platform::install_hint()),
        ErrorCode::NeedsTrust => {
            Some("Unlock your iPhone and tap Trust on the iPhone screen.".to_string())
        }
        _ => None,
    };
    DeviceSnapshot {
        state: DeviceState::Error,
        device: None,
        message: err.message.clone(),
        hint,
        demo: false,
        source: match err.code {
            ErrorCode::Unsupported => "mock (demo only)".to_string(),
            _ => "libimobiledevice".to_string(),
        },
    }
}

/// Background poller: keeps `last_snapshot` fresh and pushes `device_status`
/// events whenever the status changes. Detection runs subprocesses, so it must
/// never happen on the UI thread.
fn spawn_poller(app: AppHandle) {
    std::thread::spawn(move || loop {
        let snapshot = {
            let state = app.state::<AppState>();
            let mode = *state.mode.read().expect("mode lock");
            let result = match provider_for(mode) {
                Ok(provider) => provider.detect().map(|mut snap| {
                    if snap.source.is_empty() {
                        snap.source = provider.label().to_string();
                    }
                    snap
                }),
                Err(e) => Err(e),
            };
            let mut snap = match result {
                Ok(s) => s,
                Err(e) => error_snapshot(&e),
            };
            if mode == Mode::Mock {
                snap.demo = true;
                if snap.source.is_empty() {
                    snap.source = "mock (demo only)".to_string();
                }
            }
            let mut last = state.last_snapshot.lock().expect("snapshot lock");
            let changed = serde_json::to_string(&*last)
                .ok()
                .map(|prev| prev != serde_json::to_string(&snap).unwrap_or_default())
                .unwrap_or(true);
            if changed {
                *last = snap.clone();
                drop(last);
                let _ = app.emit("device_status", &snap);
            }
            snap
        };
        let _ = snapshot;
        std::thread::sleep(POLL_INTERVAL);
    });
}

// ---------------------------------------------------------------------------
// commands
// ---------------------------------------------------------------------------

/// Toolchain / platform diagnostics for the UI (fast: PATH lookups only).
#[tauri::command]
fn diagnostics() -> platform::Diagnostics {
    platform::diagnostics()
}

/// One-shot startup prerequisite check. Runs the real probes (tool lookup,
/// usbmuxd reachability, device listing, trust verification) off the UI
/// thread and maps them onto explicit states the setup screen renders. Never
/// mutates the system: nothing is installed, started, or paired.
#[tauri::command]
async fn check_environment() -> environment::EnvReport {
    tauri::async_runtime::spawn_blocking(environment::check)
        .await
        .unwrap_or_else(|e| environment::failure_report(&format!("{e}")))
}

/// Last known device status (the poller keeps it fresh and also pushes events).
#[tauri::command]
fn get_device_status(state: State<AppState>) -> DeviceSnapshot {
    state.last_snapshot.lock().expect("snapshot lock").clone()
}

/// Scan the connected iPhone for accessible photos/videos. Runs off the UI
/// thread; caches results so transfers resolve ids against a fresh listing.
#[tauri::command]
async fn list_media(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<MediaItem>, AppError> {
    let mode = *state.mode.read().expect("mode lock");
    let snapshot = state.last_snapshot.lock().expect("snapshot lock").clone();
    if !snapshot.is_actionable() {
        let code = match snapshot.state {
            DeviceState::NeedsTrust => ErrorCode::NeedsTrust,
            DeviceState::Unsupported => ErrorCode::Unsupported,
            DeviceState::Error => ErrorCode::ServiceUnavailable,
            _ => ErrorCode::NoDevice,
        };
        return Err(AppError::new(code, snapshot.message));
    }
    let udid = snapshot
        .device
        .as_ref()
        .map(|d| d.udid.clone())
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::NoDevice,
                "Connect and unlock your iPhone before scanning.",
            )
        })?;

    tauri::async_runtime::spawn_blocking(move || {
        let provider = provider_for(mode)?;
        let items = provider.media().list_media(&udid)?;
        let state = app.state::<AppState>();
        *state.media_cache.lock().expect("cache lock") = Some(MediaCache {
            udid: udid.clone(),
            items: items.clone(),
        });
        Ok(items)
    })
    .await
    .map_err(|e| AppError::new(ErrorCode::Internal, format!("Scan failed ({e}).")))?
}

/// Preview thumbnail for one listed file, as a `data:` URL — or `null` when
/// the source exposes no preview (the UI then shows an icon placeholder).
/// Reads a pre-generated on-device thumbnail, never the original media, so it
/// stays fast and bounded. Resolves the id against the last scan first: an
/// unknown id can never reach the device.
#[tauri::command]
async fn get_media_preview(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, AppError> {
    let mode = *state.mode.read().expect("mode lock");
    let snapshot = state.last_snapshot.lock().expect("snapshot lock").clone();
    if !snapshot.is_actionable() {
        return Ok(None);
    }
    let udid = match snapshot.device.as_ref().map(|d| d.udid.clone()) {
        Some(u) => u,
        None => return Ok(None),
    };
    let (source_path, cache_key) = {
        let cache = state.media_cache.lock().expect("cache lock");
        let cache = match cache.as_ref().filter(|c| c.udid == udid) {
            Some(c) => c,
            // No scan yet (or a different device): nothing to preview.
            None => return Ok(None),
        };
        let item = cache.items.iter().find(|i| i.id == id).ok_or_else(|| {
            AppError::new(
                ErrorCode::MediaUnavailable,
                "That file is not part of the accessible photo library.",
            )
        })?;
        (
            item.source_path.clone(),
            format!("{udid}|{}|{:?}|{:?}", item.id, item.size, item.modified_ms),
        )
    };
    if let Some(url) = state
        .preview_cache
        .lock()
        .expect("preview lock")
        .get(&cache_key)
        .map(str::to_string)
    {
        return Ok(Some(url));
    }

    let image = tauri::async_runtime::spawn_blocking(move || {
        let provider = provider_for(mode)?;
        provider.media().preview(&udid, &source_path)
    })
    .await
    .map_err(|e| AppError::new(ErrorCode::Internal, format!("Preview failed ({e}).")))??;

    let url = image.map(|img| preview::data_url(&img.bytes, img.mime));
    if let Some(url) = &url {
        state
            .preview_cache
            .lock()
            .expect("preview lock")
            .insert(cache_key, url.clone());
    }
    Ok(url)
}

/// Native folder picker for the destination directory.
///
/// Must stay `async`: Tauri executes synchronous commands on the main thread,
/// and `tauri-plugin-dialog`'s blocking API must never be called there (its
/// docs: "should *NOT* be used when running on the main thread" — the dialog
/// is shown through the main-thread event loop, so blocking that thread
/// deadlocks and the picker never appears). Async commands run on Tauri's
/// async runtime instead; this is also how the plugin's own `open` command
/// is implemented.
///
/// Returns `None` when the user cancels, so the UI keeps the previous
/// destination unchanged.
#[tauri::command]
async fn pick_destination(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_folder()?;
    // Normalize both `Path` and `file://` variants into a plain filesystem
    // path so a URL can never leak into the transfer destination.
    picked
        .into_path()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Start a transfer batch on its own thread. Idempotence is enforced here:
/// only one batch can run at a time.
#[tauri::command]
fn start_transfer(
    app: AppHandle,
    state: State<AppState>,
    items: Vec<String>,
    destination: String,
    conflict: ConflictPolicy,
    // True for "Transfer all new": skip files whose destination copy is
    // reliably the same file (name + size + preserved modification time).
    new_only: bool,
) -> Result<(), AppError> {
    let cancel = {
        let mut slot = state.transfer_cancel.lock().expect("transfer lock");
        if slot.is_some() {
            return Err(AppError::new(
                ErrorCode::Busy,
                "A transfer is already running.",
            ));
        }
        let flag = Arc::new(AtomicBool::new(false));
        *slot = Some(flag.clone());
        flag
    };

    let result = (|| -> Result<(Box<dyn DeviceProvider>, String, TransferRequest), AppError> {
        let mode = *state.mode.read().expect("mode lock");
        let provider = provider_for(mode)?;
        let snapshot = state.last_snapshot.lock().expect("snapshot lock").clone();
        let udid = snapshot
            .device
            .as_ref()
            .map(|d| d.udid.clone())
            .ok_or_else(|| AppError::new(ErrorCode::NoDevice, "No iPhone is connected."))?;
        let cache_guard = state.media_cache.lock().expect("cache lock");
        let cache = cache_guard
            .as_ref()
            .filter(|c| c.udid == udid)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::MediaUnavailable,
                    "Scan the iPhone again to refresh the file list.",
                )
            })?;
        let chosen: Vec<MediaItem> = items
            .iter()
            .filter_map(|id| cache.items.iter().find(|i| &i.id == id).cloned())
            .collect();
        if chosen.is_empty() {
            return Err(AppError::new(
                ErrorCode::MediaUnavailable,
                "Select at least one file to transfer.",
            ));
        }
        drop(cache_guard);
        let request = TransferRequest {
            items: chosen,
            destination: PathBuf::from(destination),
            conflict,
            new_only,
        };
        Ok((provider, udid, request))
    })();

    let (provider, udid, request) = match result {
        Ok(triple) => triple,
        Err(e) => {
            *state.transfer_cancel.lock().expect("transfer lock") = None;
            return Err(e);
        }
    };

    // Fail fast on the invoke itself: the folder must exist and be writable
    // before any device work starts. The transfer thread re-checks anyway, so
    // a race cannot slip a bad folder through.
    if let Err(e) = check_destination(&request.destination) {
        *state.transfer_cancel.lock().expect("transfer lock") = None;
        return Err(e);
    }

    std::thread::spawn(move || {
        {
            let handle = app.clone();
            let mut emit = |event: TransferEvent| {
                let _ = handle.emit("transfer_event", &event);
            };
            run_transfer(provider.media(), &udid, &request, &cancel, &mut emit);
        }
        let state = app.state::<AppState>();
        *state.transfer_cancel.lock().expect("transfer lock") = None;
    });
    Ok(())
}

/// Request cancellation of the running batch (safe at any point).
#[tauri::command]
fn cancel_transfer(state: State<AppState>) {
    if let Some(flag) = state
        .transfer_cancel
        .lock()
        .expect("transfer lock")
        .as_ref()
    {
        flag.store(true, Ordering::SeqCst);
    }
}

/// Development-only: enable the labeled mock adapter. Refuses in release
/// builds so mock data can never be reported as a real device in production.
#[tauri::command]
fn set_mock_mode(state: State<AppState>, enabled: bool) -> Result<bool, AppError> {
    if state
        .transfer_cancel
        .lock()
        .expect("transfer lock")
        .is_some()
    {
        return Err(AppError::new(
            ErrorCode::Busy,
            "Wait for the current transfer to finish.",
        ));
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = enabled;
        let _ = state;
        Err(AppError::new(
            ErrorCode::Unsupported,
            "Demo mode is only available in development builds.",
        ))
    }
    #[cfg(debug_assertions)]
    {
        *state.mode.write().expect("mode lock") = if enabled { Mode::Mock } else { Mode::Native };
        *state.media_cache.lock().expect("cache lock") = None;
        Ok(enabled)
    }
}

/// Development-only: switch the demo device between no-device / needs-trust /
/// ready so every UI state can be exercised without hardware.
#[tauri::command]
fn set_mock_device_state(state: State<AppState>, value: String) -> Result<(), AppError> {
    #[cfg(not(debug_assertions))]
    {
        let _ = (state, value);
        Err(AppError::new(
            ErrorCode::Unsupported,
            "Demo mode is only available in development builds.",
        ))
    }
    #[cfg(debug_assertions)]
    {
        if *state.mode.read().expect("mode lock") != Mode::Mock {
            return Err(AppError::new(
                ErrorCode::Unsupported,
                "Enable demo mode first.",
            ));
        }
        let code = match value.as_str() {
            "no_device" => 0,
            "needs_trust" => 1,
            "ready" => 2,
            other => {
                return Err(AppError::new(
                    ErrorCode::Unsupported,
                    format!("Unknown demo state '{other}'."),
                ))
            }
        };
        MOCK_DEVICE_STATE.store(code, Ordering::Relaxed);
        *state.media_cache.lock().expect("cache lock") = None;
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // WebKitGTK's experimental DMABUF renderer renders a blank window on some
    // GPU drivers (reproduced here: NVIDIA + Wayland, logged as "Failed to
    // create GBM buffer"). Software rendering is fast enough for this UI, so
    // fall back unless the user has explicitly chosen otherwise. Must be set
    // before the first WebKit widget is created.
    //
    // On webkit2gtk >= 2.50 the DMABUF fallback alone is not enough: the
    // window still never paints (silently, with no log output) unless
    // compositing mode is also disabled. Both are respected if already set.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            spawn_poller(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            diagnostics,
            check_environment,
            get_device_status,
            list_media,
            get_media_preview,
            pick_destination,
            start_transfer,
            cancel_transfer,
            set_mock_mode,
            set_mock_device_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Local iPhone Transfer");
}
