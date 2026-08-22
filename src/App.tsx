import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  cancelPreflightDuplicateScan,
  checkUploadTitleDuplicates,
  clearUploadQueue,
  deletePreflightDuplicateFile,
  ignoreDuplicateCandidate,
  importAsset,
  isTauri,
  loadConnectionSettings,
  loadCrashRecoveryStatus,
  loadPreflightDuplicateScan,
  loadSnapshot,
  preflightDuplicateFiles,
  preparePreflightLocalDeleteFile,
  queueItem,
  reAuditIgnoredDuplicateCandidates,
  resolveUploadTitleDuplicates,
  setItemDeleteSourceAfterUpload,
  setItemVisibility,
  startQueuedUploads,
  syncChannelInventory,
  recordWebviewError,
} from "./lib/local";
import type {
  DashboardSnapshot,
  ManualUploadSettings,
  PreIngestDuplicateScan,
  UploadItem,
  UploadVisibility,
} from "./lib/types";
import type { ConnectionSettings } from "./lib/types";
import { ConnectionPanel } from "./components/ConnectionPanel";
import { GoogleSetupWizard } from "./components/GoogleSetupWizard";
import { DeletionReview } from "./components/DeletionReview";
import { DuplicateReview } from "./components/DuplicateReview";
import { FolderMonitorPanel } from "./components/FolderMonitorPanel";
import { QueueTable } from "./components/QueueTable";
import { UploadProgressSummary } from "./components/UploadProgressSummary";
import { UploadIntakeReview } from "./components/UploadIntakeReview";
import { ManualUploadDefaultsPanel } from "./components/ManualUploadDefaultsPanel";
import { UploadTitleDuplicateReview } from "./components/UploadTitleDuplicateReview";
import { PreIngestDuplicatePanel } from "./components/PreIngestDuplicatePanel";
import { TransferPanel } from "./components/TransferPanel";
import { DiagnosticsPanel } from "./components/DiagnosticsPanel";
import { CrashRecoveryScreen } from "./components/CrashRecoveryScreen";
import {
  dedupeProgressLabel,
  dedupeProgressStep,
  dedupeProgressStepCount,
  recordDedupeActivity,
} from "./lib/dedupe-activity";
import type {
  DedupeActivityState,
  DedupeActivityEntry,
  DedupeProgressPhase,
} from "./lib/dedupe-activity";
import { selectedFilePaths } from "./lib/file-picker";

const emptySnapshot: DashboardSnapshot = {
  items: [],
  duplicates: [],
  pendingTitleDuplicates: [],
};
const supportedVideoExtensions = new Set([
  "3g2",
  "3gp",
  "avi",
  "flv",
  "m4v",
  "mkv",
  "mov",
  "mp4",
  "mpeg",
  "mpg",
  "webm",
  "wmv",
]);
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

function isSupportedVideoPath(path: string) {
  const extension = path.split(".").pop()?.toLowerCase();
  return extension !== undefined && supportedVideoExtensions.has(extension);
}

export default function App() {
  const [snapshot, setSnapshot] = useState<DashboardSnapshot>(emptySnapshot);
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
    useState<ConnectionSettings>();
  const [setupDismissed, setSetupDismissed] = useState(false);
  const [crashRecovery, setCrashRecovery] = useState<{
    crashDetected: boolean;
    detectedAt?: string;
    failureKind?: string;
  }>();
  const [exitConfirmationOpen, setExitConfirmationOpen] = useState(false);
  const dedupeActivityId = useRef(0);
  const preflightRunId = useRef(0);
  const recoveryModeRef = useRef(false);
  recoveryModeRef.current = Boolean(crashRecovery?.crashDetected);

  const updateConnection = useCallback((settings: ConnectionSettings) => {
    setConnectionSettings(settings);
    setSnapshot((current) => ({
      ...current,
      activeChannel: settings.activeChannel,
    }));
  }, []);

  const refresh = useCallback(async () => {
    const next = await loadSnapshot();
    setSnapshot(next);
    return next;
  }, []);

  const logDedupeActivity = useCallback(
    (state: DedupeActivityState, message: string) => {
      dedupeActivityId.current += 1;
      setDedupeActivity((entries) =>
        recordDedupeActivity(entries, {
          id: dedupeActivityId.current,
          state,
          message,
        }),
      );
    },
    [],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let active = true;
    void loadCrashRecoveryStatus()
      .then((status) => {
        if (active) setCrashRecovery(status);
      })
      .catch(() => {
        if (active) setCrashRecovery({ crashDetected: false });
      });
    return () => {
      active = false;
    };
  }, []);

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

  useEffect(() => {
    let active = true;
    void loadConnectionSettings()
      .then((settings) => {
        if (active) updateConnection(settings);
      })
      .catch(() => {
        // The account panel retains the actionable local error if settings cannot load.
      });
    return () => {
      active = false;
    };
  }, [updateConnection]);

  const reviewImport = useCallback((paths: string[]) => {
    const videos = paths.filter(isSupportedVideoPath);
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
            : `Deep SHA-256 match started for ${paths.length} file${paths.length === 1 ? "" : "s"}.`,
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

  useEffect(() => {
    if (
      !preflightScan ||
      ( ["complete", "cancelled"].includes(preflightScan.status) &&
        preflightScan.pendingMetadataFiles === 0) ||
      !isTauri
    )
      return;
    const jobId = preflightScan.id;
    const interval = window.setInterval(() => {
      void loadPreflightDuplicateScan(jobId)
        .then((scan) => {
          if (scan.id !== jobId) return;
          setPreflightScan(scan);
          setPreflightFileCount(
            Math.max(0, scan.totalFiles - scan.completedFiles),
          );
          if (scan.status === "complete") {
            const matches = scan.files.filter(
              (file) =>
                file.localMatches.length > 0 ||
                file.droppedDuplicateFileNames.length > 0 ||
                file.uploadedTitleMatches.length > 0,
            ).length;
            setNotice(
              `${scan.completedFiles} file${scan.completedFiles === 1 ? "" : "s"} checked before ingest. ${matches} need${matches === 1 ? "s" : ""} duplicate review.`,
            );
          }
        })
        .catch(() => {
          /* A later poll resumes if the app database is briefly busy. */
        });
    }, 450);
    return () => window.clearInterval(interval);
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
        `${cleared} local upload job${cleared === 1 ? " was" : "s were"} cancelled. Managed media copies were retained.`,
      );
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  const importVideos = useCallback(
    async (paths: string[], settings: ManualUploadSettings) => {
      setBusy(true);
      const failures: string[] = [];
      const importedItems: UploadItem[] = [];
      try {
        for (const path of paths) {
          try {
            importedItems.push(await importAsset(path, settings));
          } catch (error) {
            failures.push(
              error instanceof Error
                ? error.message
                : "A video could not be imported.",
            );
          }
        }
        if (importedItems.length === 0) {
          setNotice(
            `${failures.length} video${failures.length === 1 ? "" : "s"} could not be imported. ${failures[0] ?? ""}`.trim(),
          );
          return;
        }
        if (!snapshot.activeChannel) {
          await refresh();
          setNotice(
            `${importedItems.length} video${importedItems.length === 1 ? "" : "s"} safely imported to this device. Connect YouTube to start uploading them.`,
          );
          return;
        }
        let duplicateIds = new Set<string>();
        try {
          duplicateIds = new Set(
            (
              await checkUploadTitleDuplicates(
                importedItems.map((item) => item.id),
              )
            ).map((candidate) => candidate.itemId),
          );
        } catch (error) {
          await refresh();
          setNotice(
            `Videos were imported safely, but the online YouTube title check could not complete. ${error instanceof Error ? error.message : "Try again before queueing."}`,
          );
          return;
        }
        const queueFailures: string[] = [];
        for (const item of importedItems.filter(
          (candidate) => !duplicateIds.has(candidate.id),
        )) {
          try {
            await queueItem(item.id);
          } catch (error) {
            queueFailures.push(
              error instanceof Error
                ? error.message
                : "A video could not be queued.",
            );
          }
        }
        let startNotice = "";
        if (
          queueFailures.length === 0 &&
          importedItems.length > duplicateIds.size
        ) {
          try {
            const started = await startQueuedUploads();
            startNotice = `${started} queued upload${started === 1 ? "" : "s"} started.`;
          } catch (error) {
            startNotice =
              error instanceof Error
                ? error.message
                : "The saved uploads could not be started.";
          }
        }
        await refresh();
        const importNotice =
          duplicateIds.size > 0
            ? `${importedItems.length} video${importedItems.length === 1 ? "" : "s"} imported locally. ${duplicateIds.size} matching title${duplicateIds.size === 1 ? " needs" : "s need"} your decision.`
            : `${importedItems.length} video${importedItems.length === 1 ? "" : "s"} imported and queued locally.`;
        const failuresNotice =
          failures.length > 0 || queueFailures.length > 0
            ? ` ${failures.length + queueFailures.length} item${failures.length + queueFailures.length === 1 ? "" : "s"} need attention.`
            : "";
        setNotice(`${importNotice} ${startNotice}${failuresNotice}`.trim());
      } finally {
        setBusy(false);
      }
    },
    [refresh, snapshot.activeChannel],
  );

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onDragDropEvent((event) => {
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
      })
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
    void getCurrentWindow()
      .onCloseRequested((event) => {
        // Recovery is a safe holding screen. Do not trap the operator in it:
        // closing leaves the persisted crash marker and all resumable work
        // untouched for the next launch.
        if (recoveryModeRef.current) return;
        event.preventDefault();
        setExitConfirmationOpen(true);
      })
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
      // `close()` emits the same close-request event that opened this dialog.
      // Destroy is Tauri's explicit confirmed-close path and bypasses that event.
      await getCurrentWindow().destroy();
    } catch {
      setExitConfirmationOpen(false);
      setNotice("The app could not close. Your saved queue remains unchanged.");
    }
  }, []);

  useEffect(() => {
    if (
      !snapshot.items.some(
        (item) => item.status === "uploading" || item.status === "dispatching",
      )
    )
      return;
    const interval = window.setInterval(() => {
      void refresh();
    }, 1000);
    return () => window.clearInterval(interval);
  }, [refresh, snapshot.items]);

  const addVideo = async () => {
    if (!isTauri) {
      setNotice(
        "Run this screen through Tauri to import a file into the managed local workspace.",
      );
      return;
    }
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: "Video", extensions: [...supportedVideoExtensions] }],
    });
    reviewImport(selectedFilePaths(selected));
  };

  const queue = async (item: UploadItem) => {
    setBusy(true);
    try {
      const duplicates = await checkUploadTitleDuplicates([item.id]);
      if (duplicates.length > 0) {
        await refresh();
        setNotice(
          "A matching uploaded title was found. Choose Upload anyway or Skip duplicate below.",
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
      setNotice(`${updated.fileName} is saved in the local upload queue.`);
    } finally {
      setBusy(false);
    }
  };

  const chooseFilesForPreflight = async (mode: "light" | "deep") => {
    if (!isTauri) {
      setNotice("Run this screen through Tauri to check files before ingest.");
      return;
    }
    const selected = await open({
      multiple: true,
      directory: false,
      pickerMode: "document",
      fileAccessMode: "scoped",
    });
    void runPreflightDuplicateCheck(selectedFilePaths(selected), mode);
  };

  const resolveTitleDuplicates = async (
    itemIds: string[],
    action: "ignore" | "skip",
  ) => {
    setBusy(true);
    try {
      const resolved = await resolveUploadTitleDuplicates(itemIds, action);
      const ignored = resolved.filter((item) => item.status !== "cancelled");
      for (const item of ignored) await queueItem(item.id);
      if (ignored.length > 0) await startQueuedUploads();
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
  };

  const changeVisibility = async (
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
  };

  const changeSourceCleanup = async (
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
  };

  const startUploads = async () => {
    if (!snapshot.activeChannel) return;
    setBusy(true);
    try {
      const started = await startQueuedUploads();
      await refresh();
      setNotice(
        `${started} queued upload${started === 1 ? "" : "s"} started for ${snapshot.activeChannel}.`,
      );
    } catch (error) {
      setNotice(
        error instanceof Error
          ? error.message
          : "Queued uploads could not be started.",
      );
    } finally {
      setBusy(false);
    }
  };

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
      await refresh();
      updateConnection(await loadConnectionSettings());
      setLibraryRefreshVersion((version) => version + 1);
      setNotice(
        `Library refreshed: ${synced} YouTube video${synced === 1 ? "" : "s"} saved locally for ${channel}.`,
      );
    } catch (error) {
      setNotice(
        error instanceof Error
          ? error.message
          : "The YouTube library could not be refreshed. Your last complete local library was kept.",
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

  const ignoreDuplicate = async (candidateId: string) => {
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
  };

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

  if (crashRecovery?.crashDetected) {
    return (
      <CrashRecoveryScreen
        detectedAt={crashRecovery.detectedAt}
        failureKind={crashRecovery.failureKind}
        onContinue={() => setCrashRecovery({ crashDetected: false })}
      />
    );
  }

  return (
    <main className="app-shell">
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
            disabled={!snapshot.activeChannel || busy}
            onClick={() => void startUploads()}
          >
            Start uploads
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
            <GoogleSetupWizard
              onConfigured={(settings) => {
                updateConnection(settings);
                setSetupDismissed(true);
                setActiveTab("account");
                setNotice(
                  "Desktop OAuth JSON imported. Connect YouTube when you are ready.",
                );
              }}
              onDismiss={() => setSetupDismissed(true)}
            />
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
          <UploadIntakeReview
            paths={pendingImportPaths}
            onCancel={() => setPendingImportPaths(undefined)}
            onConfirm={(settings) => {
              const paths = pendingImportPaths;
              setPendingImportPaths(undefined);
              void importVideos(paths, settings);
            }}
          />
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
          <section
            aria-labelledby="workspace-tab-button-batch"
            className="workspace-tab"
            hidden={activeTab !== "batch"}
            id="workspace-tab-batch"
            role="tabpanel"
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
                    any upload. You can also choose multiple files.
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
                onQueue={(item) => void queue(item)}
                onVisibilityChange={(item, visibility) =>
                  void changeVisibility(item, visibility)
                }
                onDeleteSourceAfterUploadChange={(item, enabled) =>
                  void changeSourceCleanup(item, enabled)
                }
              />
            </section>
          </section>

          <section
            aria-labelledby="workspace-tab-button-monitor"
            className="workspace-tab"
            hidden={activeTab !== "monitor"}
            id="workspace-tab-monitor"
            role="tabpanel"
          >
            <FolderMonitorPanel
              activeChannel={snapshot.activeChannel}
              onNotice={setNotice}
              onQueueRefresh={async () => {
                await refresh();
              }}
            />
          </section>

          <section
            aria-labelledby="workspace-tab-button-dedupe"
            className="workspace-tab"
            hidden={activeTab !== "dedupe"}
            id="workspace-tab-dedupe"
            role="tabpanel"
          >
            <PreIngestDuplicatePanel
              busy={preflightBusy}
              fileCount={preflightFileCount}
              dropActive={preflightDropActive}
              onCancel={() => void cancelPreflight()}
              onChoose={(mode) => void chooseFilesForPreflight(mode)}
              onPrepareLocalDuplicateDelete={prepareLocalDuplicateDelete}
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
                candidates={snapshot.duplicates}
                onIgnore={(candidateId) => ignoreDuplicate(candidateId)}
                onDeletionComplete={async (videoId, title) => {
                  await refresh();
                  setNotice(
                    `YouTube confirmed permanent deletion of ${videoId} (“${title}”). The local execution receipt was saved.`,
                  );
                }}
                onBulkDeletionComplete={async (count) => {
                  await refresh();
                  setNotice(
                    `YouTube confirmed permanent deletion of ${count} selected video${count === 1 ? "" : "s"}. Local execution receipts were saved.`,
                  );
                }}
              />
              {dedupeActivity.length > 0 && (
                <section
                  className="dedupe-activity"
                  aria-labelledby="dedupe-activity-heading"
                >
                  <div className="dedupe-activity__heading">
                    <div>
                      <p className="eyebrow">DEVICE-LOCAL ACTIVITY</p>
                      <h3 id="dedupe-activity-heading">Dedupe activity</h3>
                    </div>
                    {dedupeBusy && (
                      <span className="dedupe-activity__running">
                        In progress
                      </span>
                    )}
                  </div>
                  <div
                    className={`dedupe-progress dedupe-progress--${dedupePhase}`}
                  >
                    <div className="dedupe-progress__heading">
                      <span>Phase progress</span>
                      <strong>
                        {dedupeProgressStep(dedupePhase)} of{" "}
                        {dedupeProgressStepCount}
                      </strong>
                    </div>
                    <div
                      aria-describedby="dedupe-progress-detail"
                      aria-label={`Dedupe phase progress: ${dedupeProgressLabel(dedupePhase)}`}
                      aria-valuemax={dedupeProgressStepCount}
                      aria-valuemin={0}
                      aria-valuenow={dedupeProgressStep(dedupePhase)}
                      className="dedupe-progress__track"
                      role="progressbar"
                    >
                      <span
                        style={{
                          width: `${(dedupeProgressStep(dedupePhase) / dedupeProgressStepCount) * 100}%`,
                        }}
                      />
                    </div>
                    <p id="dedupe-progress-detail">
                      {dedupeProgressLabel(dedupePhase)}
                    </p>
                  </div>
                  <ol
                    className="dedupe-activity__list"
                    aria-live="polite"
                    aria-label="Dedupe activity log"
                  >
                    {dedupeActivity.map((entry) => (
                      <li
                        className={`dedupe-activity__entry dedupe-activity__entry--${entry.state}`}
                        key={entry.id}
                      >
                        <span aria-hidden="true" />
                        {entry.message}
                      </li>
                    ))}
                  </ol>
                </section>
              )}
            </section>
          </section>

          <section
            aria-labelledby="workspace-tab-button-deletion"
            className="workspace-tab"
            hidden={activeTab !== "deletion"}
            id="workspace-tab-deletion"
            role="tabpanel"
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
                busy={busy}
                onNotice={setNotice}
                refreshVersion={libraryRefreshVersion}
              />
            </section>
          </section>

          <section
            aria-labelledby="workspace-tab-button-transfer"
            className="workspace-tab"
            hidden={activeTab !== "transfer"}
            id="workspace-tab-transfer"
            role="tabpanel"
          >
            <TransferPanel
              onConnectionChange={updateConnection}
              onNotice={setNotice}
            />
          </section>

          <section
            aria-labelledby="workspace-tab-button-account"
            className="workspace-tab"
            hidden={activeTab !== "account"}
            id="workspace-tab-account"
            role="tabpanel"
          >
            <ConnectionPanel onConnectionChange={updateConnection} />
          </section>

          <section
            aria-labelledby="workspace-tab-button-about"
            className="workspace-tab"
            hidden={activeTab !== "about"}
            id="workspace-tab-about"
            role="tabpanel"
          >
            <DiagnosticsPanel />
          </section>
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
