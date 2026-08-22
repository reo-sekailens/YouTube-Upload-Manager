import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  createYouTubePlaylist,
  disableFolderMonitor,
  enableFolderMonitor,
  isTauri,
  listYouTubePlaylists,
  loadFolderMonitorOverview,
  processExistingFolderFiles,
  scanFolderMonitorNow,
} from "../lib/local";
import type {
  FolderMonitorSettings,
  FolderMonitorFileActivity,
  FolderMonitorLogEntry,
  FolderMonitorVisibility,
  YouTubePlaylist,
} from "../lib/types";

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
  onNotice,
  onQueueRefresh,
}: FolderMonitorPanelProps) {
  const [settings, setSettings] = useState<FolderMonitorSettings>(unavailable);
  const [files, setFiles] = useState<FolderMonitorFileActivity[]>([]);
  const [logs, setLogs] = useState<FolderMonitorLogEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [visibility, setVisibility] =
    useState<FolderMonitorVisibility>("private");
  const [madeForKids, setMadeForKids] = useState(false);
  const [deleteSourceAfterUpload, setDeleteSourceAfterUpload] = useState(false);
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistId, setPlaylistId] = useState("");
  const [newPlaylistTitle, setNewPlaylistTitle] = useState("");
  const [creatingPlaylist, setCreatingPlaylist] = useState(false);
  const lastQueueRefreshAt = useRef<string | undefined>(undefined);

  useEffect(() => {
    let active = true;
    let loading = false;
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
        if (
          loaded.settings.lastScanAt &&
          loaded.settings.lastScanAt !== lastQueueRefreshAt.current
        ) {
          lastQueueRefreshAt.current = loaded.settings.lastScanAt;
          void onQueueRefresh().catch(() => {
            if (active)
              setLoadError(
                "The upload queue could not be refreshed after the folder scan.",
              );
          });
        }
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
    const timer = isTauri
      ? window.setInterval(() => void load(), 5000)
      : undefined;
    return () => {
      active = false;
      if (timer !== undefined) window.clearInterval(timer);
    };
  }, [activeChannel, onQueueRefresh]);

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
      lastQueueRefreshAt.current = updated.lastScanAt;
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

  const processExisting = async () => {
    if (!isTauri || !settings.enabled) return;
    setBusy(true);
    try {
      const updated = await processExistingFolderFiles();
      setSettings(updated);
      lastQueueRefreshAt.current = updated.lastScanAt;
      void onQueueRefresh().catch(() =>
        setLoadError("The upload queue could not be refreshed after processing existing files."),
      );
      onNotice(updated.detail);
    } catch (error) {
      onNotice(
        error instanceof Error
          ? error.message
          : "Existing watched-folder files could not be processed.",
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
    () => files.filter((file) => ["draft", "queued", "needs_reconciliation"].includes(file.uploadStatus ?? file.observationState)),
    [files],
  );
  const recentFiles = useMemo(() => files.slice(0, 24), [files]);

  return (
    <section
      className="panel folder-monitor"
      aria-labelledby="folder-monitor-heading"
    >
      <header className="section-heading folder-monitor__heading">
        <div>
          <p className="eyebrow">OPT-IN AUTOMATION</p>
          <h2 id="folder-monitor-heading">Watched folder uploads</h2>
          <p className="section-copy">
            Monitor one local folder while this app is running and send newly
            added, completed video files to the channel you approve.
          </p>
        </div>
        <span
          className={`monitor-status${activeStatus ? " monitor-status--active" : settings.enabled ? " monitor-status--paused" : ""}`}
        >
          <span aria-hidden="true" />
          {statusLabel(settings)}
        </span>
      </header>

      <div className="folder-monitor__consent">
        <strong>
          New videos upload automatically as{" "}
          {settings.enabled ? settings.visibility : visibility}.
        </strong>
        <span>
          Enabling this is recurring approval to copy supported files added
          after enabling into the app’s managed workspace and upload them to the
          bound YouTube channel. Existing files stay in the starting baseline
          until you explicitly choose to process them. Original files remain
          unless source cleanup is explicitly enabled below.
        </span>
      </div>

      {settings.enabled ? (
        <div className="folder-monitor__enabled">
          <dl className="folder-monitor__facts">
            <div>
              <dt>Folder</dt>
              <dd title={settings.folderPath}>
                {settings.folderPath ?? "Unavailable"}
              </dd>
            </div>
            <div>
              <dt>Bound channel</dt>
              <dd>{settings.channelName ?? "Unavailable"}</dd>
            </div>
            <div>
              <dt>Visibility</dt>
              <dd>{settings.visibility}</dd>
            </div>
            <div>
              <dt>Audience</dt>
              <dd>
                {settings.madeForKids ? "Made for kids" : "Not made for kids"}
              </dd>
            </div>
            <div>
              <dt>Original source cleanup</dt>
              <dd>
                {settings.deleteSourceAfterUpload
                  ? "Delete after confirmed upload"
                  : "Keep original"}
              </dd>
            </div>
            <div>
              <dt>Playlist</dt>
              <dd>{settings.playlistTitle ?? "No playlist"}</dd>
            </div>
            <div>
              <dt>Last scan</dt>
              <dd>{formatScanTime(settings.lastScanAt)}</dd>
            </div>
            <div>
              <dt>Last file</dt>
              <dd>{settings.lastFileName ?? "No file processed yet"}</dd>
            </div>
          </dl>
          <p className="folder-monitor__detail" role="status">
            {settings.detail}
          </p>
          <p className="folder-monitor__automatic">
            Automatic scanning is active every 5 seconds while this app is running.
          </p>
          <section className="folder-monitor__live" aria-labelledby="folder-monitor-live-heading">
            <div className="folder-monitor__live-heading">
              <div>
                <p className="eyebrow">THIS FOLDER</p>
                <h3 id="folder-monitor-live-heading">Live folder activity</h3>
              </div>
              <span>{uploadingFiles.length} uploading · {queuedFiles.length} queued</span>
            </div>
            <div className="folder-monitor__activity-grid">
              <div className="folder-monitor__activity-column">
                <h4>Uploading now</h4>
                {uploadingFiles.length === 0 ? (
                  <p className="folder-monitor__activity-empty">No files are currently being copied or uploaded.</p>
                ) : (
                  <ul className="folder-monitor__activity-list">
                    {uploadingFiles.map((file) => {
                      const progress = fileProgress(file);
                      return (
                        <li className="folder-monitor__activity-item" key={`${file.fileName}-${file.updatedAt}`}>
                          <div className="folder-monitor__activity-row">
                            <strong title={file.fileName}>{file.fileName}</strong>
                            <span>{file.uploadStatus ?? file.observationState}</span>
                          </div>
                          <p>{formatBytes(file.sizeBytes)}{progress === undefined ? "" : ` · ${progress}% uploaded`}</p>
                          {progress !== undefined && <progress aria-label={`${file.fileName} upload progress`} max={100} value={progress} />}
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
              <div className="folder-monitor__activity-column">
                <h4>Waiting in this folder’s queue</h4>
                {queuedFiles.length === 0 ? (
                  <p className="folder-monitor__activity-empty">No watched-folder files are awaiting upload.</p>
                ) : (
                  <ul className="folder-monitor__activity-list">
                    {queuedFiles.map((file) => (
                      <li className="folder-monitor__activity-item" key={`${file.fileName}-${file.updatedAt}`}>
                        <div className="folder-monitor__activity-row">
                          <strong title={file.fileName}>{file.fileName}</strong>
                          <span>{file.uploadStatus ?? file.observationState}</span>
                        </div>
                        <p>{formatBytes(file.sizeBytes)} · updated {formatScanTime(file.updatedAt)}</p>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
            <details className="folder-monitor__activity-log">
              <summary>Folder scan log ({logs.length})</summary>
              {logs.length === 0 ? (
                <p className="folder-monitor__activity-empty">No scan events have been recorded for this folder yet.</p>
              ) : (
                <ul className="folder-monitor__log-list">
                  {logs.map((log) => (
                    <li key={`${log.createdAt}-${log.kind}`}>
                      <strong>{log.kind.replaceAll("_", " ")}</strong>
                      <span>{formatScanTime(log.createdAt)}</span>
                      <p>{log.detail ?? "No further detail recorded."}</p>
                    </li>
                  ))}
                </ul>
              )}
            </details>
            <details className="folder-monitor__activity-log">
              <summary>Recently observed files ({files.length})</summary>
              <ul className="folder-monitor__log-list">
                {recentFiles.map((file) => (
                  <li key={`${file.fileName}-${file.updatedAt}`}>
                    <strong>{file.fileName}</strong>
                    <span>{formatScanTime(file.updatedAt)}</span>
                    <p>{file.uploadTitle ? `Queued as ${file.uploadTitle}` : `${formatBytes(file.sizeBytes)} · ${file.observationState}`}</p>
                  </li>
                ))}
              </ul>
            </details>
          </section>
          <div className="folder-monitor__actions">
            <button
              disabled={busy}
              onClick={() => void scanNow()}
              type="button"
            >
              {busy ? "Working…" : "Refresh scan"}
            </button>
            <button
              className="secondary-action"
              disabled={busy}
              onClick={() => void processExisting()}
              type="button"
            >
              {busy ? "Working…" : "Process existing files"}
            </button>
            <button
              className="danger-button"
              disabled={busy}
              onClick={() => void disable()}
              type="button"
            >
              Disable monitor
            </button>
          </div>
        </div>
      ) : (
        <div className="folder-monitor__disabled">
          <div>
            <p>
              {activeChannel
                ? `Ready to bind a folder to ${activeChannel}.`
                : "Connect a YouTube channel before choosing a folder."}
            </p>
            <div className="folder-monitor__options">
              <label className="folder-monitor__visibility">
                <span>Automatic upload visibility</span>
                <select
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
              <label className="folder-monitor__source-cleanup">
                <input
                  checked={deleteSourceAfterUpload}
                  disabled={!activeChannel || busy}
                  onChange={(event) =>
                    setDeleteSourceAfterUpload(event.target.checked)
                  }
                  type="checkbox"
                />{" "}
                Delete original only after YouTube confirms each upload
              </label>
              <label className="folder-monitor__visibility">
                <span>Audience</span>
                <select
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
              <label className="folder-monitor__visibility">
                <span>Add to playlist</span>
                <select
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
              <div className="playlist-create folder-monitor__playlist-create">
                <label htmlFor="folder-new-playlist">Create a private playlist</label>
                <div>
                  <input
                    disabled={!activeChannel || busy || creatingPlaylist}
                    id="folder-new-playlist"
                    maxLength={150}
                    onChange={(event) => setNewPlaylistTitle(event.target.value)}
                    placeholder="Playlist name"
                    value={newPlaylistTitle}
                  />
                  <button
                    className="secondary-action"
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
            disabled={!isTauri || !activeChannel || busy}
            onClick={() => void chooseAndEnable()}
            type="button"
          >
            {busy ? "Enabling…" : "Choose folder and enable"}
          </button>
          {!isTauri && (
            <span>Open the signed desktop app to monitor a local folder.</span>
          )}
        </div>
      )}

      {loadError && (
        <p className="folder-monitor__error" role="alert">
          {loadError}
        </p>
      )}
      <p className="folder-monitor__footnote">
        The monitor scans direct child files only. Existing files remain a safe
        baseline until you select Process existing files. Every selected file
        must still stop changing before it is accepted; a matching local SHA-256
        record or title in the last synced YouTube library is not uploaded
        automatically. Source cleanup re-hashes the original after a confirmed
        upload and never deletes the managed app copy.
      </p>
    </section>
  );
}
