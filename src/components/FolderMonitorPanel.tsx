import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  disableFolderMonitor,
  enableFolderMonitor,
  isTauri,
  listYouTubePlaylists,
  loadFolderMonitorSettings,
  scanFolderMonitorNow,
} from "../lib/local";
import type { FolderMonitorSettings, FolderMonitorVisibility, YouTubePlaylist } from "../lib/types";

const unavailable: FolderMonitorSettings = {
  enabled: false,
  visibility: "private",
  madeForKids: false,
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

export function FolderMonitorPanel({ activeChannel, onNotice, onQueueRefresh }: FolderMonitorPanelProps) {
  const [settings, setSettings] = useState<FolderMonitorSettings>(unavailable);
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [visibility, setVisibility] = useState<FolderMonitorVisibility>("private");
  const [madeForKids, setMadeForKids] = useState(false);
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistId, setPlaylistId] = useState("");
  const lastQueueRefreshAt = useRef<string | undefined>(undefined);

  useEffect(() => {
    let active = true;
    let loading = false;
    const load = async () => {
      if (loading) return;
      loading = true;
      try {
        const loaded = await loadFolderMonitorSettings();
        if (!active) return;
        setSettings(loaded);
        setLoadError("");
        if (loaded.lastScanAt && loaded.lastScanAt !== lastQueueRefreshAt.current) {
          lastQueueRefreshAt.current = loaded.lastScanAt;
          await onQueueRefresh();
        }
      } catch {
        if (active) setLoadError("Folder monitoring status could not be loaded from this device.");
      } finally {
        loading = false;
      }
    };

    void load();
    const timer = isTauri ? window.setInterval(() => void load(), 5000) : undefined;
    return () => {
      active = false;
      if (timer !== undefined) window.clearInterval(timer);
    };
  }, [activeChannel, onQueueRefresh]);

  useEffect(() => {
    if (!isTauri || !activeChannel) { setPlaylists([]); return; }
    void listYouTubePlaylists().then(setPlaylists).catch(() => setPlaylists([]));
  }, [activeChannel]);

  const chooseAndEnable = async () => {
    if (!isTauri || !activeChannel) return;
    setBusy(true);
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected !== "string") return;
      const playlist = playlists.find((candidate) => candidate.id === playlistId);
      const updated = await enableFolderMonitor(selected, visibility, madeForKids, playlist?.id, playlist?.title);
      setSettings(updated);
      setLoadError("");
      onNotice(`Folder monitoring is enabled for ${updated.channelName ?? activeChannel}. New completed videos will be copied locally, queued, and start uploading as ${updated.visibility} once stable.`);
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "Folder monitoring could not be enabled.");
    } finally {
      setBusy(false);
    }
  };

  const scanNow = async () => {
    if (!isTauri || !settings.enabled) return;
    setBusy(true);
    try {
      const updated = await scanFolderMonitorNow();
      setSettings(updated);
      lastQueueRefreshAt.current = updated.lastScanAt;
      await onQueueRefresh();
      onNotice(updated.detail);
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "The watched folder could not be scanned.");
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
      onNotice("Folder monitoring is disabled. Existing queued files and source videos were not changed.");
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "Folder monitoring could not be disabled.");
    } finally {
      setBusy(false);
    }
  };

  const activeStatus = settings.enabled && settings.status === "watching";

  return (
    <section className="panel folder-monitor" aria-labelledby="folder-monitor-heading">
      <header className="section-heading folder-monitor__heading">
        <div>
          <p className="eyebrow">OPT-IN AUTOMATION</p>
          <h2 id="folder-monitor-heading">Watched folder uploads</h2>
          <p className="section-copy">Monitor one local folder while this app is running and send newly added, completed video files to the channel you approve.</p>
        </div>
        <span className={`monitor-status${activeStatus ? " monitor-status--active" : settings.enabled ? " monitor-status--paused" : ""}`}>
          <span aria-hidden="true" />
          {statusLabel(settings)}
        </span>
      </header>

      <div className="folder-monitor__consent">
        <strong>New videos upload automatically as {settings.enabled ? settings.visibility : visibility}.</strong>
        <span>Enabling this is recurring approval to copy supported files added after enabling into the app’s managed workspace and upload them to the bound YouTube channel. Existing files are used as the starting baseline. Nothing is deleted or published publicly.</span>
      </div>

      {settings.enabled ? (
        <div className="folder-monitor__enabled">
          <dl className="folder-monitor__facts">
            <div><dt>Folder</dt><dd title={settings.folderPath}>{settings.folderPath ?? "Unavailable"}</dd></div>
            <div><dt>Bound channel</dt><dd>{settings.channelName ?? "Unavailable"}</dd></div>
            <div><dt>Visibility</dt><dd>{settings.visibility}</dd></div>
            <div><dt>Audience</dt><dd>{settings.madeForKids ? "Made for kids" : "Not made for kids"}</dd></div>
            <div><dt>Playlist</dt><dd>{settings.playlistTitle ?? "No playlist"}</dd></div>
            <div><dt>Last scan</dt><dd>{formatScanTime(settings.lastScanAt)}</dd></div>
            <div><dt>Last file</dt><dd>{settings.lastFileName ?? "No file processed yet"}</dd></div>
          </dl>
          <p className="folder-monitor__detail" role="status">{settings.detail}</p>
          <div className="folder-monitor__actions">
            <button disabled={busy} onClick={() => void scanNow()} type="button">{busy ? "Working…" : "Scan now"}</button>
            <button className="danger-button" disabled={busy} onClick={() => void disable()} type="button">Disable monitor</button>
          </div>
        </div>
      ) : (
        <div className="folder-monitor__disabled">
          <div>
            <p>{activeChannel ? `Ready to bind a folder to ${activeChannel}.` : "Connect a YouTube channel before choosing a folder."}</p>
            <div className="folder-monitor__options"><label className="folder-monitor__visibility"><span>Automatic upload visibility</span><select disabled={!activeChannel || busy} onChange={(event) => setVisibility(event.target.value as FolderMonitorVisibility)} value={visibility}><option value="private">Private</option><option value="unlisted">Unlisted</option></select></label><label className="folder-monitor__visibility"><span>Audience</span><select disabled={!activeChannel || busy} onChange={(event) => setMadeForKids(event.target.value === "yes")} value={madeForKids ? "yes" : "no"}><option value="no">Not made for kids</option><option value="yes">Made for kids</option></select></label><label className="folder-monitor__visibility"><span>Add to playlist</span><select disabled={!activeChannel || busy} onChange={(event) => setPlaylistId(event.target.value)} value={playlistId}><option value="">No playlist</option>{playlists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}</select></label></div>
          </div>
          <button disabled={!isTauri || !activeChannel || busy} onClick={() => void chooseAndEnable()} type="button">{busy ? "Enabling…" : "Choose folder and enable"}</button>
          {!isTauri && <span>Open the signed desktop app to monitor a local folder.</span>}
        </div>
      )}

      {loadError && <p className="folder-monitor__error" role="alert">{loadError}</p>}
      <p className="folder-monitor__footnote">The monitor scans direct child files only. A file must stop changing before it is accepted; a matching local SHA-256 record or title in the last synced YouTube library is not uploaded automatically.</p>
    </section>
  );
}
