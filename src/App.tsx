import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import type {
  Category,
  ConflictPolicy,
  DeviceSnapshot,
  Diagnostics,
  EnvReport,
  MediaItem,
  MockDeviceState,
  SortOrder,
  TransferEvent,
} from "./types";

const DEST_KEY = "lit-destination";

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
  const [notice, setNotice] = useState<{ kind: "info" | "error"; text: string } | null>(null);
  const [mockState, setMockState] = useState<MockDeviceState>("ready");
  const [mockEnabled, setMockEnabled] = useState(false);
  const [previewItem, setPreviewItem] = useState<MediaItem | null>(null);
  /** Replace policy is armed but needs the user's explicit confirmation. */
  const [confirmReplace, setConfirmReplace] = useState<string[] | null>(null);
  /** Whether the pending Replace run is a "Transfer all new" batch. */
  const [pendingNewOnly, setPendingNewOnly] = useState(false);
  const scanInFlight = useRef(false);

  const native = nativeAvailable && isTauri();

  // ---- startup prerequisite check -----------------------------------------
  const runEnvCheck = useCallback(async () => {
    setEnvChecking(true);
    setEnvError(null);
    try {
      setEnvReport(await checkEnvironment());
    } catch (e) {
      setEnvError(errorMessage(e));
    } finally {
      setEnvChecking(false);
    }
  }, []);

  // ---- boot: diagnostics, initial status, event subscriptions -------------
  useEffect(() => {
    let dispose: Array<() => void> = [];
    const available = isTauri();
    setNativeAvailable(available);
    if (!available) return;

    (async () => {
      try {
        setDiag(await getDiagnostics());
      } catch (e) {
        setNotice({ kind: "error", text: errorMessage(e) });
      }
      // The setup screen owns the first-run experience: check prerequisites
      // before bothering with a device scan.
      await runEnvCheck();
      try {
        setDevice(await getDeviceStatus());
      } catch {
        /* the poller will push a status shortly */
      }
    })();

    onDeviceStatus((snapshot) => setDevice(snapshot))
      .then((unlisten) => dispose.push(unlisten))
      .catch(() => undefined);
    onTransferEvent(handleTransferEvent)
      .then((unlisten) => dispose.push(unlisten))
      .catch(() => undefined);

    return () => {
      dispose.forEach((fn) => fn());
      dispose = [];
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
  const handleTransferEvent = useCallback((event: TransferEvent) => {
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
  }, []);

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
    } catch (e) {
      setMedia([]);
      setScanError(errorMessage(e));
    } finally {
      scanInFlight.current = false;
      setScanning(false);
    }
  }, []);

  const deviceReady = device?.state === "ready";
  const udid = device?.device?.udid ?? null;

  // Auto-scan when a device becomes ready; reset when it goes away.
  useEffect(() => {
    if (deviceReady && media === null && !scanning && native) {
      void scan();
    }
    if (!deviceReady && !scanning) {
      if (media !== null) setMedia(null);
      setSelected(new Set());
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
      setNotice({ kind: "error", text: errorMessage(e) });
    }
  };

  const runTransfer = useCallback(
    async (ids: string[], policy: ConflictPolicy, newOnly: boolean) => {
      if (!destination || ids.length === 0) return;
      setNotice(null);
      setTransfer({ ...EMPTY_TRANSFER, running: true, totalFiles: ids.length });
      try {
        await startTransfer(ids, destination, policy, newOnly);
      } catch (e) {
        setTransfer(EMPTY_TRANSFER);
        setNotice({ kind: "error", text: errorMessage(e) });
      }
    },
    [destination],
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
      setNotice({ kind: "error", text: errorMessage(e) });
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
      setNotice({ kind: "error", text: errorMessage(e) });
    }
  };

  const changeMockState = async (value: MockDeviceState) => {
    try {
      await setMockDeviceState(value);
      setMockState(value);
      setMedia(null);
    } catch (e) {
      setNotice({ kind: "error", text: errorMessage(e) });
    }
  };

  // ---- derived display values ---------------------------------------------
  // Setup gate: the desktop app opens on the setup screen until the
  // prerequisites pass (or the user explicitly continues). Demo mode and the
  // browser preview are exempt — they never touch real USB.
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
        />

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
        {notice && (
          <div className={`notice-banner ${notice.kind}`}>
            <span>{notice.text}</span>
            <button type="button" onClick={() => setNotice(null)} aria-label="Dismiss">
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
