import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { KeyboardEvent } from "react";
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
  resumeQueuedUploads,
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

const emptySnapshot: DashboardSnapshot = {
  revision: 0,
  items: [],
  duplicates: [],
  pendingTitleDuplicates: [],
};
const workspaceTabs = [
  ["batch", "Batch uploads"],
  ["monitor", "Folder monitor"],
  ["dedupe", "Duplicate review"],
  ["transfer", "Export and import"],
  ["deletion", "Video deletion"],
  ["account", "Connected account"],
  ["about", "About and support"],
] as const;
type WorkspaceTab = (typeof workspaceTabs)[number][0];

const ConnectionPanel = lazy(() =>
  import("./components/ConnectionPanel").then((module) => ({
    default: module.ConnectionPanel,
  })),
);
const CrashRecoveryScreen = lazy(() =>
  import("./components/CrashRecoveryScreen").then((module) => ({
    default: module.CrashRecoveryScreen,
  })),
);
const GoogleSetupWizard = lazy(() =>
  import("./components/GoogleSetupWizard").then((module) => ({
    default: module.GoogleSetupWizard,
  })),
);
const loadQueueTable = () =>
  import("./components/QueueTable").then((module) => ({
    default: module.QueueTable,
  }));
const QueueTable = lazy(loadQueueTable);
const loadManualUploadDefaultsPanel = () =>
  import("./components/ManualUploadDefaultsPanel").then((module) => ({
    default: module.ManualUploadDefaultsPanel,
  }));
const ManualUploadDefaultsPanel = lazy(loadManualUploadDefaultsPanel);
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
  ]);
}
const DeletionReview = lazy(() =>
  import("./components/DeletionReview").then((module) => ({
    default: module.DeletionReview,
  })),
);
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
const DuplicateReview = lazy(() =>
  import("./components/DuplicateReview").then((module) => ({
    default: module.DuplicateReview,
  })),
);
const FolderMonitorPanel = lazy(() =>
  import("./components/FolderMonitorPanel").then((module) => ({
    default: module.FolderMonitorPanel,
  })),
);
const PreIngestDuplicatePanel = lazy(() =>
  import("./components/PreIngestDuplicatePanel").then((module) => ({
    default: module.PreIngestDuplicatePanel,
  })),
);
const TransferPanel = lazy(() =>
  import("./components/TransferPanel").then((module) => ({
    default: module.TransferPanel,
  })),
);
const UploadIntakeReview = lazy(() =>
  import("./components/UploadIntakeReview").then((module) => ({
    default: module.UploadIntakeReview,
  })),
);
const RevisionedStateBridge = lazy(
  () => import("./lib/revisioned-state-bridge"),
);

function WorkspacePending() {
  return (
    <p className="workspace-loading" role="status">
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
    <main className="app-shell" data-performance-shell="holding">
      <header className="topbar">
        <div className="brand-lockup">
          <img className="brand-mark" src="/favicon.svg" alt="" />
          <div>
            <p className="eyebrow">SAFE STARTUP</p>
            <h1>YouTube Upload Manager</h1>
            <p className="subtle">Your saved work remains on this device.</p>
          </div>
        </div>
      </header>
      <section
        aria-busy={!failure}
        aria-live="polite"
        className="panel"
        role="status"
      >
        <p className="eyebrow">
          {failure ? "STARTUP NEEDS ATTENTION" : "RECOVERING LOCAL WORK"}
        </p>
        <h2>{failure ? "The workspace is still locked" : "Preparing your workspace…"}</h2>
        <p>{detail}</p>
        <p className="subtle">
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
  const [startup, setStartup] = useState<StartupBootstrap | undefined>(
    initialStartup,
  );
  const [startupFailure, setStartupFailure] = useState<string>();
  const [snapshot, setSnapshot] = useState<DashboardSnapshot>(
    initialStartup?.snapshot ?? emptySnapshot,
  );
  const [notice, setNotice] = useState(
    "Local queue is ready. No media leaves this device except during a YouTube upload.",
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

  const updateConnection = useCallback((settings: ConnectionSettings) => {
    setConnectionSettings(settings);
    setSnapshot((current) => {
      if (current.activeChannelId === settings.activeChannelId) {
        return {
          ...current,
          activeChannel: settings.activeChannel,
        };
      }
      stateChannelIdRef.current = settings.activeChannelId;
      stateRevisionRef.current = 0;
      return {
        ...emptySnapshot,
        activeChannel: settings.activeChannel,
        activeChannelId: settings.activeChannelId,
      };
    });
    if (isTauri) {
      void loadSnapshot()
        .then((next) => {
          if (next.activeChannelId !== settings.activeChannelId) return;
          if (
            stateChannelIdRef.current === next.activeChannelId &&
            next.revision < stateRevisionRef.current
          )
            return;
          stateChannelIdRef.current = next.activeChannelId;
          stateRevisionRef.current = next.revision;
          setSnapshot(next);
        })
        .catch(() => undefined);
    }
  }, []);

  const refresh = useCallback(async () => {
    const next = await loadSnapshot();
    if (
      stateChannelIdRef.current === next.activeChannelId &&
      next.revision < stateRevisionRef.current
    )
      return next;
    stateChannelIdRef.current = next.activeChannelId;
    stateRevisionRef.current = next.revision;
    setSnapshot(next);
    return next;
  }, []);

  const refreshQueue = useCallback(async () => {
    await refresh();
  }, [refresh]);

  const applyStartup = useCallback((next: StartupBootstrap) => {
    setStartup(next);
    stateChannelIdRef.current = next.snapshot.activeChannelId;
    stateRevisionRef.current = next.snapshot.revision;
    setSnapshot(next.snapshot);
    setConnectionSettings(next.connection);
    setCrashRecovery(next.crashRecovery);
  }, []);

  const handleWorkspaceKeyDown = useCallback(
    (event: KeyboardEvent<HTMLButtonElement>) => {
      const currentIndex = workspaceTabs.findIndex(([id]) => id === activeTab);
      let nextIndex: number | undefined;
      if (event.key === "ArrowDown" || event.key === "ArrowRight")
        nextIndex = (currentIndex + 1) % workspaceTabs.length;
      else if (event.key === "ArrowUp" || event.key === "ArrowLeft")
        nextIndex =
          (currentIndex - 1 + workspaceTabs.length) % workspaceTabs.length;
      else if (event.key === "Home") nextIndex = 0;
      else if (event.key === "End") nextIndex = workspaceTabs.length - 1;
      if (nextIndex === undefined) return;

      event.preventDefault();
      const nextTab = workspaceTabs[nextIndex][0];
      setActiveTab(nextTab);
      window.requestAnimationFrame(() => {
        document.getElementById(`workspace-tab-button-${nextTab}`)?.focus();
      });
    },
    [activeTab, setActiveTab],
  );

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
            ? `Fast filename match started for ${paths.length} file${paths.length === 1 ? "" : "s"}.`
            : `Deep BLAKE3 match started for ${paths.length} file${paths.length === 1 ? "" : "s"}.`,
        );
      } catch (error) {
        if (runId !== preflightRunId.current) return;
        setNotice(
          error instanceof Error
            ? error.message
            : "The selected files could not be checked for duplicates.",
        );
      } finally {
        if (runId === preflightRunId.current) setPreflightBusy(false);
      }
    },
    [],
  );

  const loadFinalPreflightResult = useCallback(async (jobId: string) => {
    if (finalizedPreflightJobs.current.has(jobId)) return;
    finalizedPreflightJobs.current.add(jobId);
    try {
      const scan = await loadPreflightDuplicateScan(jobId);
      if (scan.id !== jobId) return;
      setPreflightScan(scan);
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
          `${scan.completedFiles} file${scan.completedFiles === 1 ? "" : "s"} checked before ingest. ${matches} need${matches === 1 ? "s" : ""} duplicate review.`,
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
    const size = 48;
    const scan = await loadPreflightDuplicateScan(current.id, {
      fileOffset: kind === "files" ? (page - 1) * size : current.fileOffset ?? 0,
      fileLimit: size,
      activityOffset:
        kind === "activity" ? (page - 1) * size : current.activityOffset ?? 0,
      activityLimit: size,
    });
    setPreflightScan(scan);
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
          `Deleted “${confirmation}” from this device. Its managed upload copy and YouTube videos were not changed.`,
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
    await cancelPreflightDuplicateScan(preflightScan.id);
    setPreflightScan(
      (current) => current && { ...current, status: "cancelled" },
    );
    setNotice(
      "The pre-ingest duplicate job was cancelled. No selected file was ingested or uploaded.",
    );
  }, [preflightScan]);

  const clearUploads = useCallback(async () => {
    setBusy(true);
    try {
      const cleared = await clearUploadQueue();
      await refresh();
      setNotice(
        `${cleared} local upload job${cleared === 1 ? " was" : "s were"} removed from the queue. Media copies were retained.`,
      );
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const cancelUpload = useCallback(async (item: UploadItem) => {
    setBusy(true);
    try {
      await cancelUploadItem(item.id);
      await refresh();
      setNotice(`Removed “${item.title}” from the upload queue. Media was retained.`);
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const importVideos = useCallback(
    async (paths: string[], settings: ManualUploadSettings) => {
      setBusy(true);
      try {
        const receipt = await importAndQueueBatch(paths, settings);
        await refresh();
        if (receipt.importedCount === 0) {
          const firstFailure = receipt.items.find(
            (item) => item.status === "failed",
          )?.detail;
          setNotice(
            `${receipt.failedCount} video${receipt.failedCount === 1 ? "" : "s"} could not be imported. ${firstFailure ?? ""}`.trim(),
          );
          return;
        }
        if (receipt.detail) {
          setNotice(
            `${receipt.importedCount} video${receipt.importedCount === 1 ? "" : "s"} safely imported to this device. ${receipt.detail}`,
          );
          return;
        }
        const importNotice = receipt.duplicateCount > 0
          ? `${receipt.importedCount} video${receipt.importedCount === 1 ? "" : "s"} imported locally. ${receipt.duplicateCount} light duplicate match${receipt.duplicateCount === 1 ? " needs" : "es need"} your decision.`
          : receipt.queuedCount > 0
            ? `${receipt.importedCount} video${receipt.importedCount === 1 ? "" : "s"} imported and ${receipt.queuedCount} queued locally.`
            : `${receipt.importedCount} video${receipt.importedCount === 1 ? "" : "s"} safely imported to this device. Connect YouTube to start uploading them.`;
        const firstFailure = receipt.items.find(
          (item) => item.status === "failed",
        )?.detail;
        const failuresNotice = receipt.failedCount > 0
          ? ` ${receipt.failedCount} item${receipt.failedCount === 1 ? "" : "s"} need attention.${firstFailure ? ` ${firstFailure}` : ""}`
          : "";
        setNotice(
          `${importNotice}${receipt.queuedCount > 0 ? " Uploads start automatically when capacity is available." : ""}${failuresNotice}`.trim(),
        );
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

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) =>
        getCurrentWindow().onCloseRequested((event) => {
        // Recovery is a safe holding screen. Do not trap the operator in it:
        // closing leaves the persisted crash marker and all resumable work
        // untouched for the next launch.
        if (recoveryModeRef.current) return;
        event.preventDefault();
        setExitConfirmationOpen(true);
        }),
      )
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // Closing follows the operating system default when the native event is unavailable.
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

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
        "Run this screen through Tauri to import a file into the managed local workspace.",
      );
      return;
    }
    const { selectVideoFiles } = await import("./lib/video-picker");
    reviewImport(await selectVideoFiles());
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
      setSnapshot((current) => ({
        ...current,
        items: current.items.map((candidate) =>
          candidate.id === updated.id ? updated : candidate,
        ),
      }));
      setNotice(
        `${updated.fileName} is queued and will start automatically when capacity is available.`,
      );
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const chooseFilesForPreflight = useCallback(async (mode: "light" | "deep") => {
    if (!isTauri) {
      setNotice("Run this screen through Tauri to check files before ingest.");
      return;
    }
    const { selectPreflightFiles } = await import("./lib/video-picker");
    void runPreflightDuplicateCheck(await selectPreflightFiles(), mode);
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
      setNotice(
        error instanceof Error
          ? error.message
          : "The duplicate decision could not be saved.",
      );
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
      setSnapshot((current) => ({
        ...current,
        items: current.items.map((candidate) =>
          candidate.id === updated.id ? updated : candidate,
        ),
      }));
      setNotice(`${updated.fileName} will upload as ${visibility}.`);
    } catch (error) {
      setNotice(
        error instanceof Error
          ? error.message
          : "The upload visibility could not be changed.",
      );
    } finally {
      setBusy(false);
    }
  }, []);

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
      setSnapshot((current) => ({
        ...current,
        items: current.items.map((candidate) =>
          candidate.id === updated.id ? updated : candidate,
        ),
      }));
      setNotice(
        deleteSourceAfterUpload
          ? `${updated.fileName} will delete its original source only after YouTube confirms the upload.`
          : `${updated.fileName} will retain its original source after upload.`,
      );
    } catch (error) {
      setNotice(
        error instanceof Error
          ? error.message
          : "The source cleanup choice could not be saved.",
      );
    } finally {
      setBusy(false);
    }
  }, []);

  const deleteOriginalAfterUpload = useCallback(async (
    item: UploadItem,
    confirmation: string,
  ) => {
    setBusy(true);
    try {
      await deleteUploadedSource(item.id, confirmation);
      await refresh();
      setNotice(
        `Original source cleanup completed for “${item.fileName}”. The managed app copy and YouTube video were retained.`,
      );
    } catch (error) {
      setNotice(
        error instanceof Error
          ? error.message
          : "The original source could not be deleted.",
      );
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const refreshYouTubeLibrary = async () => {
    const channel = snapshot.activeChannel;
    if (!channel) {
      setNotice("Connect a YouTube channel before refreshing its library.");
      return;
    }
    setBusy(true);
    setNotice(`Refreshing ${channel}'s YouTube library…`);
    try {
      const synced = await syncChannelInventory();
      const started = await resumeQueuedUploads();
      await refresh();
      updateConnection(await loadConnectionSettings());
      setLibraryRefreshVersion((version) => version + 1);
      setNotice(
        `Library refreshed: ${synced} YouTube video${synced === 1 ? "" : "s"} saved locally for ${channel}.${started > 0 ? ` ${started} queued upload${started === 1 ? "" : "s"} started.` : " Automatic upload dispatch was checked."}`,
      );
    } catch (error) {
      setNotice(
        operatorErrorMessage(
          error,
          "The YouTube library could not be refreshed. Your last complete local library was kept.",
        ),
      );
    } finally {
      setBusy(false);
    }
  };

  const runDedupe = async () => {
    const channel = snapshot.activeChannel;
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
      logDedupeActivity(
        "success",
        `Synced ${synced} uploaded video${synced === 1 ? "" : "s"} into this device's channel inventory.`,
      );
      setDedupePhase("rebuilding");
      logDedupeActivity(
        "running",
        "Rebuilding normalized exact-title and numbered-copy candidates locally…",
      );
      const refreshed = await refresh();
      const candidateCount = refreshed.duplicates.length;
      logDedupeActivity(
        "success",
        `Ready for review: ${candidateCount} duplicate candidate${candidateCount === 1 ? "" : "s"}. No videos were removed.`,
      );
      setDedupePhase("complete");
      setNotice(
        `Dedupe complete for ${channel}: ${synced} uploaded video${synced === 1 ? "" : "s"} checked and ${candidateCount} candidate${candidateCount === 1 ? "" : "s"} ready for review.`,
      );
    } catch (error) {
      const message =
        error instanceof Error
          ? error.message
          : "The YouTube library could not be synchronized.";
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
      setNotice(
        error instanceof Error
          ? error.message
          : "The duplicate review decision could not be saved.",
      );
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
      setNotice(
        error instanceof Error
          ? error.message
          : "Ignored duplicate matches could not be restored.",
      );
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
        `YouTube confirmed permanent deletion of ${count} selected video${count === 1 ? "" : "s"}. Local execution receipts were saved.`,
      );
    },
    [refresh],
  );

  if (crashRecovery?.crashDetected) {
    return (
      <Suspense fallback={<main className="crash-recovery" role="status" />}>
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
    <main className="app-shell">
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
      <header className="topbar">
        <div className="brand-lockup">
          <img className="brand-mark" src="/favicon.svg" alt="" />
          <div>
            <p className="eyebrow">UPLOAD WORKSPACE</p>
            <h1>YouTube Upload Manager</h1>
            <p className="subtle">
              {snapshot.activeChannel
                ? `Active channel: ${snapshot.activeChannel}`
                : "Your files stay on this device until you start an upload."}
            </p>
          </div>
        </div>
        <div className="actions">
          <button
            className="secondary-action"
            disabled={!snapshot.activeChannel || busy}
            onClick={() => void refreshYouTubeLibrary()}
            type="button"
          >
            Refresh library
          </button>
          <button
            className="import-button"
            disabled={busy}
            onClick={() => void addVideo()}
          >
            Import videos
          </button>
        </div>
      </header>

      <p className="notice" role="status">
        <span aria-hidden="true" />
        {notice}
      </p>
      {connectionSettings &&
        !connectionSettings.oauthConfigured &&
        !setupDismissed && (
          <>
            <div aria-hidden="true" className="google-setup-backdrop" />
            <Suspense fallback={<WorkspacePending />}>
              <GoogleSetupWizard
                onOpenConnectedAccount={() => {
                  setSetupDismissed(true);
                  setActiveTab("account");
                  setNotice(
                    "Import your Desktop OAuth JSON from Connected account, then connect YouTube when you are ready.",
                  );
                }}
                onDismiss={() => setSetupDismissed(true)}
              />
            </Suspense>
          </>
        )}
      {exitConfirmationOpen && (
        <>
          <div aria-hidden="true" className="exit-confirmation-backdrop" />
          <section
            aria-labelledby="exit-confirmation-heading"
            aria-modal="true"
            className="exit-confirmation"
            role="dialog"
          >
            <p className="eyebrow">EXIT APPLICATION</p>
            <h2 id="exit-confirmation-heading">Exit YouTube Upload Manager?</h2>
            <p>
              Your queue and duplicate-review progress are saved locally. Any
              active upload will be recovered when you open the app again.
            </p>
            <div className="exit-confirmation__actions">
              <button
                autoFocus
                className="secondary-action"
                onClick={() => setExitConfirmationOpen(false)}
                type="button"
              >
                Keep app open
              </button>
              <button className="danger-button" onClick={() => void confirmExit()} type="button">
                Exit app
              </button>
            </div>
          </section>
        </>
      )}
      {pendingImportPaths && (
        <>
          <div aria-hidden="true" className="intake-review-backdrop" />
          <Suspense fallback={<WorkspacePending />}>
            <UploadIntakeReview
              paths={pendingImportPaths}
              onCancel={() => setPendingImportPaths(undefined)}
              onConfirm={(settings) => {
                const paths = pendingImportPaths;
                setPendingImportPaths(undefined);
                void importVideos(paths, settings);
              }}
            />
          </Suspense>
        </>
      )}
      {!isTauri && (
        <p className="warning">
          Browser preview mode: managed local file import is available only in
          the signed Tauri app.
        </p>
      )}

      <div className="workspace-layout">
        <nav
          aria-label="Workspace sections"
          className="workspace-sidebar"
          role="tablist"
        >
          <span className="workspace-sidebar__label">Workspace</span>
          {workspaceTabs.map(([id, label]) => (
            <button
              aria-controls={`workspace-tab-${id}`}
              aria-selected={activeTab === id}
              className={activeTab === id ? "is-active" : ""}
              id={`workspace-tab-button-${id}`}
              key={id}
              onClick={() => setActiveTab(id)}
              onKeyDown={handleWorkspaceKeyDown}
              role="tab"
              tabIndex={activeTab === id ? 0 : -1}
              type="button"
            >
              {label}
              {id === "dedupe" && dedupeBusy && (
                <span
                  className="workspace-sidebar__activity"
                  aria-label="Duplicate detection in progress"
                />
              )}
            </button>
          ))}
        </nav>
        <div className="workspace-tabs">
          <Suspense
            fallback={
              <section
                aria-labelledby={`workspace-tab-button-${activeTab}`}
                className="workspace-tab"
                id={`workspace-tab-${activeTab}`}
                role="tabpanel"
                tabIndex={0}
              >
                <WorkspacePending />
              </section>
            }
          >
          {activeTab === "batch" && (
            <section
            aria-labelledby="workspace-tab-button-batch"
            className="workspace-tab"
            data-performance-batch-content="ready"
            id="workspace-tab-batch"
            role="tabpanel"
            tabIndex={0}
          >
            <UploadTitleDuplicateReview
              busy={busy}
              candidates={snapshot.pendingTitleDuplicates}
              onResolve={resolveTitleDuplicates}
            />
            <section
              className="queue-workspace"
              aria-labelledby="queue-heading"
            >
              <div className="section-heading">
                <div>
                  <p className="eyebrow">PERSISTENT QUEUE</p>
                  <h2 id="queue-heading">Your upload queue</h2>
                  <p className="section-copy">
                    Every import and upload state is saved locally, so
                    interrupted work can continue where it stopped.
                  </p>
                </div>
                <div className="queue-workspace__actions">
                  <span className="item-count">
                    {snapshot.items.length} saved item
                    {snapshot.items.length === 1 ? "" : "s"}
                  </span>
                  <button
                    className="text-button"
                    disabled={busy}
                    onClick={() => void clearUploads()}
                    type="button"
                  >
                    Clear upload queue
                  </button>
                </div>
              </div>
              <UploadProgressSummary items={snapshot.items} />
              <ManualUploadDefaultsPanel />
              <div
                className={`queue-dropzone${dropActive ? " queue-dropzone--active" : ""}`}
              >
                <div className="queue-dropzone__copy">
                  <strong>Drag and drop videos here</strong>
                  <p>
                    They are copied into this device’s managed workspace before
                    any upload. Files over YouTube’s 256 GB or 12-hour limits
                    are stopped before copying. You can also choose multiple
                    files.
                  </p>
                </div>
                <button
                  className="secondary-action"
                  disabled={busy}
                  onClick={() => void addVideo()}
                  type="button"
                >
                  Choose videos
                </button>
              </div>
              <QueueTable
                items={snapshot.items}
                busy={busy}
                onQueue={queue}
                onCancel={cancelUpload}
                onVisibilityChange={changeVisibility}
                onDeleteSourceAfterUploadChange={changeSourceCleanup}
                onDeleteUploadedSource={deleteOriginalAfterUpload}
              />
            </section>
            </section>
          )}

          {activeTab === "monitor" && (
            <section
            aria-labelledby="workspace-tab-button-monitor"
            className="workspace-tab"
            id="workspace-tab-monitor"
            role="tabpanel"
            tabIndex={0}
          >
            <FolderMonitorPanel
              activeChannel={snapshot.activeChannel}
              activeChannelId={snapshot.activeChannelId}
              onNotice={setNotice}
              onQueueRefresh={refreshQueue}
            />
            </section>
          )}

          {activeTab === "dedupe" && (
            <section
            aria-labelledby="workspace-tab-button-dedupe"
            className="workspace-tab"
            id="workspace-tab-dedupe"
            role="tabpanel"
            tabIndex={0}
          >
            <PreIngestDuplicatePanel
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
              className="panel duplicates-panel"
              aria-labelledby="duplicates-heading"
            >
              <div className="section-heading duplicate-heading">
                <div>
                  <p className="eyebrow">REVIEW REQUIRED</p>
                  <h2 id="duplicates-heading">Duplicate candidates</h2>
                  <p className="section-copy">
                    {snapshot.activeChannel
                      ? `Check ${snapshot.activeChannel}'s uploaded-video titles for exact matches and numbered copies.`
                      : "Connect a YouTube channel to check its uploaded-video titles for duplicates."}
                  </p>
                </div>
                <div className="duplicate-heading__actions">
                  <button
                    className="secondary-action dedupe-action"
                    disabled={busy}
                    onClick={() => void reAuditIgnoredMatches()}
                    type="button"
                  >
                    Re-audit ignored matches
                  </button>
                  <button
                    className="secondary-action dedupe-action"
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
            </section>
          )}

          {activeTab === "deletion" && (
            <section
            aria-labelledby="workspace-tab-button-deletion"
            className="workspace-tab"
            id="workspace-tab-deletion"
            role="tabpanel"
            tabIndex={0}
          >
            <section className="panel" aria-labelledby="deletion-heading">
              <div className="section-heading">
                <div>
                  <p className="eyebrow">EXPLICIT LOCAL REVIEW</p>
                  <h2 id="deletion-heading">Video removal requests</h2>
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
            </section>
          )}

          {activeTab === "transfer" && (
            <section
            aria-labelledby="workspace-tab-button-transfer"
            className="workspace-tab"
            id="workspace-tab-transfer"
            role="tabpanel"
            tabIndex={0}
          >
            <TransferPanel
              onConnectionChange={updateConnection}
              onNotice={setNotice}
            />
            </section>
          )}

          {activeTab === "account" && (
            <section
            aria-labelledby="workspace-tab-button-account"
            className="workspace-tab"
            id="workspace-tab-account"
            role="tabpanel"
            tabIndex={0}
          >
            <ConnectionPanel onConnectionChange={updateConnection} />
            </section>
          )}

          {activeTab === "about" && (
            <section
            aria-labelledby="workspace-tab-button-about"
            className="workspace-tab"
            id="workspace-tab-about"
            role="tabpanel"
            tabIndex={0}
          >
            <DiagnosticsPanel />
            </section>
          )}
          </Suspense>
        </div>
      </div>

      <footer className="app-disclaimer">
        YouTube Upload Manager is an independent project and is not affiliated
        with, endorsed by, sponsored by, or provided by Google or YouTube.
        Google and YouTube are trademarks of Google LLC; all other names, logos,
        and trademarks belong to their respective owners.
      </footer>
    </main>
  );
}
