import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import {
  cancelTransfer,
  checkEnvironment,
  errorMessage,
  getDeviceStatus,
  getDiagnostics,
  isTauri,
  listMedia,
  onDeviceStatus,
  onTransferEvent,
  pickDestination,
  setMockDeviceState,
  setMockMode,
  startTransfer,
} from "./api";
import { clearPreviews } from "./previews";
import { Icon } from "./icons";
import { useCloseOnOutsidePointerDown } from "./hooks";
import {
  newestUnseenProblem,
  notificationReducer,
  parseNotifications,
  serializeNotifications,
  STORAGE_KEY,
  transferOutcomeNotification,
  unreadCount,
  type AppNotification,
  type NotificationCategoryFilter,
  type NotificationFilter,
  type NotificationInput,
  type NotificationTarget,
} from "./notifications";
import { Sidebar } from "./components/Sidebar";
import { AppHeader } from "./components/AppHeader";
import { DeviceCard } from "./components/DeviceCard";
import { FileList } from "./components/FileList";
import { DestinationToolbar } from "./components/DestinationToolbar";
import {
  EMPTY_TRANSFER,
  TransferProgress,
  TransferSummary,
  type TransferState,
} from "./components/TransferProgress";
import { PreviewDialog } from "./components/PreviewDialog";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { SetupScreen } from "./components/SetupScreen";
import { NotificationBell } from "./components/NotificationBell";
import { NotificationPanel } from "./components/NotificationPanel";
import type {
  Category,
  ConflictPolicy,
  DeviceSnapshot,
  DeviceState,
  Diagnostics,
  EnvReport,
  MediaItem,
  MockDeviceState,
  SortOrder,
  TransferEvent,
} from "./types";

const DEST_KEY = "lit-destination";
const NOTIF_PANEL_ID = "notification-panel";

/** Where a notification takes the user. Selectors are internal constants. */
const TARGET_SELECTOR: Record<NotificationTarget, string> = {
  device: ".connect-card",
  files: ".media-list, .empty-state, .section-heading",
  destination: ".action-toolbar",
};

function readStoredNotifications() {
  try {
    return parseNotifications(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    // Storage can be unavailable (private mode, disabled). Notifications then
    // simply live for this session.
    return [];
  }
}

export default function App() {
  const [nativeAvailable, setNativeAvailable] = useState(false);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  /** Startup prerequisite check (tools/service/device/trust). */
  const [envReport, setEnvReport] = useState<EnvReport | null>(null);
  const [envChecking, setEnvChecking] = useState(false);
  const [envDismissed, setEnvDismissed] = useState(false);
  const [envError, setEnvError] = useState<string | null>(null);
  const [device, setDevice] = useState<DeviceSnapshot | null>(null);
  const [media, setMedia] = useState<MediaItem[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [destination, setDestination] = useState<string | null>(null);
  const [conflict, setConflict] = useState<ConflictPolicy>("skip");
  const [category, setCategory] = useState<Category>("All Media");
  const [sort, setSort] = useState<SortOrder>("newest");
  const [transfer, setTransfer] = useState<TransferState>(EMPTY_TRANSFER);
  const [mockState, setMockState] = useState<MockDeviceState>("ready");
  const [mockEnabled, setMockEnabled] = useState(false);
  const [previewItem, setPreviewItem] = useState<MediaItem | null>(null);
  /** Replace policy is armed but needs the user's explicit confirmation. */
  const [confirmReplace, setConfirmReplace] = useState<string[] | null>(null);
  /** Whether the pending Replace run is a "Transfer all new" batch. */
  const [pendingNewOnly, setPendingNewOnly] = useState(false);

  // ---- notifications -------------------------------------------------------
  const [notifications, dispatch] = useReducer(
    notificationReducer,
    undefined,
    readStoredNotifications,
  );
  const [panelOpen, setPanelOpen] = useState(false);
  const [filter, setFilter] = useState<NotificationFilter>("all");
  /** Panel-only category scoping; never persisted, so a restart shows everything. */
  const [categoryFilter, setCategoryFilter] = useState<NotificationCategoryFilter>("any");
  const [announcement, setAnnouncement] = useState("");
  const notifRootRef = useRef<HTMLDivElement>(null);
  const bellRef = useRef<HTMLButtonElement>(null);
  const announcedId = useRef<string | null>(null);
  const announceReady = useRef(false);
  const scanInFlight = useRef(false);
  /** Previous device state, so transitions (not every poll) raise a notice. */
  const lastDeviceState = useRef<DeviceState | null>(null);

  const unread = useMemo(() => unreadCount(notifications), [notifications]);
  const problem = useMemo(() => newestUnseenProblem(notifications), [notifications]);

  const notify = useCallback((input: NotificationInput) => {
    dispatch({ type: "add", input });
  }, []);

  // ---- derived display values ---------------------------------------------
  // Declared above the effects and callbacks that read them: a `const` is in
  // the temporal dead zone until its own line runs, so referencing one from an
  // earlier dependency array throws at render time.
  /** True only inside the Tauri shell, with a real IPC bridge available. */
  const native = nativeAvailable && isTauri();
  const deviceReady = device?.state === "ready";
  const udid = device?.device?.udid ?? null;
  /**
   * Setup gate: the desktop app opens on the setup screen until the
   * prerequisites pass (or the user explicitly continues). Demo mode and the
   * browser preview are exempt — they never touch real USB.
   */
  const showSetup = native && !device?.demo && !envDismissed;
  const connectionLabel = !native
    ? "Desktop bridge unavailable"
    : device?.demo
      ? "Demo device"
      : deviceReady
        ? "iPhone connected"
        : device?.state === "needs_trust"
          ? "Waiting for Trust"
          : device?.state === "no_device" || !device
            ? "No iPhone detected"
            : "Unavailable";

  /** Latest transfer handler, so listener registration never re-runs. */
  const handleTransferEventRef = useRef<(event: TransferEvent) => void>(() => undefined);

  /** Persist locally so the bell survives a restart. */
  useEffect(() => {
    try {
      window.localStorage.setItem(STORAGE_KEY, serializeNotifications(notifications));
    } catch {
      /* storage unavailable — keep them in memory only */
    }
  }, [notifications]);

  /**
   * Announce only genuinely new activity, and never re-announce on load: a
   * screen reader is told about the newest notification once, not about the
   * entire restored history every time the app starts.
   */
  useEffect(() => {
    if (!announceReady.current) {
      announceReady.current = true;
      announcedId.current = notifications[0]?.id ?? null;
      return;
    }
    const newest = notifications[0];
    if (!newest || announcedId.current === newest.id) return;
    announcedId.current = newest.id;
    setAnnouncement(`${newest.title}. ${newest.body ?? ""}`.trim());
  }, [notifications]);

  const closePanel = useCallback(() => {
    setPanelOpen(false);
    // Return focus to the control that opened the panel.
    bellRef.current?.focus();
  }, []);

  useCloseOnOutsidePointerDown(panelOpen, notifRootRef, closePanel);

  /** Read a notification and move to the resource it refers to. */
  const activateNotification = useCallback(
    (notification: AppNotification) => {
      dispatch({ type: "markRead", id: notification.id });
      const target = notification.target;
      if (!target) return;
      // The setup gate hides the main panels; step past it so the target exists.
      if (showSetup) setEnvDismissed(true);
      const focusTarget = () => {
        const element = document.querySelector<HTMLElement>(TARGET_SELECTOR[target]);
        if (!element) return;
        const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        element.scrollIntoView({ block: "center", behavior: reduceMotion ? "auto" : "smooth" });
        if (!element.hasAttribute("tabindex")) element.setAttribute("tabindex", "-1");
        element.focus({ preventScroll: true });
      };
      if (showSetup) {
        window.requestAnimationFrame(() => window.requestAnimationFrame(focusTarget));
      } else {
        focusTarget();
      }
      setPanelOpen(false);
    },
    [showSetup],
  );

  // ---- startup prerequisite check -----------------------------------------
  const runEnvCheck = useCallback(async () => {
    setEnvChecking(true);
    setEnvError(null);
    try {
      setEnvReport(await checkEnvironment());
    } catch (e) {
      const message = errorMessage(e);
      setEnvError(message);
      notify({
        severity: "error",
        category: "setup",
        title: "Could not check this computer",
        body: message,
        target: "device",
        dedupeKey: "setup:check-failed",
      });
    } finally {
      setEnvChecking(false);
    }
  }, [notify]);

  // ---- boot: diagnostics, initial status, event subscriptions -------------
  useEffect(() => {
    const available = isTauri();
    setNativeAvailable(available);
    if (!available) return;

    // Listeners are registered asynchronously, so cleanup cannot assume the
    // promise has resolved yet. Tracking that explicitly is what stops a
    // listener from outliving the effect: under <React.StrictMode> the effect
    // mounts, cleans up, and mounts again, and a late-resolving promise used to
    // push its unsubscribe into an array nobody would ever iterate — leaving
    // duplicate `device_status`/`transfer_event` handlers alive, so every
    // transfer event was applied twice.
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const track = (pending: Promise<() => void>) => {
      pending
        .then((unlisten) => {
          if (disposed) unlisten();
          else unlisteners.push(unlisten);
        })
        .catch(() => undefined);
    };

    (async () => {
      try {
        setDiag(await getDiagnostics());
      } catch (e) {
        notify({
          severity: "error",
          category: "setup",
          title: "USB toolchain could not be read",
          body: errorMessage(e),
          target: "device",
          dedupeKey: "setup:diagnostics",
        });
      }
      await runEnvCheck();
      try {
        setDevice(await getDeviceStatus());
      } catch {
        /* the poller will push a status shortly */
      }
    })();

    track(onDeviceStatus((snapshot) => setDevice(snapshot)));
    track(onTransferEvent((event) => handleTransferEventRef.current(event)));

    return () => {
      disposed = true;
      unlisteners.forEach((fn) => fn());
      unlisteners.length = 0;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---- environment report → notification -----------------------------------
  useEffect(() => {
    if (!envReport || envReport.status === "ready" || envReport.status === "device_not_connected") {
      return;
    }
    const body = [envReport.message, envReport.hint].filter(Boolean).join(" ");
    switch (envReport.status) {
      case "dependency_missing":
        notify({
          severity: "error",
          category: "setup",
          title: "USB support needs setup",
          body,
          target: "device",
          dedupeKey: "setup:dependency_missing",
        });
        break;
      case "service_not_running":
        notify({
          severity: "error",
          category: "setup",
          title: "iPhone connection service is not running",
          body,
          target: "device",
          dedupeKey: "setup:service_not_running",
        });
        break;
      case "permission_required":
        notify({
          severity: "error",
          category: "setup",
          title: "Permission needed for the connection service",
          body,
          target: "device",
          dedupeKey: "setup:permission_required",
        });
        break;
      case "pairing_required":
        notify({
          severity: "warning",
          category: "device",
          title: "Your iPhone is waiting for approval",
          body: "Unlock the iPhone and tap Trust, then check again.",
          target: "device",
          dedupeKey: "device:pairing_required",
        });
        break;
      case "unsupported_platform":
        notify({
          severity: "error",
          category: "setup",
          title: "This operating system is not supported yet",
          body,
          target: "device",
          dedupeKey: "setup:unsupported_platform",
        });
        break;
      default:
        notify({
          severity: "error",
          category: "setup",
          title: "The connection check needs attention",
          body,
          target: "device",
          dedupeKey: "setup:error",
        });
    }
  }, [envReport, notify]);

  // ---- device transitions → notification -----------------------------------
  useEffect(() => {
    if (!device) return;
    const previous = lastDeviceState.current;
    lastDeviceState.current = device.state;
    if (previous === device.state) return;
    // Nothing to report on the very first snapshot: the setup screen and the
    // device card already describe the current state.
    if (previous === null) return;

    switch (device.state) {
      case "ready":
        notify({
          severity: "success",
          category: "device",
          title: device.demo ? "Demo iPhone connected" : "iPhone connected",
          body: device.device?.name ?? device.device?.model ?? null,
          target: "device",
        });
        break;
      case "needs_trust":
        notify({
          severity: "warning",
          category: "device",
          title: "Approve this computer on your iPhone",
          body: device.hint ?? "Unlock the iPhone and tap Trust.",
          target: "device",
          dedupeKey: "device:needs_trust",
        });
        break;
      case "no_device":
        notify({
          severity: "warning",
          category: "device",
          title: device.demo ? "Demo iPhone disconnected" : "iPhone disconnected",
          body: "Reconnect the cable to continue.",
          target: "device",
        });
        break;
      case "unsupported":
      case "error":
        notify({
          severity: "error",
          category: "device",
          title: device.message || "The iPhone connection stopped",
          body: device.hint ?? null,
          target: "device",
          dedupeKey: `device:${device.state}`,
        });
        break;
      default:
        break;
    }
  }, [device, notify]);

  // ---- destination persistence (local only) --------------------------------
  useEffect(() => {
    try {
      setDestination(window.localStorage.getItem(DEST_KEY));
    } catch {
      /* storage unavailable */
    }
  }, []);

  const saveDestination = useCallback((path: string) => {
    setDestination(path);
    try {
      window.localStorage.setItem(DEST_KEY, path);
    } catch {
      /* storage unavailable */
    }
  }, []);

  // ---- transfer event handling --------------------------------------------
  const handleTransferEvent = useCallback(
    (event: TransferEvent) => {
      setTransfer((prev) => {
        switch (event.type) {
          case "started":
            return {
              ...EMPTY_TRANSFER,
              running: true,
              totalFiles: event.total_files ?? 0,
              totalBytes: event.total_bytes ?? null,
            };
          case "file_started":
            return {
              ...prev,
              currentName: event.name,
              currentIndex: event.index ?? 0,
            };
          case "progress":
            return { ...prev, progress: event };
          case "file_finished":
            return {
              ...prev,
              results: [
                ...prev.results,
                {
                  name: event.name,
                  outcome: event.outcome,
                  savedAs: event.saved_as,
                  note: event.note ?? null,
                  error: event.error?.message ?? null,
                },
              ],
              currentName: null,
              progress: null,
            };
          case "finished":
            return {
              ...prev,
              running: false,
              currentName: null,
              progress: null,
              finished: event,
            };
          default:
            return prev;
        }
      });

      // The outcome is recorded as a notification as well, so dismissing the
      // on-screen summary does not erase the fact that a transfer failed. The
      // mapping lives in the notification store so it stays unit-tested.
      if (event.type === "finished") {
        notify(transferOutcomeNotification(event));
      }
    },
    [notify],
  );

  // Keep the listener registration independent of this callback's identity.
  handleTransferEventRef.current = handleTransferEvent;

  // ---- scanning ------------------------------------------------------------
  const scan = useCallback(async () => {
    if (!isTauri() || scanInFlight.current) return;
    scanInFlight.current = true;
    setScanning(true);
    setScanError(null);
    try {
      const items = await listMedia();
      // Thumbnails are keyed by size + mtime, but a fresh scan may have
      // dropped files entirely — release them with the old listing.
      clearPreviews();
      setMedia(items);
      setSelected(new Set());
      notify({
        severity: "success",
        category: "scan",
        title:
          items.length === 0
            ? "No photos or videos found"
            : `Found ${items.length} accessible file${items.length === 1 ? "" : "s"}`,
        body:
          items.length === 0
            ? "iOS did not expose any media over USB. iCloud Photos may still be uploading."
            : null,
        target: "files",
      });
    } catch (e) {
      const message = errorMessage(e);
      setMedia([]);
      setScanError(message);
      notify({
        severity: "error",
        category: "scan",
        title: "Could not read your photos",
        body: message,
        target: "files",
        dedupeKey: "scan:error",
      });
    } finally {
      scanInFlight.current = false;
      setScanning(false);
    }
  }, [notify]);

  // Auto-scan when a device becomes ready; reset when it goes away.
  useEffect(() => {
    if (deviceReady && media === null && !scanning && native) {
      void scan();
    }
    if (!deviceReady && !scanning) {
      if (media !== null) setMedia(null);
      setSelected(new Set());
      // A previous scan failure describes a device that is no longer there, so
      // it must not linger with a "Try Again" that cannot succeed.
      setScanError(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deviceReady, device?.state, native]);

  // ---- filtering + sorting --------------------------------------------------
  // Sorting is applied here so "Oldest first" is exact: the backend lists
  // newest-first, and files without a date always stay last.
  const visible = useMemo(() => {
    const items = media ?? [];
    const filtered =
      category === "Photos"
        ? items.filter((m) => m.kind === "photo")
        : category === "Videos"
          ? items.filter((m) => m.kind === "video")
          : items.slice();
    const dated = filtered.filter((m) => m.modified_ms !== null);
    const undated = filtered.filter((m) => m.modified_ms === null);
    dated.sort((a, b) =>
      sort === "newest"
        ? (b.modified_ms ?? 0) - (a.modified_ms ?? 0)
        : (a.modified_ms ?? 0) - (b.modified_ms ?? 0),
    );
    return [...dated, ...undated];
  }, [media, category, sort]);

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAllVisible = () => {
    setSelected((prev) => {
      const next = new Set(prev);
      visible.forEach((m) => next.add(m.id));
      return next;
    });
  };

  const clearSelection = () => setSelected(new Set());

  // ---- actions -------------------------------------------------------------
  const chooseDestination = async () => {
    try {
      const path = await pickDestination();
      if (path) saveDestination(path);
    } catch (e) {
      notify({
        severity: "error",
        category: "destination",
        title: "Could not set the destination folder",
        body: errorMessage(e),
        target: "destination",
        dedupeKey: "destination:pick-failed",
      });
    }
  };

  const runTransfer = useCallback(
    async (ids: string[], policy: ConflictPolicy, newOnly: boolean) => {
      if (!destination || ids.length === 0) return;
      setTransfer({ ...EMPTY_TRANSFER, running: true, totalFiles: ids.length });
      try {
        await startTransfer(ids, destination, policy, newOnly);
      } catch (e) {
        setTransfer(EMPTY_TRANSFER);
        notify({
          severity: "error",
          category: "transfer",
          title: "Transfer could not start",
          body: errorMessage(e),
          target: "destination",
          dedupeKey: "transfer:start-failed",
        });
      }
    },
    [destination, notify],
  );

  /** Starts a transfer; the Replace policy first collects explicit consent. */
  const beginTransfer = (ids: string[], policy: ConflictPolicy, newOnly = false) => {
    if (!destination || ids.length === 0) return;
    if (policy === "replace") {
      setConfirmReplace(ids);
      setPendingNewOnly(newOnly);
      return;
    }
    void runTransfer(ids, policy, newOnly);
  };

  const confirmReplacedTransfer = () => {
    const ids = confirmReplace;
    const newOnly = pendingNewOnly;
    setConfirmReplace(null);
    if (ids) void runTransfer(ids, "replace", newOnly);
  };

  const selectedIds = visible.filter((m) => selected.has(m.id)).map((m) => m.id);
  const listBusy = scanning || transfer.running;
  const canTransfer =
    native &&
    deviceReady &&
    !scanning &&
    !transfer.running &&
    Boolean(destination) &&
    selectedIds.length > 0;
  const canTransferAll =
    native &&
    deviceReady &&
    !scanning &&
    !transfer.running &&
    Boolean(destination) &&
    (media?.length ?? 0) > 0;

  // Plain-language reasons for the disabled transfer buttons.
  const selectedDisabledReason = !native
    ? "Open the desktop app to transfer files."
    : !deviceReady
      ? "Connect and unlock your iPhone."
      : scanning
        ? "Wait for the scan to finish."
        : transfer.running
          ? "A transfer is already running."
          : !destination
            ? "Choose a destination folder."
            : selectedIds.length === 0
              ? "Select at least one file."
              : null;
  const allDisabledReason = !native
    ? "Open the desktop app to transfer files."
    : !deviceReady
      ? "Connect and unlock your iPhone."
      : scanning
        ? "Wait for the scan to finish."
        : transfer.running
          ? "A transfer is already running."
          : !destination
            ? "Choose a destination folder."
            : (media?.length ?? 0) === 0
              ? "Scan your iPhone for files first."
              : null;

  const cancelActiveTransfer = async () => {
    try {
      await cancelTransfer();
    } catch (e) {
      notify({
        severity: "error",
        category: "transfer",
        title: "Could not cancel the transfer",
        body: errorMessage(e),
        target: "files",
        dedupeKey: "transfer:cancel-failed",
      });
    }
  };

  const toggleMock = async () => {
    const next = !mockEnabled;
    try {
      await setMockMode(next);
      setMockEnabled(next);
      if (next) {
        setMockState("ready");
        await setMockDeviceState("ready");
      }
      setMedia(null);
    } catch (e) {
      notify({
        severity: "error",
        category: "setup",
        title: "Could not change demo mode",
        body: errorMessage(e),
        target: "device",
        dedupeKey: "setup:mock-toggle",
      });
    }
  };

  const changeMockState = async (value: MockDeviceState) => {
    try {
      await setMockDeviceState(value);
      setMockState(value);
      setMedia(null);
    } catch (e) {
      notify({
        severity: "error",
        category: "setup",
        title: "Could not change the demo device state",
        body: errorMessage(e),
        target: "device",
        dedupeKey: "setup:mock-state",
      });
    }
  };

  return (
    <main className="app-shell">
      <Sidebar
        category={category}
        onCategory={setCategory}
        deviceReady={deviceReady}
        demoDevice={Boolean(device?.demo)}
        diag={diag}
        mockEnabled={mockEnabled}
        mockState={mockState}
        onToggleMock={() => void toggleMock()}
        onMockState={(s) => void changeMockState(s)}
      />

      <section className="main-panel">
        <AppHeader
          connectionLabel={connectionLabel}
          connected={deviceReady}
          warn={device?.state === "needs_trust"}
          notificationCenter={
            <div className="notif-root" ref={notifRootRef}>
              <NotificationBell
                unread={unread}
                open={panelOpen}
                onToggle={() => setPanelOpen((open) => !open)}
                panelId={NOTIF_PANEL_ID}
                buttonRef={bellRef}
              />
              {panelOpen ? (
                <NotificationPanel
                  id={NOTIF_PANEL_ID}
                  notifications={notifications}
                  filter={filter}
                  onFilter={setFilter}
                  category={categoryFilter}
                  onCategory={setCategoryFilter}
                  onMarkRead={(id) => dispatch({ type: "markRead", id })}
                  onMarkUnread={(id) => dispatch({ type: "markUnread", id })}
                  onMarkAllRead={() => dispatch({ type: "markAllRead" })}
                  onDismiss={(id) => dispatch({ type: "dismiss", id })}
                  onClearAll={() => dispatch({ type: "clearAll" })}
                  onActivate={activateNotification}
                  onClose={closePanel}
                />
              ) : null}
            </div>
          }
        />

        {/* Screen-reader announcement for activity that is not otherwise
            announced. The visible panel is a dialog, so it is not a live
            region. */}
        <div className="sr-only" role="status" aria-live="polite">
          {announcement}
        </div>

        {device?.demo && (
          <div className="demo-banner">
            <strong>DEMO MODE</strong> — showing mock data on purpose. This is not a real iPhone.
          </div>
        )}
        {!native && (
          <div className="demo-banner">
            Browser preview — device detection and transfers are disabled. Use the desktop app.
          </div>
        )}

        {/* The newest unread problem, surfaced inline so a failure is visible
            without opening the panel. It is the same record the panel holds;
            dismissing it marks that record read.
            Suppressed while the setup screen is showing: that screen already
            explains the same state in more detail, and stacking a second red
            banner on top of it would just be the same fact three times. */}
        {problem && !showSetup && (
          <div className={`notice-banner ${problem.severity}`} role="alert">
            <Icon
              name={problem.severity === "error" ? "error" : "warning"}
              size="medium"
              className="notice-icon"
            />
            <div className="notice-copy">
              <strong>{problem.title}</strong>
              {problem.body ? <span>{problem.body}</span> : null}
            </div>
            <button
              type="button"
              className="ghost-button notice-dismiss"
              onClick={() => dispatch({ type: "markRead", id: problem.id })}
            >
              Dismiss
            </button>
          </div>
        )}

        {showSetup ? (
          <SetupScreen
            report={envReport}
            checking={envChecking}
            error={envError}
            onRecheck={() => void runEnvCheck()}
            onDismiss={() => setEnvDismissed(true)}
          />
        ) : (
          <>
        <DeviceCard
          native={native}
          scanning={scanning}
          busy={listBusy}
          device={device}
          diag={diag}
          onScan={() => void scan()}
        />

        <DestinationToolbar
          destination={destination}
          conflict={conflict}
          running={transfer.running}
          selectedCount={selectedIds.length}
          canTransfer={canTransfer}
          canTransferAll={canTransferAll}
          selectedDisabledReason={selectedDisabledReason}
          allDisabledReason={allDisabledReason}
          onChangeDestination={() => void chooseDestination()}
          onConflict={setConflict}
          onTransferSelected={() => beginTransfer(selectedIds, conflict)}
          onTransferAll={() =>
            beginTransfer(
              (media ?? []).map((m) => m.id),
              conflict,
              true,
            )
          }
        />

        <FileList
          media={media}
          visible={visible}
          category={category}
          selected={selected}
          udid={udid}
          scanning={scanning}
          transferRunning={transfer.running}
          deviceReady={deviceReady}
          scanError={scanError}
          sort={sort}
          onSort={setSort}
          onScan={() => void scan()}
          onToggle={toggle}
          onSelectAll={selectAllVisible}
          onClear={clearSelection}
          onPreview={setPreviewItem}
        />

        {previewItem ? (
          <PreviewDialog
            item={previewItem}
            udid={udid}
            onClose={() => setPreviewItem(null)}
          />
        ) : null}

        {confirmReplace ? (
          <ConfirmDialog
            count={confirmReplace.length}
            onConfirm={confirmReplacedTransfer}
            onCancel={() => setConfirmReplace(null)}
          />
        ) : null}

        {transfer.running && (
          <TransferProgress transfer={transfer} onCancel={() => void cancelActiveTransfer()} />
        )}

        {!transfer.running && transfer.finished && (
          <TransferSummary
            finished={transfer.finished}
            onDismiss={() => setTransfer(EMPTY_TRANSFER)}
          />
        )}

        <div className="prototype-note">
          {diag
            ? `Platform: ${diag.os}${
                diag.toolchain_ready
                  ? " · toolchain ready"
                  : ` · missing: ${diag.missing_tools.join(", ")}`
              }`
            : "All files stay on this computer."}
        </div>
          </>
        )}
      </section>
    </main>
  );
}
