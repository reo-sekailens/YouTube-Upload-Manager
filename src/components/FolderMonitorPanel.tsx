import { useEffect, useMemo, useRef, useState } from "react";
import "./FolderMonitorPanel.lazy.css";
import {
  createYouTubePlaylist,
  disableFolderMonitor,
  enableFolderMonitor,
  isTauri,
  listYouTubePlaylists,
  loadFolderMonitorOverview,
  requeueCancelledFolderMonitorFiles,
  reconcileFolderMonitorUploads,
  deleteFolderMonitorUploadedSource,
  deleteFolderMonitorUploadedSources,
  scanFolderMonitorNow,
} from "../lib/local";
import { listen } from "@tauri-apps/api/event";
import type {
  FolderMonitorSettings,
  FolderMonitorFileActivity,
  FolderMonitorLogEntry,
  FolderMonitorVisibility,
  FolderMonitorLocalDeleteResult,
  YouTubePlaylist,
} from "../lib/types";
import { useRetainedWorkspaceState } from "../lib/retained-workspace-state";
import { subscribeLocalStateChanges } from "../lib/state-events";

const unavailable: FolderMonitorSettings = {
  enabled: false,
  visibility: "private",
  madeForKids: false,
  deleteSourceAfterUpload: false,
  status: "disabled",
  detail: "Folder monitoring is off.",
};

type FolderMonitorPanelProps = {
  activeChannel?: string;
  activeChannelId?: string;
  onNotice: (message: string) => void;
  onQueueRefresh: () => Promise<void>;
};

function statusLabel(settings: FolderMonitorSettings) {
  if (!settings.enabled) return "Off";
  if (settings.status === "watching") return "Watching";
  if (settings.status === "scanning") return "Scanning";
  if (settings.status === "paused") return "Paused";
  if (settings.status === "error") return "Needs attention";
  return settings.status.replaceAll("_", " ");
}

function formatScanTime(value?: string) {
  if (!value) return "Not scanned yet";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MB`;
  return `${(value / 1024 ** 3).toFixed(1)} GB`;
}

function fileProgress(file: FolderMonitorFileActivity) {
  if (!file.totalBytes || file.totalBytes <= 0 || file.confirmedBytes === undefined)
    return undefined;
  return Math.min(100, Math.round((file.confirmedBytes / file.totalBytes) * 100));
}

export function FolderMonitorPanel({
  activeChannel,
  activeChannelId,
  onNotice,
  onQueueRefresh,
}: FolderMonitorPanelProps) {
  const [settings, setSettings] = useState<FolderMonitorSettings>(unavailable);
  const [files, setFiles] = useState<FolderMonitorFileActivity[]>([]);
  const [logs, setLogs] = useState<FolderMonitorLogEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [visibility, setVisibility] =
    useRetainedWorkspaceState<FolderMonitorVisibility>(
      "monitor.visibility",
      "private",
    );
  const [madeForKids, setMadeForKids] = useRetainedWorkspaceState(
    "monitor.made-for-kids",
    false,
  );
  const [deleteSourceAfterUpload, setDeleteSourceAfterUpload] =
    useRetainedWorkspaceState("monitor.delete-source-after-upload", false);
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistId, setPlaylistId] = useRetainedWorkspaceState(
    "monitor.playlist-id",
    "",
  );
  const [newPlaylistTitle, setNewPlaylistTitle] = useRetainedWorkspaceState(
    "monitor.new-playlist-title",
    "",
  );
  const [creatingPlaylist, setCreatingPlaylist] = useState(false);
  const [deletionQueue, setDeletionQueue] = useState<FolderMonitorFileActivity[]>([]);
  const [deletionConfirmation, setDeletionConfirmation] = useState("");
  const [deletionSubmitting, setDeletionSubmitting] = useState(false);
  const [deletionProgress, setDeletionProgress] = useState({ completed: 0, total: 0 });
  const [deletionResults, setDeletionResults] = useState<FolderMonitorLocalDeleteResult[]>([]);
  const invalidationTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    let active = true;
    let loading = false;
    let unsubscribe: (() => void) | undefined;
    const load = async () => {
      if (loading) return;
      loading = true;
      try {
        const loaded = await loadFolderMonitorOverview();
        if (!active) return;
        setSettings(loaded.settings);
        setFiles(loaded.files);
        setLogs(loaded.logs);
        setLoadError("");
      } catch {
        if (active)
          setLoadError(
            "Folder monitoring status could not be loaded from this device.",
          );
      } finally {
        loading = false;
      }
    };

    void load();
    if (isTauri && activeChannelId) {
      void subscribeLocalStateChanges((batch) => {
        if (
          !batch.changes.some(
            (change) =>
              change.channelId === activeChannelId &&
              ["folder_monitor", "monitor", "upload"].includes(change.surface),
          ) ||
          invalidationTimer.current !== undefined
        )
          return;
        invalidationTimer.current = window.setTimeout(() => {
          invalidationTimer.current = undefined;
          void load();
        }, 75);
      })
        .then((stop) => {
          if (active) unsubscribe = stop;
          else stop();
        })
        .catch(() => undefined);
    }
    return () => {
      active = false;
      unsubscribe?.();
      if (invalidationTimer.current !== undefined) {
        window.clearTimeout(invalidationTimer.current);
        invalidationTimer.current = undefined;
      }
    };
  }, [activeChannel, activeChannelId]);

  useEffect(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    void listen<{ completed: number; total: number; result: FolderMonitorLocalDeleteResult }>(
      "folder-monitor-local-delete-progress",
      (event) => {
        setDeletionProgress({ completed: event.payload.completed, total: event.payload.total });
        setDeletionResults((previous) => [...previous, event.payload.result]);
      },
    ).then((stop) => { unlisten = stop; }).catch(() => undefined);
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!isTauri || !activeChannel) {
      setPlaylists([]);
      return;
    }
    void listYouTubePlaylists()
      .then(setPlaylists)
      .catch(() => setPlaylists([]));
  }, [activeChannel]);

  const chooseAndEnable = async () => {
    if (!isTauri || !activeChannel) return;
    setBusy(true);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== "string") return;
      const playlist = playlists.find(
        (candidate) => candidate.id === playlistId,
      );
      const updated = await enableFolderMonitor(
        selected,
        visibility,
        madeForKids,
        deleteSourceAfterUpload,
        playlist?.id,
        playlist?.title,
      );
      setSettings(updated);
      setLoadError("");
      onNotice(
        `Folder monitoring is enabled for ${updated.channelName ?? activeChannel}. New completed videos will be copied locally, queued, and start uploading as ${updated.visibility} once stable.`,
      );
    } catch (error) {
      onNotice(
        error instanceof Error
          ? error.message
          : "Folder monitoring could not be enabled.",
      );
    } finally {
      setBusy(false);
    }
  };

  const createPlaylist = async () => {
    const title = newPlaylistTitle.trim();
    if (!title) return;
    setCreatingPlaylist(true);
    setLoadError("");
    try {
      const playlist = await createYouTubePlaylist(title);
      setPlaylists((current) =>
        [...current, playlist].sort((left, right) => left.title.localeCompare(right.title)),
      );
      setPlaylistId(playlist.id);
      setNewPlaylistTitle("");
      onNotice(`Created private playlist “${playlist.title}” and selected it for this folder.`);
    } catch (error) {
      setLoadError(
        error instanceof Error ? error.message : "The playlist could not be created.",
      );
    } finally {
      setCreatingPlaylist(false);
    }
  };

  const scanNow = async () => {
    if (!isTauri || !settings.enabled) return;
    setBusy(true);
    try {
      const updated = await scanFolderMonitorNow();
      setSettings(updated);
      void onQueueRefresh().catch(() =>
        setLoadError("The upload queue could not be refreshed after the folder scan."),
      );
      onNotice(updated.detail);
    } catch (error) {
      onNotice(
        error instanceof Error
          ? error.message
          : "The watched folder could not be scanned.",
      );
    } finally {
      setBusy(false);
    }
  };

  const disable = async () => {
    if (!isTauri || !settings.enabled) return;
    setBusy(true);
    try {
      const updated = await disableFolderMonitor();
      setSettings(updated);
      onNotice(
        "Folder monitoring is disabled. Existing queued files and source videos were not changed.",
      );
    } catch (error) {
      onNotice(
        error instanceof Error
          ? error.message
          : "Folder monitoring could not be disabled.",
      );
    } finally {
      setBusy(false);
    }
  };

  const activeStatus = settings.enabled && settings.status === "watching";
  const uploadingFiles = useMemo(
    () => files.filter((file) => ["importing", "dispatching", "uploading"].includes(file.uploadStatus ?? file.observationState)),
    [files],
  );
  const queuedFiles = useMemo(
    () => files.filter((file) => ["draft", "queued"].includes(file.uploadStatus ?? file.observationState)),
    [files],
  );
  const reconciliationFiles = useMemo(
    () => files.filter((file) => file.uploadStatus === "needs_reconciliation"),
    [files],
  );
  const cancelledFiles = useMemo(
    () =>
      files.filter(
        (file) =>
          file.uploadStatus === "cancelled" &&
          file.itemId &&
          !["duplicate", "duplicate_title", "hash_failed", "rejected"].includes(
            file.observationState,
          ),
      ),
    [files],
  );
  const uploadedFiles = useMemo(
    () =>
      files.filter(
        (file) =>
          file.itemId &&
          file.videoId &&
          file.liveConfirmed &&
          file.localSourceAvailable &&
          file.sourceDeleteStatus !== "deleted" &&
          file.uploadStatus === "uploaded",
      ),
    [files],
  );
  const recentFiles = useMemo(() => files.slice(0, 24), [files]);

  const openDeletionReview = (selected: FolderMonitorFileActivity[]) => {
    if (!isTauri || busy || deletionSubmitting || selected.length === 0) return;
    setDeletionQueue(selected);
    setDeletionConfirmation("");
    setDeletionProgress({ completed: 0, total: selected.length });
    setDeletionResults([]);
  };

  const deleteLocalSource = async () => {
    const target = deletionQueue[0];
    if (!isTauri || !target?.itemId || deletionSubmitting)
      return;
    setDeletionSubmitting(true);
    setDeletionProgress({ completed: 0, total: deletionQueue.length });
    setDeletionResults([]);
    try {
      const bulk = deletionQueue.length > 1;
      if (bulk) {
        const results = await deleteFolderMonitorUploadedSources(
          deletionQueue.flatMap((file) => file.itemId ? [file.itemId] : []),
          deletionConfirmation,
        );
        setDeletionResults((previous) => previous.length === results.length ? previous : results);
        setDeletionProgress({ completed: results.length, total: results.length });
      } else {
        const result = await deleteFolderMonitorUploadedSource(target.itemId, deletionConfirmation);
        setDeletionResults([result]);
        setDeletionProgress({ completed: 1, total: 1 });
      }
      const remaining = bulk ? [] : deletionQueue.slice(1);
      setDeletionQueue(remaining);
      setDeletionConfirmation("");
      const refreshed = await loadFolderMonitorOverview();
      setSettings(refreshed.settings);
      setFiles(refreshed.files);
      setLogs(refreshed.logs);
      onNotice(
        remaining.length > 0
          ? `Deleted the local source for “${target.fileName}”. Confirm the next local watched file.`
          : "Local source deletion finished. See the deletion log for any file retained by a safety check.",
      );
    } catch (error) {
      onNotice(
        error instanceof Error
          ? error.message
          : "The local watched file could not be deleted.",
      );
    } finally {
      setDeletionSubmitting(false);
    }
  };

  const requeueCancelled = async (itemIds: string[]) => {
    if (!isTauri || busy || itemIds.length === 0) return;
    setBusy(true);
    try {
      const requeued = await requeueCancelledFolderMonitorFiles(itemIds);
      const refreshed = await loadFolderMonitorOverview();
      setSettings(refreshed.settings);
      setFiles(refreshed.files);
      setLogs(refreshed.logs);
      await onQueueRefresh();
      onNotice(
        `${requeued} cancelled watched-folder ${requeued === 1 ? "file was" : "files were"} queued again and will resume automatically.`,
      );
    } catch (error) {
      onNotice(
        typeof error === "string" && error.trim()
          ? error
          : error instanceof Error
            ? error.message
            : "The cancelled watched-folder files could not be queued again.",
      );
    } finally {
      setBusy(false);
    }
  };

  const reconcileUploads = async () => {
    if (!isTauri || busy || reconciliationFiles.length === 0) return;
    setBusy(true);
    try {
      const retried = await reconcileFolderMonitorUploads();
      const refreshed = await loadFolderMonitorOverview();
      setSettings(refreshed.settings);
      setFiles(refreshed.files);
      setLogs(refreshed.logs);
      await onQueueRefresh();
      onNotice(retried > 0 ? `${retried} watched upload${retried === 1 ? " was" : "s were"} absent from YouTube and queued for upload.` : "YouTube confirmed the ambiguous watched uploads; no duplicate upload was sent.");
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "Watched-upload reconciliation could not complete.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section
      className="rounded-xl border border-[#dce1e8] bg-white p-5"
      aria-labelledby="folder-monitor-heading"
    >
      <header className="mb-4 flex items-start justify-between gap-4 max-sm:flex-col">
        <div>
          <p className="mb-2 text-[0.67rem] font-bold tracking-[0.1em] text-[#68748a] uppercase">OPT-IN AUTOMATION</p>
          <h2 className="m-0 text-[1.15rem] font-bold tracking-[-0.035em] text-[#172033]" id="folder-monitor-heading">Watched folder uploads</h2>
          <p className="mt-2 mb-0 max-w-3xl text-[0.79rem] leading-relaxed text-[#64758a]">
            Monitor one local folder while this app is running and send newly
            added, completed video files to the channel you approve.
          </p>
        </div>
        <span
          className={`inline-flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-1.5 text-[0.72rem] font-[680] capitalize whitespace-nowrap ${activeStatus ? "bg-[#eaf7ef] text-[#26714e]" : settings.enabled ? "bg-[#fff7e3] text-[#866317]" : "bg-[#f2f4f7] text-[#68748a]"}`}
        >
          <span aria-hidden="true" className="size-1.5 rounded-full bg-current" />
          {statusLabel(settings)}
        </span>
      </header>

      <div className="grid gap-1 rounded-lg border border-[#d7e4ff] bg-[#f0f5ff] px-3.5 py-3">
        <strong className="text-[0.82rem] text-[#284b82]">
          New videos upload automatically as{" "}
          {settings.enabled ? settings.visibility : visibility}.
        </strong>
        <span className="text-[0.76rem] leading-relaxed text-[#536987]">
          Enabling this is recurring approval to upload supported files directly
          from this folder to the bound YouTube channel. No full app copy is
          made. Every supported direct-child file is accepted automatically once
          it remains unchanged across two scans. Keep each source available and
          unchanged until YouTube confirms it; optional cleanup runs only then.
        </span>
      </div>

      {settings.enabled ? (
        <div className="mt-3.5">
          <dl className="grid grid-cols-2 gap-2.5 m-0 max-sm:grid-cols-1">
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Folder</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]" title={settings.folderPath}>
                {settings.folderPath ?? "Unavailable"}
              </dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Bound channel</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{settings.channelName ?? "Unavailable"}</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Visibility</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{settings.visibility}</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Audience</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">
                {settings.madeForKids ? "Made for kids" : "Not made for kids"}
              </dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Original source cleanup</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">
                {settings.deleteSourceAfterUpload
                  ? "Automatic after confirmed upload"
                  : "Keep original"}
              </dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Transfer source</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">Watched file in place</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Playlist</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{settings.playlistTitle ?? "No playlist"}</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Last scan</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{formatScanTime(settings.lastScanAt)}</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Last file</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{settings.lastFileName ?? "No file processed yet"}</dd>
            </div>
          </dl>
          <p className="mt-3 mb-0 text-[0.76rem] leading-relaxed text-[#52617a]" role="status">
            {settings.detail}
          </p>
          <p className="mt-2 mb-0 text-[0.72rem] text-[#487057]">
            Automatic scanning checks this folder every 5 seconds while monitoring is enabled and the app is running.
          </p>
          <section className="mt-3.5 rounded-lg border border-[#d8e5f8] bg-[#f8fbff] p-3" aria-labelledby="folder-monitor-live-heading">
            <div className="flex items-center justify-between gap-3 max-sm:flex-col max-sm:items-start max-sm:gap-1">
              <div>
                <p className="mb-0.5 text-[0.67rem] font-bold tracking-[0.1em] text-[#68748a] uppercase">THIS FOLDER</p>
                <h3 className="m-0 text-[0.92rem] text-[#25314a]" id="folder-monitor-live-heading">Live folder activity</h3>
              </div>
              <span className="text-[0.7rem] font-bold text-[#3d668e] whitespace-nowrap">{uploadingFiles.length} uploading · {queuedFiles.length} queued{reconciliationFiles.length > 0 ? ` · ${reconciliationFiles.length} to reconcile` : ""}</span>
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2.5 max-sm:grid-cols-1">
              <div className="min-w-0 rounded-md border border-[#e2e8f0] bg-white p-2.5">
                <h4 className="m-0 text-[0.72rem] text-[#384966]">Uploading now</h4>
                {uploadingFiles.length === 0 ? (
                  <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No files are currently being uploaded.</p>
                ) : (
                  <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                    {uploadingFiles.map((file) => {
                      const progress = fileProgress(file);
                      return (
                        <li className="border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={`${file.fileName}-${file.updatedAt}`}>
                          <div className="flex items-baseline justify-between gap-2">
                            <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]" title={file.fileName}>{file.fileName}</strong>
                            <span className="text-[0.66rem] text-[#4671a4] capitalize">{file.uploadStatus ?? file.observationState}</span>
                          </div>
                          <p className="mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{formatBytes(file.sizeBytes)}{progress === undefined ? "" : ` · ${progress}% uploaded`}</p>
                          {progress !== undefined && <progress className="mt-1 w-full accent-[#2463df]" aria-label={`${file.fileName} upload progress`} max={100} value={progress} />}
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
              <div className="min-w-0 rounded-md border border-[#e2e8f0] bg-white p-2.5">
                <h4 className="m-0 text-[0.72rem] text-[#384966]">Waiting in this folder’s queue</h4>
                {queuedFiles.length === 0 ? (
                  <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No watched-folder files are awaiting upload.</p>
                ) : (
                  <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                    {queuedFiles.map((file) => (
                      <li className="border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={`${file.fileName}-${file.updatedAt}`}>
                        <div className="flex items-baseline justify-between gap-2">
                          <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]" title={file.fileName}>{file.fileName}</strong>
                          <span className="text-[0.66rem] text-[#4671a4] capitalize">{file.uploadStatus ?? file.observationState}</span>
                        </div>
                        <p className="mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{formatBytes(file.sizeBytes)} · updated {formatScanTime(file.updatedAt)}</p>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
            {reconciliationFiles.length > 0 && (
              <div className="mt-2.5 rounded-md border border-[#f0d39e] bg-[#fffaf0] p-2.5">
                <div className="flex items-center justify-between gap-3 max-sm:flex-col max-sm:items-start">
                  <div>
                    <strong className="text-[0.72rem] text-[#805b16]">{reconciliationFiles.length} upload{reconciliationFiles.length === 1 ? " needs" : "s need"} reconciliation</strong>
                    <p className="mt-1 mb-0 text-[0.68rem] leading-snug text-[#806b43]">They are not treated as queued. Reconcile checks YouTube first, then retries only video IDs absent from the active channel.</p>
                  </div>
                  <button className="shrink-0 rounded-md border border-[#d9b970] bg-white px-3 py-2 text-[0.74rem] font-[680] text-[#765515] disabled:cursor-not-allowed disabled:opacity-50" disabled={busy} onClick={() => void reconcileUploads()} type="button">{busy ? "Checking YouTube…" : `Reconcile and upload (${reconciliationFiles.length})`}</button>
                </div>
              </div>
            )}
            <details className="mt-3 border-t border-[#dfe7f2] pt-2">
              <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Folder scan log ({logs.length})</summary>
              {logs.length === 0 ? (
                <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No scan events have been recorded for this folder yet.</p>
              ) : (
                <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                  {logs.map((log) => (
                    <li className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-2 border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={`${log.createdAt}-${log.kind}`}>
                      <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]">{log.kind.replaceAll("_", " ")}</strong>
                      <span className="text-[0.64rem] text-[#6e7d91] whitespace-nowrap">{formatScanTime(log.createdAt)}</span>
                      <p className="col-span-full mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{log.detail ?? "No further detail recorded."}</p>
                    </li>
                  ))}
                </ul>
              )}
            </details>
            <details className="mt-3 border-t border-[#dfe7f2] pt-2">
              <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Recently observed files ({files.length})</summary>
              <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                {recentFiles.map((file) => (
                  <li className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-2 border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={`${file.fileName}-${file.updatedAt}`}>
                    <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]">{file.fileName}</strong>
                    <span className="text-[0.64rem] text-[#6e7d91] whitespace-nowrap">{formatScanTime(file.updatedAt)}</span>
                    <p className="col-span-full mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{file.uploadTitle ? `Queued as ${file.uploadTitle}` : `${formatBytes(file.sizeBytes)} · ${file.observationState}`}</p>
                  </li>
                ))}
              </ul>
            </details>
            <details className="mt-3 border-t border-[#dfe7f2] pt-2">
              <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Cancelled watched files ({cancelledFiles.length})</summary>
              {cancelledFiles.length === 0 ? (
                <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No eligible cancelled watched-folder files are waiting to be queued again.</p>
              ) : (
                <>
                  <p className="my-2 text-[0.68rem] leading-snug text-[#68788e]">Cancelled files stay out of the queue until you explicitly add them back. Duplicate and integrity-safety stops cannot be requeued here.</p>
                  <button
                    className="rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] transition-colors hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={busy}
                    onClick={() => void requeueCancelled(cancelledFiles.flatMap((file) => file.itemId ? [file.itemId] : []))}
                    type="button"
                  >
                    {busy ? "Working…" : `Queue all ${cancelledFiles.length} again`}
                  </button>
                  <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                    {cancelledFiles.map((file) => (
                      <li className="flex items-center justify-between gap-2 border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={`${file.itemId}-${file.updatedAt}`}>
                        <div className="min-w-0">
                          <div className="flex items-baseline justify-between gap-2">
                            <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]" title={file.fileName}>{file.fileName}</strong>
                            <span className="text-[0.66rem] text-[#4671a4] capitalize">cancelled</span>
                          </div>
                          <p className="mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{formatBytes(file.sizeBytes)} · cancelled {formatScanTime(file.updatedAt)}</p>
                        </div>
                        <button
                          className="shrink-0 rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] transition-colors hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-50"
                          disabled={busy}
                          onClick={() => void requeueCancelled(file.itemId ? [file.itemId] : [])}
                          type="button"
                        >
                          Queue again
                        </button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </details>
            <details className="mt-4 border-t border-[#dfe7f2] pt-3">
              <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Uploaded to YouTube ({uploadedFiles.length})</summary>
              <p className="mt-2 mb-3 max-w-4xl text-[0.7rem] leading-relaxed text-[#68788e]">Only watched files whose completed upload still appears in the authenticated active-channel YouTube inventory are shown. Delete removes the local watched source only; the YouTube video and managed app copy are retained.</p>
              {uploadedFiles.length === 0 ? (
                <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No watched-folder uploads are currently confirmed by YouTube.</p>
              ) : (
                <>
                  <button
                    className="rounded-md border border-[#e5c2c0] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#a4413b] transition-colors hover:border-[#d89d98] hover:bg-[#fff5f4] disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={busy || deletionSubmitting}
                    onClick={() => openDeletionReview(uploadedFiles)}
                    type="button"
                  >
                    Delete ({uploadedFiles.length}) files…
                  </button>
                  {deletionQueue[0] && (
                    <div className="mt-3 grid gap-3 rounded-lg border border-[#e5c2c0] bg-[#fff8f7] p-3.5" role="dialog" aria-label="Confirm local watched file deletion">
                      <div className="grid gap-1">
                        <strong className="text-[0.75rem] text-[#8f3731]">{deletionQueue.length > 1 ? `Delete (${deletionQueue.length}) files` : "Delete local file"}</strong>
                        <p className="m-0 text-[0.69rem] leading-relaxed text-[#7b514d]">{deletionQueue.length > 1 ? <>Type <code className="rounded bg-white px-1 py-0.5 text-[#8f3731]">DELETE {deletionQueue.length} FILES</code> to permanently delete all selected local watched files. The YouTube videos are not deleted.</> : <>Type <code className="rounded bg-white px-1 py-0.5 text-[#8f3731]">{deletionQueue[0].fileName}</code> to permanently delete that local watched file. The YouTube video is not deleted.</>}</p>
                      </div>
                      <div className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 max-sm:grid-cols-1">
                        <input
                          aria-label={deletionQueue.length > 1 ? "Exact bulk deletion phrase" : "Exact local filename"}
                          className="min-w-0 rounded-md border border-[#d9b8b4] bg-white px-3 py-2.5 text-[0.74rem] text-[#503532]"
                          disabled={deletionSubmitting}
                          onChange={(event) => setDeletionConfirmation(event.target.value)}
                          placeholder={deletionQueue.length > 1 ? `DELETE ${deletionQueue.length} FILES` : "Exact local filename"}
                          value={deletionConfirmation}
                        />
                        <button className="rounded-md border border-[#cbd3df] bg-white px-3 py-2.5 text-[0.74rem] font-[680] text-[#34405a] disabled:cursor-not-allowed disabled:opacity-50" disabled={deletionSubmitting} onClick={() => { setDeletionQueue([]); setDeletionConfirmation(""); }} type="button">Cancel</button>
                        <button className="rounded-md border border-[#b94842] bg-[#b94842] px-3 py-2.5 text-[0.74rem] font-[680] text-white disabled:cursor-not-allowed disabled:opacity-50" disabled={deletionSubmitting || deletionConfirmation.trim() !== (deletionQueue.length > 1 ? `DELETE ${deletionQueue.length} FILES` : deletionQueue[0].fileName)} onClick={() => void deleteLocalSource()} type="button">{deletionSubmitting ? "Deleting…" : deletionQueue.length > 1 ? `Delete (${deletionQueue.length}) files` : "Delete local file"}</button>
                      </div>
                      {deletionSubmitting && (
                        <div className="grid gap-1.5" aria-live="polite">
                          <div className="flex items-center justify-between gap-2 text-[0.68rem] font-semibold text-[#7b514d]"><span>Deleting local watched files…</span><span>{deletionProgress.completed} of {deletionProgress.total}</span></div>
                          <progress className="h-2 w-full accent-[#b94842]" max={Math.max(deletionProgress.total, 1)} value={deletionProgress.completed} />
                        </div>
                      )}
                    </div>
                  )}
                  {deletionResults.length > 0 && (
                    <details className="mt-3 rounded-lg border border-[#dfe7f2] bg-white px-3 py-2" open={deletionResults.some((result) => result.status !== "deleted")}>
                      <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Deletion log ({deletionResults.length} files)</summary>
                      <div className="mt-2 grid gap-1.5" aria-live="polite">
                        <div className="flex items-center justify-between gap-2 text-[0.68rem] text-[#68788e]"><span>{deletionResults.filter((result) => result.status === "deleted").length} deleted locally · {deletionResults.filter((result) => result.status !== "deleted").length} retained</span><span>{deletionProgress.completed} of {deletionProgress.total || deletionResults.length}</span></div>
                        <progress className="h-2 w-full accent-[#26714e]" max={Math.max(deletionProgress.total || deletionResults.length, 1)} value={Math.min(deletionProgress.completed || deletionResults.length, deletionProgress.total || deletionResults.length)} />
                        <ul className="m-0 grid max-h-48 list-none gap-1.5 overflow-auto p-0">
                          {deletionResults.map((result) => <li className="grid gap-0.5 border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={result.itemId}><div className="flex items-center justify-between gap-2"><strong className="truncate text-[0.7rem] text-[#2f4262]" title={result.fileName}>{result.fileName}</strong><span className={result.status === "deleted" ? "text-[0.65rem] font-bold text-[#26714e]" : "text-[0.65rem] font-bold text-[#a4413b]"}>{result.status === "deleted" ? "Deleted locally" : "Retained"}</span></div><p className="m-0 text-[0.65rem] leading-snug text-[#68788e]">{result.detail}</p></li>)}
                        </ul>
                      </div>
                    </details>
                  )}
                  <ul className="mt-3 grid max-h-64 list-none gap-2 overflow-auto p-0">
                    {uploadedFiles.map((file) => (
                      <li className="flex items-center justify-between gap-3 border-t border-[#edf0f5] py-2 first:border-t-0 first:pt-0" key={`${file.itemId}-${file.videoId}`}>
                        <div className="min-w-0">
                          <div className="flex items-baseline justify-between gap-2">
                            <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]" title={file.fileName}>{file.uploadTitle ?? file.fileName}</strong>
                            <span className="text-[0.66rem] font-bold text-[#26714e]">YouTube confirmed</span>
                          </div>
                          <p className="mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{file.fileName} · video ID {file.videoId}</p>
                        </div>
                        <button className="shrink-0 rounded-md border border-[#e5c2c0] bg-white px-3 py-2.5 text-[0.79rem] font-[680] text-[#a4413b] disabled:cursor-not-allowed disabled:opacity-50" disabled={busy || deletionSubmitting} onClick={() => openDeletionReview([file])} type="button">Delete local file…</button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </details>
          </section>
          <div className="mt-3 flex flex-wrap gap-2">
            <button
              className="rounded-md border border-[#2463df] bg-[#2463df] px-3 py-2 text-[0.79rem] leading-tight font-[680] text-white transition-[background,border-color,box-shadow] hover:border-[#1b54c6] hover:bg-[#1b54c6] hover:shadow-[0_3px_8px_rgba(31,78,181,0.16)] disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy}
              onClick={() => void scanNow()}
              type="button"
            >
              {busy ? "Working…" : "Refresh scan"}
            </button>
            <button
              className="rounded-md border border-[#e5c2c0] bg-white px-3 py-2 text-[0.79rem] leading-tight font-[680] text-[#a4413b] transition-[background,border-color] hover:border-[#d89d98] hover:bg-[#fff5f4] disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy}
              onClick={() => void disable()}
              type="button"
            >
              Disable monitor
            </button>
          </div>
        </div>
      ) : (
        <div className="mt-3.5 flex items-center justify-between gap-3 max-sm:flex-col max-sm:items-stretch">
          <div>
            <p className="m-0 text-[0.79rem] text-[#5f6c80]">
              {activeChannel
                ? `Ready to bind a folder to ${activeChannel}.`
                : "Connect a YouTube channel before choosing a folder."}
            </p>
            <div className="mt-3 flex flex-wrap gap-2.5">
              <label className="flex flex-[11rem] items-center gap-2 text-[0.72rem] font-[680] text-[#4f6078]">
                <span>Automatic upload visibility</span>
                <select className="rounded-md border border-[#cbd5e3] bg-white px-2 py-1.5 text-[0.74rem] font-[650] text-[#344a67] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
                  disabled={!activeChannel || busy}
                  onChange={(event) =>
                    setVisibility(event.target.value as FolderMonitorVisibility)
                  }
                  value={visibility}
                >
                  <option value="private">Private</option>
                  <option value="unlisted">Unlisted</option>
                </select>
              </label>
              <label className="flex flex-[11rem] items-start gap-1.5 text-[0.68rem] leading-snug font-semibold text-[#4f6078]">
                <input className="mt-0.5 shrink-0 accent-[#2463df]"
                  checked={deleteSourceAfterUpload}
                  disabled={!activeChannel || busy}
                  onChange={(event) =>
                    setDeleteSourceAfterUpload(event.target.checked)
                  }
                  type="checkbox"
                />{" "}
                Automatically delete original only after YouTube confirms each upload
              </label>
              <label className="flex flex-[11rem] items-center gap-2 text-[0.72rem] font-[680] text-[#4f6078]">
                <span>Audience</span>
                <select className="rounded-md border border-[#cbd5e3] bg-white px-2 py-1.5 text-[0.74rem] font-[650] text-[#344a67] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
                  disabled={!activeChannel || busy}
                  onChange={(event) =>
                    setMadeForKids(event.target.value === "yes")
                  }
                  value={madeForKids ? "yes" : "no"}
                >
                  <option value="no">Not made for kids</option>
                  <option value="yes">Made for kids</option>
                </select>
              </label>
              <label className="flex flex-[11rem] items-center gap-2 text-[0.72rem] font-[680] text-[#4f6078]">
                <span>Add to playlist</span>
                <select className="rounded-md border border-[#cbd5e3] bg-white px-2 py-1.5 text-[0.74rem] font-[650] text-[#344a67] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
                  disabled={!activeChannel || busy}
                  onChange={(event) => setPlaylistId(event.target.value)}
                  value={playlistId}
                >
                  <option value="">No playlist</option>
                  {playlists.map((playlist) => (
                    <option key={playlist.id} value={playlist.id}>
                      {playlist.title}
                    </option>
                  ))}
                </select>
              </label>
              <div className="grid flex-[11rem] self-end gap-1 rounded-lg border border-[#dce3ed] bg-[#f7f9fc] p-2.5">
                <label className="text-[0.68rem] font-bold text-[#56667e] uppercase" htmlFor="folder-new-playlist">Create a private playlist</label>
                <div className="flex gap-2 max-sm:flex-col">
                  <input
                    className="min-w-0 flex-1 rounded-md border border-[#ccd6e4] bg-white px-2.5 py-2 text-[#2d3f5d]"
                    disabled={!activeChannel || busy || creatingPlaylist}
                    id="folder-new-playlist"
                    maxLength={150}
                    onChange={(event) => setNewPlaylistTitle(event.target.value)}
                    placeholder="Playlist name"
                    value={newPlaylistTitle}
                  />
                  <button
                    className="shrink-0 rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] transition-colors hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={!activeChannel || busy || creatingPlaylist || newPlaylistTitle.trim().length === 0}
                    onClick={() => void createPlaylist()}
                    type="button"
                  >
                    {creatingPlaylist ? "Creating…" : "Create playlist"}
                  </button>
                </div>
              </div>
            </div>
          </div>
          <button
            className="rounded-md border border-[#2463df] bg-[#2463df] px-3 py-2 text-[0.79rem] leading-tight font-[680] text-white transition-[background,border-color,box-shadow] hover:border-[#1b54c6] hover:bg-[#1b54c6] hover:shadow-[0_3px_8px_rgba(31,78,181,0.16)] disabled:cursor-not-allowed disabled:opacity-50 max-sm:w-full"
            disabled={!isTauri || !activeChannel || busy}
            onClick={() => void chooseAndEnable()}
            type="button"
          >
            {busy ? "Enabling…" : "Choose folder and enable"}
          </button>
          {!isTauri && (
            <span className="text-[0.72rem] text-[#7b8799]">Open the signed desktop app to monitor a local folder.</span>
          )}
        </div>
      )}

      {loadError && (
        <p className="mt-3 mb-0 text-[0.74rem] text-[#a4413b]" role="alert">
          {loadError}
        </p>
      )}
      <p className="mt-3.5 mb-0 border-t border-[#e7eaf0] pt-3 text-[0.7rem] leading-relaxed text-[#7b8799]">
        The monitor scans direct child files only. Every supported file must
        still stop changing before it is accepted; files over YouTube’s 256 GB
        or 12-hour limits are rejected before upload. A matching local
        BLAKE3 record or title in the last synced YouTube library is not
        uploaded automatically. Watched files are used in place, so keep each
        one unchanged and available until YouTube confirms it. Source cleanup
        re-hashes the original and runs only after that confirmation.
      </p>
    </section>
  );
}
