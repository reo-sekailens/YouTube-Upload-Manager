import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { WorkspaceTab } from "./components/AppWorkspaceParts";
import {
  cancelPreflightDuplicateScan,
  cancelUploadItem,
  checkUploadTitleDuplicates,
  clearUploadQueue,
  completeStartupAfterSafeShell,
  deleteUploadedSource,
  exitApplication,
  deletePreflightDuplicateFile,
  ignoreDuplicateCandidate,
  importAndQueueBatch,
  isTauri,
  loadConnectionSettings,
  loadPreflightDuplicateFileMetadata,
  loadPreflightDuplicateScan,
  loadSnapshot,
  loadStartupBootstrap,
  preflightDuplicateFiles,
  preparePreflightLocalDeleteFile,
  queueItem,
  reAuditIgnoredDuplicateCandidates,
  resolveUploadTitleDuplicates,
  setItemDeleteSourceAfterUpload,
  setItemVisibility,
  syncChannelInventory,
  recordWebviewError,
} from "./lib/local";
import type {
  DashboardSnapshot,
  ManualUploadSettings,
  PreIngestDuplicateScan,
  StartupBootstrap,
  StartupReadiness,
  UploadItem,
  UploadVisibility,
} from "./lib/types";
import type { ConnectionSettings } from "./lib/types";
import type {
  DedupeActivityState,
  DedupeActivityEntry,
  DedupeProgressPhase,
} from "./lib/dedupe-activity";
import { resolveTheme, type AppTheme } from "./lib/theme";
import { canAcceptChannelSnapshot } from "./lib/channel-snapshot";
import type { OperatorAction } from "./lib/operator-feedback";

const emptySnapshot: DashboardSnapshot = {
  revision: 0,
  items: [],
  duplicates: [],
  pendingTitleDuplicates: [],
};
const pluralS = (count: number) => (count === 1 ? "" : "s");
const secondaryButtonClass = "ui-button-secondary";
const primaryButtonClass = "ui-button-primary";
const eyebrowClass = "ui-eyebrow";
const appShellClass = "ui-shell";
const topbarClass = "ui-topbar";
const brandLockupClass = "ui-brand-lockup";
const titleClass = "ui-title";
const subtleClass = "ui-subtle";
const panelClass = "ui-panel";
const sectionHeadingClass = "ui-section-heading";
const actionRowClass = "ui-action-row";

const QueueToolbar = lazy(() => import("./components/QueueToolbar"));
const DesktopRuntimeIntegration = lazy(() => import("./components/DesktopRuntimeIntegration"));

const ConnectionPanel = lazy(() => import("./components/ConnectionPanel"));
const CrashRecoveryScreen = lazy(() => import("./components/CrashRecoveryScreen"));
const AppDialogs = lazy(() => import("./components/AppWorkspaceParts").then((module) => ({ default: module.AppDialogs })));
let prefetchedNavigation: typeof import("./components/AppWorkspaceParts")["WorkspaceNavigation"] | undefined;
const loadBatchImportArea = () => import("./components/AppWorkspaceParts").then((module) => {
  prefetchedNavigation = module.WorkspaceNavigation;
  return { default: module.BatchImportArea };
});
const BatchImportArea = lazy(loadBatchImportArea);
const WorkspaceHeader = lazy(() => import("./components/AppWorkspaceParts").then((module) => ({ default: module.WorkspaceHeader })));
const WorkspaceFooter = lazy(() => import("./components/AppWorkspaceParts").then((module) => ({ default: module.WorkspaceFooter })));
const BrowserPreviewNotice = lazy(() => import("./components/AppWorkspaceParts").then((module) => ({ default: module.BrowserPreviewNotice })));
const LazyWorkspaceNavigation = lazy(() => import("./components/AppWorkspaceParts").then((module) => ({ default: module.WorkspaceNavigation })));
function WorkspaceNavigation(props: Parameters<NonNullable<typeof prefetchedNavigation>>[0]) {
  const Navigation = prefetchedNavigation ?? LazyWorkspaceNavigation;
  return <Navigation {...props} />;
}
const loadQueueTable = () => import("./components/UploadQueueWorkspace");
const QueueTable = lazy(loadQueueTable);
const loadManualUploadDefaultsPanel = () =>
  import("./components/BatchPreparationPanel");
const BatchPreparationPanel = lazy(loadManualUploadDefaultsPanel);
const loadUploadProgressSummary = () =>
  import("./components/UploadProgressSummary").then((module) => ({
    default: module.UploadProgressSummary,
  }));
const UploadProgressSummary = lazy(loadUploadProgressSummary);
const loadUploadTitleDuplicateReview = () =>
  import("./components/UploadTitleDuplicateReview").then((module) => ({
    default: module.UploadTitleDuplicateReview,
  }));
const UploadTitleDuplicateReview = lazy(loadUploadTitleDuplicateReview);

export function prefetchBatchWorkspace() {
  return Promise.all([
    loadQueueTable(),
    loadManualUploadDefaultsPanel(),
    loadUploadProgressSummary(),
    loadUploadTitleDuplicateReview(),
    loadBatchImportArea(),
  ]);
}
const DeletionReview = lazy(() =>
  import("./components/DeletionReview").then((module) => ({
    default: module.DeletionReview,
  })),
);
const VideoTitleRename = lazy(() => import("./components/VideoTitleRename"));
const PlaylistManager = lazy(() => import("./components/PlaylistManager"));
const DedupeActivityPanel = lazy(() =>
  import("./components/DedupeActivityPanel").then((module) => ({
    default: module.DedupeActivityPanel,
  })),
);
const DiagnosticsPanel = lazy(() =>
  import("./components/DiagnosticsPanel").then((module) => ({
    default: module.DiagnosticsPanel,
  })),
);
const DuplicateReview = lazy(() => import("./components/DuplicateReview"));
const FolderMonitorPanel = lazy(() => import("./components/FolderMonitorPanel"));
const PreIngestDuplicatePanel = lazy(() => import("./components/PreIngestDuplicatePanel"));
const TransferPanel = lazy(() =>
  import("./components/TransferPanel").then((module) => ({
    default: module.TransferPanel,
  })),
);
const RevisionedStateBridge = lazy(
  () => import("./lib/revisioned-state-bridge"),
);

function WorkspacePending() {
  return (
    <p className="m-0 text-sm text-muted" role="status">
      Loading workspace…
    </p>
  );
}

function SafeStartupShell({
  failure,
  readiness,
}: {
  failure?: string;
  readiness?: StartupReadiness;
}) {
  const detail =
    failure ??
    readiness?.detail ??
    "Checking saved device-local work before upload controls are enabled.";
  return (
    <main className={appShellClass} data-performance-shell="holding">
      <header className={topbarClass}>
        <div className={brandLockupClass}>
          <img className="ui-brand-mark" src="/favicon.svg" alt="" />
          <div>
            <p className={eyebrowClass}>SAFE STARTUP</p>
            <h1 className={titleClass}>YouTube Upload Manager</h1>
            <p className={subtleClass}>Your saved work remains on this device.</p>
          </div>
        </div>
      </header>
      <section
        aria-busy={!failure}
        aria-live="polite"
        className="ui-safe-panel"
        role="status"
      >
        <p className={eyebrowClass}>
          {failure ? "STARTUP NEEDS ATTENTION" : "RECOVERING LOCAL WORK"}
        </p>
        <h2>{failure ? "The workspace is still locked" : "Preparing your workspace…"}</h2>
        <p>{detail}</p>
        <p className={subtleClass}>
          Upload and queue actions stay unavailable until interrupted work has
          been safely classified.
        </p>
      </section>
    </main>
  );
}

function operatorErrorMessage(error: unknown, fallback: string) {
  return error instanceof Error
    ? error.message
    : typeof error === "string" && error.trim()
      ? error
      : fallback;
}

export default function App({
  initialStartup,
}: {
  /** Test/preview seam; production startup always begins with the safe shell. */
  initialStartup?: StartupBootstrap;
} = {}) {
  const [theme, setTheme] = useState<AppTheme>(() => {
    try {
      return resolveTheme(
        window.localStorage.getItem("appearance-theme"),
        window.matchMedia("(prefers-color-scheme: dark)").matches,
      );
    } catch {
      return "light";
    }
  });
  const [startup, setStartup] = useState<StartupBootstrap | undefined>(
    initialStartup,
  );
  const [startupFailure, setStartupFailure] = useState<string>();
  const [snapshot, setSnapshot] = useState<DashboardSnapshot>(
    initialStartup?.snapshot ?? emptySnapshot,
  );
  const [notice, setNotice] = useState(
    "Local queue ready. Original files upload directly to YouTube.",
  );
  const [busy, setBusy] = useState(false);
  const [dedupeBusy, setDedupeBusy] = useState(false);
  const [dedupeActivity, setDedupeActivity] = useState<DedupeActivityEntry[]>(
    [],
  );
  const [dedupePhase, setDedupePhase] = useState<DedupeProgressPhase>("idle");
  const [libraryRefreshVersion, setLibraryRefreshVersion] = useState(0);
  const [dropActive, setDropActive] = useState(false);
  const [preflightDropActive, setPreflightDropActive] = useState(false);
  const [preflightBusy, setPreflightBusy] = useState(false);
  const [preflightFileCount, setPreflightFileCount] = useState(0);
  const [preflightScan, setPreflightScan] = useState<PreIngestDuplicateScan>();
  const [pendingImportPaths, setPendingImportPaths] = useState<string[]>();
  const [activeTab, setActiveTab] = useState<WorkspaceTab>("batch");
  const [connectionSettings, setConnectionSettings] =
    useState<ConnectionSettings | undefined>(initialStartup?.connection);
  const [setupDismissed, setSetupDismissed] = useState(false);
  const [crashRecovery, setCrashRecovery] = useState(
    initialStartup?.crashRecovery,
  );
  const [exitConfirmationOpen, setExitConfirmationOpen] = useState(false);
  const dedupeActivityId = useRef(0);
  const preflightRunId = useRef(0);
  const stateRevisionRef = useRef(initialStartup?.snapshot.revision ?? 0);
  const stateChannelIdRef = useRef(initialStartup?.snapshot.activeChannelId);
  const stateChannelGenerationRef = useRef(0);
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;
  const finalizedPreflightJobs = useRef(new Set<string>());
  const startupCompletionRequest = useRef<
    Promise<StartupBootstrap> | undefined
  >(undefined);
  const startupReady = Boolean(
    startup?.readiness.classificationComplete &&
      startup.readiness.safeShellRendered &&
      startup.readiness.queueActionsEnabled,
  );
  const recoveryModeRef = useRef(false);
  recoveryModeRef.current = Boolean(crashRecovery?.crashDetected || !startupReady);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
    try {
      window.localStorage.setItem("appearance-theme", theme);
    } catch {
      // Appearance is a convenience preference; storage failures are harmless.
    }
  }, [theme]);

  const updateConnection = useCallback((settings: ConnectionSettings) => {
    const channelChanged = stateChannelIdRef.current !== settings.activeChannelId;
    if (channelChanged) {
      stateChannelGenerationRef.current += 1;
      stateChannelIdRef.current = settings.activeChannelId;
      stateRevisionRef.current = 0;
      preflightRunId.current += 1;
      finalizedPreflightJobs.current.clear();
      setPreflightScan(undefined);
      setPreflightFileCount(0);
      setPreflightBusy(false);
      setPendingImportPaths(undefined);
      setDedupeActivity([]);
      setDedupePhase("idle");
      snapshotRef.current = {
        ...emptySnapshot,
        activeChannel: settings.activeChannel,
        activeChannelId: settings.activeChannelId,
      };
    }
    setConnectionSettings(settings);
    setSnapshot((current) => {
      if (current.activeChannelId === settings.activeChannelId) {
        return {
          ...current,
          activeChannel: settings.activeChannel,
        };
      }
      return {
        ...emptySnapshot,
        activeChannel: settings.activeChannel,
        activeChannelId: settings.activeChannelId,
      };
    });
    if (isTauri) {
      const requested = {
        channelId: settings.activeChannelId,
        generation: stateChannelGenerationRef.current,
        revision: stateRevisionRef.current,
      };
      void loadSnapshot()
        .then((next) => {
          if (!canAcceptChannelSnapshot(requested, {
            channelId: stateChannelIdRef.current,
            generation: stateChannelGenerationRef.current,
            revision: stateRevisionRef.current,
          }, next)) return;
          stateRevisionRef.current = next.revision;
          snapshotRef.current = next;
          setSnapshot(next);
        })
        .catch(() => undefined);
    }
  }, []);

  const refresh = useCallback(async () => {
    const requested = {
      channelId: stateChannelIdRef.current,
      generation: stateChannelGenerationRef.current,
      revision: stateRevisionRef.current,
    };
    const next = await loadSnapshot();
    if (!canAcceptChannelSnapshot(requested, {
      channelId: stateChannelIdRef.current,
      generation: stateChannelGenerationRef.current,
      revision: stateRevisionRef.current,
    }, next)) return snapshotRef.current;
    stateRevisionRef.current = next.revision;
    snapshotRef.current = next;
    setSnapshot(next);
    return next;
  }, []);

  const refreshQueue = useCallback(async () => {
    await refresh();
  }, [refresh]);

  const reportOperatorError = useCallback(async (error: unknown, action: OperatorAction) => {
    const { operatorFailureMessage } = await import("./lib/operator-feedback");
    const message = operatorFailureMessage(error, action);
    setNotice(message);
    return message;
  }, []);

  const updateSnapshotItem = useCallback((updated: UploadItem) => {
    setSnapshot((current) => ({
      ...current,
      items: current.items.map((item) =>
        item.id === updated.id ? updated : item,
      ),
    }));
  }, []);

  const applyStartup = useCallback((next: StartupBootstrap) => {
    setStartup(next);
    stateChannelIdRef.current = next.snapshot.activeChannelId;
    stateRevisionRef.current = next.snapshot.revision;
    setSnapshot(next.snapshot);
    setConnectionSettings(next.connection);
    setCrashRecovery(next.crashRecovery);
  }, []);


  const logDedupeActivity = useCallback(
    (state: DedupeActivityState, message: string) => {
      dedupeActivityId.current += 1;
      setDedupeActivity((entries) =>
        [...entries, {
          id: dedupeActivityId.current,
          state,
          message,
        }].slice(-8),
      );
    },
    [],
  );

  useEffect(() => {
    if (startup) return;
    let active = true;
    void loadStartupBootstrap()
      .then((next) => {
        if (!active) return;
        applyStartup(next);
      })
      .catch((error) => {
        if (!active) return;
        setStartupFailure(
          operatorErrorMessage(
            error,
            "Startup state could not be read. No upload controls were enabled.",
          ),
        );
      });
    return () => {
      active = false;
    };
  }, [applyStartup, startup]);

  useEffect(() => {
    if (!startup || startupReady) return;
    let active = true;
    let innerFrame = 0;
    const outerFrame = window.requestAnimationFrame(() => {
      // Fetch and parse only the active Batch code after the safe shell has had
      // a frame. Components remain unmounted and cannot run effects or invokes.
      void prefetchBatchWorkspace().catch(() => undefined);
      innerFrame = window.requestAnimationFrame(() => {
        if (document.querySelector("[data-performance-shell]"))
          performance.mark("[data-performance-shell]");
        const request =
          startupCompletionRequest.current ??
          (import.meta.env.TAURI_ENV_PERFORMANCE_HARNESS === "1"
            ? import("./performance-harness").then(
                ({ completeStartupAfterSafeShellPaint }) =>
                  completeStartupAfterSafeShellPaint(
                    completeStartupAfterSafeShell,
                  ),
              )
            : completeStartupAfterSafeShell());
        startupCompletionRequest.current = request;
        void request
          .then((next) => {
            if (!active) return;
            applyStartup(next);
            setStartupFailure(undefined);
          })
          .catch((error) => {
            if (!active) return;
            setStartupFailure(
              operatorErrorMessage(
                error,
                "Saved work could not be recovered. Upload controls remain locked.",
              ),
            );
          });
      });
    });
    return () => {
      active = false;
      window.cancelAnimationFrame(outerFrame);
      if (innerFrame) window.cancelAnimationFrame(innerFrame);
    };
  }, [applyStartup, startup, startupReady]);

  useEffect(() => {
    const enterRecovery = (failureKind: string) => {
      void recordWebviewError();
      setCrashRecovery({ crashDetected: true, failureKind });
    };
    const onUnhandledRejection = () => enterRecovery("Unhandled promise rejection");
    const onWebviewError = () => enterRecovery("Webview error");
    window.addEventListener("error", onWebviewError);
    window.addEventListener("unhandledrejection", onUnhandledRejection);
    return () => {
      window.removeEventListener("error", onWebviewError);
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
    };
  }, []);

  const reviewImport = useCallback((paths: string[]) => {
    const videos = paths.filter((path) =>
      /\.(?:3g[2p]|avi|flv|m(?:4v|kv|ov|p(?:4|e?g))|webm|wmv)$/i.test(path),
    );
    if (videos.length === 0) {
      setNotice(
        "Drop a supported video file: MP4, MOV, MKV, WebM, AVI, or another supported video format.",
      );
      return;
    }
    setPendingImportPaths(videos);
  }, []);

  const runPreflightDuplicateCheck = useCallback(
    async (paths: string[], mode: "light" | "deep" = "light") => {
      if (paths.length === 0) return;
      const runId = preflightRunId.current + 1;
      preflightRunId.current = runId;
      setPreflightBusy(true);
      setPreflightFileCount(paths.length);
      setPreflightScan(undefined);
      try {
        const scan = await preflightDuplicateFiles(paths, mode);
        if (runId !== preflightRunId.current) return;
        setPreflightScan(scan);
        setNotice(
          mode === "light"
            ? `Fast filename match started for ${paths.length} file${pluralS(paths.length)}.`
            : `Deep BLAKE3 match started for ${paths.length} file${pluralS(paths.length)}.`,
        );
      } catch (error) {
        if (runId !== preflightRunId.current) return;
        await reportOperatorError(error, "preflight");
      } finally {
        if (runId === preflightRunId.current) setPreflightBusy(false);
      }
    },
    [],
  );

  const loadFinalPreflightResult = useCallback(async (jobId: string) => {
    if (finalizedPreflightJobs.current.has(jobId)) return;
    finalizedPreflightJobs.current.add(jobId);
    const generation = stateChannelGenerationRef.current;
    try {
      const scan = await loadPreflightDuplicateScan(jobId);
      if (scan.id !== jobId || generation !== stateChannelGenerationRef.current) return;
      setPreflightScan((current) => current?.id === jobId ? scan : current);
      setPreflightFileCount(
        Math.max(0, scan.totalFiles - scan.completedFiles),
      );
      if (scan.status === "complete") {
        const matches = scan.matchedFiles ?? scan.files.filter(
          (file) =>
            file.localMatches.length > 0 ||
            file.droppedDuplicateFileNames.length > 0 ||
            file.uploadedTitleMatches.length > 0,
        ).length;
        setNotice(
          `${scan.completedFiles} file${pluralS(scan.completedFiles)} checked before ingest. ${matches} need${matches === 1 ? "s" : ""} duplicate review.`,
        );
      }
    } catch {
      finalizedPreflightJobs.current.delete(jobId);
    }
  }, []);

  const loadPreflightPage = useCallback(async (
    kind: "files" | "activity",
    page: number,
  ) => {
    const current = preflightScan;
    if (!current) return;
    const runId = preflightRunId.current;
    const size = 48;
    const scan = await loadPreflightDuplicateScan(current.id, {
      fileOffset: kind === "files" ? (page - 1) * size : current.fileOffset ?? 0,
      fileLimit: size,
      activityOffset:
        kind === "activity" ? (page - 1) * size : current.activityOffset ?? 0,
      activityLimit: size,
    });
    if (runId !== preflightRunId.current || scan.id !== current.id) return;
    setPreflightScan((latest) => latest?.id === current.id ? scan : latest);
  }, [preflightScan]);

  const deleteLocalDuplicate = useCallback(
    async (token: string, confirmation: string, ordinal: number) => {
      setPreflightBusy(true);
      try {
        await deletePreflightDuplicateFile(token, confirmation);
        setPreflightScan(
          (current) =>
            current && {
              ...current,
              files: current.files.filter((file) => file.ordinal !== ordinal),
            },
        );
        setNotice(
          `Deleted “${confirmation}” from this device. YouTube videos were not changed.`,
        );
      } finally {
        setPreflightBusy(false);
      }
    },
    [],
  );

  const prepareLocalDuplicateDelete = useCallback(
    async (jobId: string, ordinal: number) => {
      setPreflightBusy(true);
      try {
        return await preparePreflightLocalDeleteFile(jobId, ordinal);
      } finally {
        setPreflightBusy(false);
      }
    },
    [],
  );

  const cancelPreflight = useCallback(async () => {
    if (!preflightScan || preflightScan.status === "complete") return;
    try {
      await cancelPreflightDuplicateScan(preflightScan.id);
      setPreflightScan(
        (current) => current?.id === preflightScan.id ? { ...current, status: "cancelled" } : current,
      );
      setNotice(
        "The pre-ingest duplicate job was cancelled. No selected file was ingested or uploaded.",
      );
    } catch (error) {
      await reportOperatorError(error, "cancelPreflight");
    }
  }, [preflightScan]);

  const clearUploads = useCallback(async () => {
    setBusy(true);
    try {
      const cleared = await clearUploadQueue();
      await refresh();
      setNotice(
        `${cleared} local upload job${cleared === 1 ? " was" : "s were"} removed from this queue. Local records, BLAKE3 hashes, and original files were retained.`,
      );
    } catch (error) {
      await reportOperatorError(error, "clearQueue");
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const cancelUpload = useCallback(async (item: UploadItem) => {
    setBusy(true);
    try {
      await cancelUploadItem(item.id);
      await refresh();
      setNotice(`Removed “${item.title}” from the upload queue. Its original file was retained.`);
    } catch (error) {
      await reportOperatorError(error, "cancelUpload");
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const importVideos = useCallback(
    async (paths: string[], settings: ManualUploadSettings) => {
      setBusy(true);
      setNotice(`Checking ${paths.length} original video${pluralS(paths.length)} for the saved queue. Keep them available and unchanged until upload or recovery completes.`);
      try {
        const receipt = await importAndQueueBatch(paths, settings);
        await refresh();
        if (receipt.importedCount === 0) {
          const firstFailure = receipt.items.find(
            (item) => item.status === "failed",
          )?.detail;
          setNotice(
            `${receipt.failedCount} video${pluralS(receipt.failedCount)} could not be imported. ${firstFailure ?? ""}`.trim(),
          );
          return;
        }
        if (receipt.detail) {
          setNotice(
            `${receipt.importedCount} original video${pluralS(receipt.importedCount)} added to the saved queue. ${receipt.detail}`,
          );
          return;
        }
        const importNotice = receipt.duplicateCount > 0
          ? `${receipt.importedCount} original video${pluralS(receipt.importedCount)} saved as source references. ${receipt.duplicateCount} light duplicate match${receipt.duplicateCount === 1 ? " needs" : "es need"} your decision.`
          : receipt.queuedCount > 0
            ? `${receipt.importedCount} original video${pluralS(receipt.importedCount)} saved as source references and ${receipt.queuedCount} queued.`
            : `${receipt.importedCount} original video${pluralS(receipt.importedCount)} saved as source references. Connect YouTube to start uploading them.`;
        const firstFailure = receipt.items.find(
          (item) => item.status === "failed",
        )?.detail;
        const failuresNotice = receipt.failedCount > 0
          ? ` ${receipt.failedCount} item${pluralS(receipt.failedCount)} need attention.${firstFailure ? ` ${firstFailure}` : ""}`
          : "";
        setNotice(
          `${importNotice}${receipt.queuedCount > 0 ? " Uploads start when capacity and limits allow." : ""}${failuresNotice}`.trim(),
        );
      } catch (error) {
        await reportOperatorError(error, "import");
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) =>
        getCurrentWindow().onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          if (activeTab === "dedupe") setPreflightDropActive(true);
          else setDropActive(true);
        } else if (event.payload.type === "leave") {
          setDropActive(false);
          setPreflightDropActive(false);
        } else if (event.payload.type === "drop") {
          setDropActive(false);
          setPreflightDropActive(false);
          if (activeTab === "dedupe")
            void runPreflightDuplicateCheck(event.payload.paths);
          else reviewImport(event.payload.paths);
        }
        }),
      )
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // Mobile uses the native document picker rather than desktop drag-and-drop.
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [activeTab, reviewImport, runPreflightDuplicateCheck]);

  const confirmExit = useCallback(async () => {
    try {
      setExitConfirmationOpen(false);
      await exitApplication();
    } catch {
      setNotice("The app could not close. Your saved queue remains unchanged.");
    }
  }, []);

  const addVideo = useCallback(async () => {
    if (!isTauri) {
      setNotice(
        "Run this screen through Tauri to select original files for the saved upload queue.",
      );
      return;
    }
    try {
      const { selectVideoFiles } = await import("./lib/video-picker");
      reviewImport(await selectVideoFiles());
    } catch (error) {
      await reportOperatorError(error, "videoPicker");
    }
  }, [reviewImport]);

  const queue = useCallback(async (item: UploadItem) => {
    setBusy(true);
    try {
      const duplicates = await checkUploadTitleDuplicates([item.id]);
      if (duplicates.length > 0) {
        await refresh();
        setNotice(
          "A light duplicate match was found. Choose Upload anyway or Skip duplicate below.",
        );
        return;
      }
      const updated = await queueItem(item.id);
      updateSnapshotItem(updated);
      setNotice(
        `${updated.fileName} is queued. Uploads start when capacity and limits allow.`,
      );
    } catch (error) {
      await reportOperatorError(error, "queue");
    } finally {
      setBusy(false);
    }
  }, [refresh, updateSnapshotItem]);

  const chooseFilesForPreflight = useCallback(async (mode: "light" | "deep") => {
    if (!isTauri) {
      setNotice("Run this screen through Tauri to check files before ingest.");
      return;
    }
    try {
      const { selectPreflightFiles } = await import("./lib/video-picker");
      await runPreflightDuplicateCheck(await selectPreflightFiles(), mode);
    } catch (error) {
      await reportOperatorError(error, "preflightPicker");
    }
  }, [runPreflightDuplicateCheck]);

  const resolveTitleDuplicates = useCallback(async (
    itemIds: string[],
    action: "ignore" | "skip",
  ) => {
    setBusy(true);
    try {
      const resolved = await resolveUploadTitleDuplicates(itemIds, action);
      const ignored = resolved.filter((item) => item.status !== "cancelled");
      for (const item of ignored) await queueItem(item.id);
      await refresh();
      setNotice(
        action === "ignore"
          ? `${ignored.length} title-matched video${ignored.length === 1 ? " is" : "s are"} queued to upload.`
          : `${resolved.length} duplicate video${resolved.length === 1 ? " was" : "s were"} skipped. The local files were not changed.`,
      );
    } catch (error) {
      await reportOperatorError(error, "duplicateDecision");
      await refresh();
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const changeVisibility = useCallback(async (
    item: UploadItem,
    visibility: UploadVisibility,
  ) => {
    setBusy(true);
    try {
      const updated = await setItemVisibility(item.id, visibility);
      updateSnapshotItem(updated);
      setNotice(`${updated.fileName} will upload as ${visibility}.`);
    } catch (error) {
      await reportOperatorError(error, "visibility");
    } finally {
      setBusy(false);
    }
  }, [updateSnapshotItem]);

  const changeSourceCleanup = useCallback(async (
    item: UploadItem,
    deleteSourceAfterUpload: boolean,
  ) => {
    setBusy(true);
    try {
      const updated = await setItemDeleteSourceAfterUpload(
        item.id,
        deleteSourceAfterUpload,
      );
      updateSnapshotItem(updated);
      setNotice(
        deleteSourceAfterUpload
          ? `${updated.fileName} will delete its original source only after YouTube processing succeeds.`
          : `${updated.fileName} will retain its original source after upload.`,
      );
    } catch (error) {
      await reportOperatorError(error, "cleanupChoice");
    } finally {
      setBusy(false);
    }
  }, [updateSnapshotItem]);

  const deleteOriginalAfterUpload = useCallback(async (
    item: UploadItem,
    confirmation: string,
  ) => {
    setBusy(true);
    try {
      await deleteUploadedSource(item.id, confirmation);
      await refresh();
      setNotice(
        `Original source cleanup completed for “${item.fileName}”. The YouTube video was retained.`,
      );
    } catch (error) {
      await reportOperatorError(error, "cleanupOriginal");
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const refreshYouTubeLibrary = async () => {
    const channel = snapshot.activeChannel;
    const generation = stateChannelGenerationRef.current;
    if (!channel) {
      setNotice("Connect a YouTube channel before refreshing its library.");
      return;
    }
    setBusy(true);
    setNotice(`Refreshing ${channel}'s YouTube library…`);
    try {
      const synced = await syncChannelInventory();
      if (generation !== stateChannelGenerationRef.current) return;
      await refresh();
      if (generation !== stateChannelGenerationRef.current) return;
      const nextConnection = await loadConnectionSettings();
      if (generation !== stateChannelGenerationRef.current) return;
      updateConnection(nextConnection);
      setLibraryRefreshVersion((version) => version + 1);
      setNotice(
        `Library refreshed: ${synced} YouTube video${pluralS(synced)} saved locally for ${channel}. Eligible uploads resume automatically.`,
      );
    } catch (error) {
      await reportOperatorError(error, "library");
    } finally {
      setBusy(false);
    }
  };

  const runDedupe = async () => {
    const channel = snapshot.activeChannel;
    const generation = stateChannelGenerationRef.current;
    if (!channel) {
      setNotice(
        "Connect a YouTube channel before running duplicate detection.",
      );
      return;
    }
    setBusy(true);
    setDedupeBusy(true);
    setDedupeActivity([]);
    setDedupePhase("syncing");
    logDedupeActivity("running", `Started duplicate detection for ${channel}.`);
    logDedupeActivity(
      "running",
      "Synchronizing this channel's uploaded-video inventory from YouTube…",
    );
    setNotice(`Checking ${channel}'s uploaded-video titles for duplicates…`);
    try {
      const synced = await syncChannelInventory();
      if (generation !== stateChannelGenerationRef.current) return;
      logDedupeActivity(
        "success",
        `Synced ${synced} uploaded video${pluralS(synced)} into this device's channel inventory.`,
      );
      setDedupePhase("rebuilding");
      logDedupeActivity(
        "running",
        "Rebuilding normalized exact-title and numbered-copy candidates locally…",
      );
      const refreshed = await refresh();
      if (generation !== stateChannelGenerationRef.current) return;
      const candidateCount = refreshed.duplicates.length;
      logDedupeActivity(
        "success",
        `Ready for review: ${candidateCount} duplicate candidate${pluralS(candidateCount)}. No videos were removed.`,
      );
      setDedupePhase("complete");
      setNotice(
        `Dedupe complete for ${channel}: ${synced} uploaded video${pluralS(synced)} checked and ${candidateCount} candidate${pluralS(candidateCount)} ready for review.`,
      );
    } catch (error) {
      if (generation !== stateChannelGenerationRef.current) return;
      const message = await reportOperatorError(error, "dedupe");
      setDedupePhase("error");
      logDedupeActivity("error", `Dedupe stopped: ${message}`);
      setNotice(`Dedupe could not run: ${message}`);
    } finally {
      setDedupeBusy(false);
      setBusy(false);
    }
  };

  const ignoreDuplicate = useCallback(async (candidateId: string) => {
    setBusy(true);
    try {
      await ignoreDuplicateCandidate(candidateId);
      await refresh();
      setNotice(
        "This potential match is ignored on this device. Re-audit ignored matches restores it for review.",
      );
    } catch (error) {
      await reportOperatorError(error, "ignoreDuplicate");
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const reAuditIgnoredMatches = async () => {
    setBusy(true);
    try {
      const restored = await reAuditIgnoredDuplicateCandidates();
      if (restored === 0) {
        setNotice("There are no ignored duplicate matches to re-audit.");
        return;
      }
      if (snapshot.activeChannel) {
        await runDedupe();
      } else {
        await refresh();
        setNotice(
          `${restored} ignored duplicate match${restored === 1 ? " was" : "es were"} restored for local review. Connect YouTube to re-audit uploaded-title matches.`,
        );
      }
    } catch (error) {
      await reportOperatorError(error, "reAudit");
    } finally {
      setBusy(false);
    }
  };

  const handleDuplicateDeletionComplete = useCallback(
    async (videoId: string, title: string) => {
      await refresh();
      setNotice(
        `YouTube confirmed permanent deletion of ${videoId} (“${title}”). The local execution receipt was saved.`,
      );
    },
    [refresh],
  );
  const handleBulkDuplicateDeletionComplete = useCallback(
    async (count: number) => {
      await refresh();
      setNotice(
        `YouTube confirmed permanent deletion of ${count} selected video${pluralS(count)}. Local execution receipts were saved.`,
      );
    },
    [refresh],
  );

  if (crashRecovery?.crashDetected) {
    return (
      <Suspense fallback={<main className="ui-loading" role="status" />}>
        <i data-performance-shell="recovery" hidden />
        <CrashRecoveryScreen
          detectedAt={crashRecovery.detectedAt}
          failureKind={crashRecovery.failureKind}
          onContinue={() => setCrashRecovery({ crashDetected: false })}
        />
      </Suspense>
    );
  }

  if (!startupReady) {
    return (
      <SafeStartupShell
        failure={startupFailure}
        readiness={startup?.readiness}
      />
    );
  }

  return (
    <main className={appShellClass}>
      {isTauri && connectionSettings?.activeChannelId && (
        <Suspense fallback={null}>
          <RevisionedStateBridge
            options={[
              connectionSettings.activeChannelId,
              snapshot,
              stateChannelIdRef,
              stateRevisionRef,
              setConnectionSettings,
              setSnapshot,
              setPreflightScan,
              setPreflightFileCount,
              updateConnection,
              loadFinalPreflightResult,
            ]}
          />
        </Suspense>
      )}
      <Suspense fallback={<WorkspacePending />}>
        <WorkspaceHeader channel={snapshot.activeChannel} dark={theme === "dark"} busy={busy}
          onTheme={() => setTheme((current) => current === "dark" ? "light" : "dark")}
          onRefresh={() => void refreshYouTubeLibrary()} onImport={() => void addVideo()} />
      </Suspense>

      <p className="ui-notice" role="status">
        <span aria-hidden="true" className="ui-notice-dot" />
        {notice}
      </p>
      <Suspense fallback={null}><DesktopRuntimeIntegration recoveryModeRef={recoveryModeRef} channelId={snapshot.activeChannelId} onExitRequest={() => setExitConfirmationOpen(true)} onShowAccount={() => setActiveTab("account")} onShowUploads={() => setActiveTab("batch")} onNotice={setNotice} onRefresh={refreshQueue} /></Suspense>
      {(exitConfirmationOpen || pendingImportPaths || (connectionSettings && !connectionSettings.oauthConfigured && !setupDismissed)) && (
        <Suspense fallback={<WorkspacePending />}>
          <AppDialogs
            setupOpen={Boolean(connectionSettings && !connectionSettings.oauthConfigured && !setupDismissed)}
            exitOpen={exitConfirmationOpen}
            importPaths={pendingImportPaths}
            activeChannelId={snapshot.activeChannelId}
            onOpenAccount={() => { setSetupDismissed(true); setActiveTab("account"); }}
            onDismissSetup={() => setSetupDismissed(true)}
            onKeepOpen={() => setExitConfirmationOpen(false)}
            onExit={() => void confirmExit()}
            onCancelImport={() => setPendingImportPaths(undefined)}
            onConfirmImport={(settings) => {
              const paths = pendingImportPaths;
              setPendingImportPaths(undefined);
              if (paths) void importVideos(paths, settings);
            }}
          />
        </Suspense>
      )}
      {!isTauri && (
        <Suspense fallback={null}><BrowserPreviewNotice /></Suspense>
      )}

      <div className="ui-workspace-layout">
        <Suspense fallback={<WorkspacePending />}>
          <WorkspaceNavigation active={activeTab} dedupeBusy={dedupeBusy} onSelect={setActiveTab} />
        </Suspense>
        <div className="min-w-0">
          <Suspense
            fallback={
              <section
                aria-labelledby={`workspace-tab-button-${activeTab}`}
                id={`workspace-tab-${activeTab}`}
                role="tabpanel"
                tabIndex={0}
              >
                <WorkspacePending />
              </section>
            }
          >
          <section
            aria-labelledby={`workspace-tab-button-${activeTab}`}
            data-performance-batch-content={
              activeTab === "batch" ? "ready" : undefined
            }
            id={`workspace-tab-${activeTab}`}
            role="tabpanel"
            tabIndex={0}
          >
          {activeTab === "batch" && (
            <>
            <UploadTitleDuplicateReview
              activeChannelId={snapshot.activeChannelId}
              busy={busy}
              candidates={snapshot.pendingTitleDuplicates}
              onResolve={resolveTitleDuplicates}
            />
            <section
              className="ui-queue-panel"
              aria-labelledby="queue-heading"
            >
              <div className={`${sectionHeadingClass} mb-4`}>
                <div>
                  <p className={eyebrowClass}>PERSISTENT QUEUE</p>
                  <h2 className="ui-card-heading" id="queue-heading">Your upload queue</h2>
                  <p className="ui-section-copy">
                    Queue and session state are saved locally without making
                    another video file. Recovery requires each original to stay
                    available and unchanged until its upload completes.
                  </p>
                </div>
                <Suspense fallback={<span className="ui-queue-count">{snapshot.items.length} saved item{snapshot.items.length === 1 ? "" : "s"}</span>}>
                  <QueueToolbar busy={busy} count={snapshot.items.length} onClear={() => void clearUploads()} />
                </Suspense>
              </div>
              <UploadProgressSummary items={snapshot.items} />
              <BatchPreparationPanel items={snapshot.items} channelId={snapshot.activeChannelId} channelName={snapshot.activeChannel} busy={busy} onRefresh={refreshQueue} onNotice={setNotice} onConnectionChanged={(settings) => { if (settings) updateConnection(settings); }} />
              <BatchImportArea active={dropActive} busy={busy} onChoose={() => void addVideo()} />
              <QueueTable
                items={snapshot.items}
                busy={busy}
                channelId={snapshot.activeChannelId}
                channelName={snapshot.activeChannel}
                onRefresh={refreshQueue}
                onNotice={setNotice}
                onQueue={queue}
                onCancel={cancelUpload}
                onVisibilityChange={changeVisibility}
                onDeleteSourceAfterUploadChange={changeSourceCleanup}
                onDeleteUploadedSource={deleteOriginalAfterUpload}
              />
            </section>
            </>
          )}

          {activeTab === "monitor" && (
            <FolderMonitorPanel
              activeChannel={snapshot.activeChannel}
              activeChannelId={snapshot.activeChannelId}
              onNotice={setNotice}
              onQueueRefresh={refreshQueue}
            />
          )}

          {activeTab === "dedupe" && (
            <>
            <PreIngestDuplicatePanel
              activeChannelId={snapshot.activeChannelId}
              busy={preflightBusy}
              fileCount={preflightFileCount}
              dropActive={preflightDropActive}
              onCancel={cancelPreflight}
              onChoose={chooseFilesForPreflight}
              onPrepareLocalDuplicateDelete={prepareLocalDuplicateDelete}
              onLoadMetadata={loadPreflightDuplicateFileMetadata}
              onLoadPage={loadPreflightPage}
              onDeleteLocalDuplicate={deleteLocalDuplicate}
              scan={preflightScan}
            />
            <section
              className={panelClass}
              aria-labelledby="duplicates-heading"
            >
              <div className={sectionHeadingClass}>
                <div>
                  <p className={eyebrowClass}>REVIEW REQUIRED</p>
                  <h2 className="ui-card-heading" id="duplicates-heading">Duplicate candidates</h2>
                  <p className="ui-section-copy">
                    {snapshot.activeChannel
                      ? `Check ${snapshot.activeChannel}'s uploaded-video titles for exact matches and numbered copies.`
                      : "Connect a YouTube channel to check its uploaded-video titles for duplicates."}
                  </p>
                </div>
                <div className={actionRowClass}>
                  <button
                    className={secondaryButtonClass}
                    disabled={busy}
                    onClick={() => void reAuditIgnoredMatches()}
                    type="button"
                  >
                    Re-audit ignored matches
                  </button>
                  <button
                    className={secondaryButtonClass}
                    disabled={!snapshot.activeChannel || busy}
                    aria-busy={dedupeBusy}
                    onClick={() => void runDedupe()}
                  >
                    {dedupeBusy ? "Running dedupe…" : "Run dedupe"}
                  </button>
                </div>
              </div>
              <DuplicateReview
                activeChannelId={snapshot.activeChannelId}
                candidates={snapshot.duplicates}
                onIgnore={ignoreDuplicate}
                onDeletionComplete={handleDuplicateDeletionComplete}
                onBulkDeletionComplete={handleBulkDuplicateDeletionComplete}
              />
              <DedupeActivityPanel
                activity={dedupeActivity}
                busy={dedupeBusy}
                phase={dedupePhase}
              />
            </section>
            </>
          )}

          {activeTab === "deletion" && (
            <section className={panelClass} aria-labelledby="deletion-heading">
              <div className={sectionHeadingClass}>
                <div>
                  <p className={eyebrowClass}>EXPLICIT LOCAL REVIEW</p>
                  <h2 className="ui-card-heading" id="deletion-heading">Video removal requests</h2>
                </div>
              </div>
              <DeletionReview
                activeChannel={snapshot.activeChannel}
                activeChannelId={snapshot.activeChannelId}
                busy={busy}
                onNotice={setNotice}
                refreshVersion={libraryRefreshVersion}
              />
            </section>
          )}

          {activeTab === "rename" && (
            <VideoTitleRename
              activeChannel={snapshot.activeChannel}
              activeChannelId={snapshot.activeChannelId}
              onNotice={setNotice}
              refreshVersion={libraryRefreshVersion}
            />
          )}

          {activeTab === "playlists" && (
            <PlaylistManager activeChannel={snapshot.activeChannel} activeChannelId={snapshot.activeChannelId} onNotice={setNotice} refreshVersion={libraryRefreshVersion} />
          )}

          {activeTab === "transfer" && (
            <TransferPanel
              onConnectionChange={updateConnection}
              onNotice={setNotice}
            />
          )}

          {activeTab === "account" && (
            <ConnectionPanel onConnectionChange={updateConnection} />
          )}

          {activeTab === "about" && (
            <DiagnosticsPanel />
          )}
          </section>
          </Suspense>
        </div>
      </div>

      <Suspense fallback={null}><WorkspaceFooter /></Suspense>
    </main>
  );
}
