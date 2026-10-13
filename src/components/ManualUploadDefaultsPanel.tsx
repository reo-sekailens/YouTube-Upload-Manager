import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  isTauri,
  listYouTubePlaylists,
  loadConnectionSettings,
} from "../lib/local";
import { loadManualUploadDefaults, saveManualUploadDefaults } from "../lib/manual-upload-defaults";
import type { ManualUploadDefaults, UploadVisibility, YouTubePlaylist } from "../lib/types";
import { assertWorkspaceChannel, workspaceErrorMessage } from "../lib/workspace-requests";
import "./ManualUploadDefaultsPanel.lazy.css";

type ManualUploadDefaultsPanelProps = {
  activeChannelId?: string;
  activeChannel?: string;
};

type DefaultsPatch = {
  madeForKids?: boolean;
  visibility?: UploadVisibility;
  playlistId?: string;
  expectedChannelId?: string;
};

const selectClass = "min-h-11 w-full min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-60";
const buttonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";

/** Check the immutable connection before and after reading its provider playlists. */
export async function loadDefaultPlaylistsForChannel(
  expectedChannelId: string,
  current: () => boolean,
): Promise<YouTubePlaylist[] | undefined> {
  assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
  if (!current()) return undefined;
  const playlists = await listYouTubePlaylists();
  if (!current()) return undefined;
  assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
  return current() ? playlists : undefined;
}

export function ManualUploadDefaultsPanel({ activeChannelId, activeChannel }: ManualUploadDefaultsPanelProps) {
  const id = useId();
  const [defaults, setDefaults] = useState<ManualUploadDefaults>();
  const [loadingDefaults, setLoadingDefaults] = useState(true);
  const [defaultsError, setDefaultsError] = useState("");
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [loadingPlaylists, setLoadingPlaylists] = useState(Boolean(activeChannelId));
  const [playlistError, setPlaylistError] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [notice, setNotice] = useState("");
  const [reload, setReload] = useState(0);
  const [playlistRefresh, setPlaylistRefresh] = useState(0);
  const mounted = useRef(false);
  const scope = useRef(activeChannelId);
  scope.current = activeChannelId;
  const defaultsRead = useRef(0);
  const playlistRead = useRef(0);
  const loadedPlaylistScope = useRef<string | undefined>(undefined);
  const mutationPending = useRef(false);
  const playlistPending = useRef(false);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      defaultsRead.current += 1;
      playlistRead.current += 1;
    };
  }, []);

  useEffect(() => {
    const generation = ++defaultsRead.current;
    const current = () => mounted.current && scope.current === activeChannelId && defaultsRead.current === generation;
    setDefaults(undefined);
    setLoadingDefaults(true);
    setDefaultsError("");
    setSaveError("");
    setNotice("");
    void loadManualUploadDefaults(activeChannelId).then((saved) => {
      if (current()) setDefaults(saved);
    }).catch((error: unknown) => {
      if (current()) setDefaultsError(workspaceErrorMessage(error, "Upload defaults could not be loaded. Try again before changing them."));
    }).finally(() => {
      if (current()) setLoadingDefaults(false);
    });
    return () => { defaultsRead.current += 1; };
  }, [activeChannelId, reload]);

  useEffect(() => {
    const generation = ++playlistRead.current;
    const current = () => mounted.current && scope.current === activeChannelId && playlistRead.current === generation;
    setPlaylists([]);
    loadedPlaylistScope.current = undefined;
    setPlaylistError("");
    if (!activeChannelId || !isTauri) {
      setLoadingPlaylists(false);
      return;
    }
    playlistPending.current = true;
    setLoadingPlaylists(true);
    void loadDefaultPlaylistsForChannel(activeChannelId, current).then((items) => {
      if (items && current()) {
        loadedPlaylistScope.current = activeChannelId;
        setPlaylists(items);
      }
    }).catch((error: unknown) => {
      if (current()) setPlaylistError(workspaceErrorMessage(error, "Playlists could not be loaded. Refresh to try again."));
    }).finally(() => {
      if (current()) {
        playlistPending.current = false;
        setLoadingPlaylists(false);
      }
    });
    return () => { playlistRead.current += 1; };
  }, [activeChannelId, playlistRefresh]);

  const save = useCallback(async (patch: DefaultsPatch, confirmation: string) => {
    if (!isTauri || mutationPending.current || loadingDefaults || !defaults) return;
    const channelAtStart = activeChannelId;
    const generation = ++defaultsRead.current;
    const current = () => mounted.current && scope.current === channelAtStart && defaultsRead.current === generation;
    mutationPending.current = true;
    setSaving(true);
    setSaveError("");
    setNotice("");
    try {
      const saved = await saveManualUploadDefaults(patch);
      if (current()) {
        setDefaults(saved);
        setNotice(confirmation);
      }
    } catch (error) {
      if (current()) setSaveError(workspaceErrorMessage(error, "The default could not be saved. Your saved choices are unchanged; try again."));
    } finally {
      mutationPending.current = false;
      if (mounted.current) {
        setSaving(false);
        // A completed device-wide write may have overlapped a connection change.
        if (!current()) setReload((value) => value + 1);
      }
    }
  }, [activeChannelId, defaults, loadingDefaults]);

  const visiblePlaylists = loadedPlaylistScope.current === activeChannelId ? playlists : [];
  const selectedPlaylistId = defaults?.playlistChannelId === activeChannelId ? defaults?.playlistId ?? "" : "";
  const selectedPlaylist = visiblePlaylists.find((playlist) => playlist.id === selectedPlaylistId);
  const unavailablePlaylist = Boolean(selectedPlaylistId && !selectedPlaylist && !loadingPlaylists);
  const controlsDisabled = !isTauri || loadingDefaults || !defaults || saving;
  const refreshPlaylists = () => {
    if (!activeChannelId || !isTauri || playlistPending.current || mutationPending.current) return;
    playlistPending.current = true;
    setLoadingPlaylists(true);
    setPlaylistRefresh((value) => value + 1);
  };

  return (
    <section aria-labelledby={`${id}-heading`} className="mb-4 grid gap-4 rounded-lg border border-line bg-subtle-surface p-4">
      <div className="grid gap-1.5">
        <p className="m-0 text-xs font-semibold tracking-wide text-muted uppercase">On this device</p>
        <h3 className="m-0 text-base font-semibold text-ink" id={`${id}-heading`}>Upload defaults</h3>
        <p className="m-0 max-w-2xl text-sm leading-relaxed text-muted">Prefills every manual upload review. You can change these choices for each batch.</p>
      </div>
      <div className="grid min-w-0 items-start gap-4 md:grid-cols-3">
        <div className="grid min-w-0 gap-2">
          <label className="text-sm font-semibold text-ink" htmlFor={`${id}-audience`}>Made for Kids</label>
          <select aria-describedby={`${id}-audience-help`} className={selectClass} disabled={controlsDisabled} id={`${id}-audience`} onChange={(event) => void save({ madeForKids: event.target.value === "yes" }, `Audience default saved: ${event.target.value === "yes" ? "Made for Kids" : "Not made for kids"}.`)} value={defaults?.madeForKids ? "yes" : "no"}>
            <option value="no">No, not made for kids</option>
            <option value="yes">Yes, made for kids</option>
          </select>
          <p className="m-0 text-xs leading-relaxed text-muted" id={`${id}-audience-help`}>For every channel on this device.</p>
        </div>
        <div className="grid min-w-0 gap-2">
          <label className="text-sm font-semibold text-ink" htmlFor={`${id}-visibility`}>Default visibility</label>
          <select aria-describedby={`${id}-visibility-help`} className={selectClass} disabled={controlsDisabled} id={`${id}-visibility`} onChange={(event) => { const visibility = event.target.value as UploadVisibility; void save({ visibility }, `Visibility default saved: ${visibility[0].toUpperCase()}${visibility.slice(1)}.`); }} value={defaults?.visibility ?? "private"}>
            <option value="private">Private</option>
            <option value="unlisted">Unlisted</option>
            <option value="public">Public</option>
          </select>
          <p className="m-0 text-xs leading-relaxed text-muted" id={`${id}-visibility-help`}>For every channel on this device.</p>
        </div>
        <div className="grid min-w-0 gap-2">
          <label className="text-sm font-semibold text-ink" htmlFor={`${id}-playlist`}>Default playlist</label>
          <select aria-describedby={`${id}-playlist-help${unavailablePlaylist ? ` ${id}-playlist-unavailable` : ""}`} className={selectClass} disabled={controlsDisabled || !activeChannelId || loadingPlaylists} id={`${id}-playlist`} onChange={(event) => { const playlistId = event.target.value; const playlist = visiblePlaylists.find((item) => item.id === playlistId); void save({ playlistId, expectedChannelId: activeChannelId }, playlist ? `Playlist default saved: “${playlist.title}”.` : "Playlist default saved: No playlist."); }} value={selectedPlaylistId}>
            <option value="">No playlist</option>
            {selectedPlaylistId && !selectedPlaylist ? <option value={selectedPlaylistId}>{defaults?.playlistTitle || "Saved playlist"} (saved)</option> : null}
            {visiblePlaylists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}
          </select>
          <p className="m-0 text-xs leading-relaxed text-muted" id={`${id}-playlist-help`}>{activeChannelId ? `For ${activeChannel || "the connected channel"} on this device.` : "Connect a YouTube channel to choose its playlist default."}</p>
          {activeChannelId ? <button className={`${buttonClass} justify-self-start`} disabled={!isTauri || loadingPlaylists || saving} onClick={refreshPlaylists} type="button"><RefreshCw aria-hidden="true" size={16} />{loadingPlaylists ? "Loading playlists…" : "Refresh playlists"}</button> : null}
          {playlistError ? <p className="m-0 text-sm leading-relaxed text-danger" role="alert">{playlistError}</p> : unavailablePlaylist ? <p className="m-0 text-xs leading-relaxed text-warning-ink" id={`${id}-playlist-unavailable`}>The saved playlist is not in the current list. Refresh or choose No playlist. Each batch checks it before import.</p> : null}
        </div>
      </div>
      {loadingDefaults || saving || notice ? <p className="m-0 text-sm leading-relaxed text-muted" role="status">{loadingDefaults ? "Loading saved upload defaults…" : saving ? "Saving default…" : notice}</p> : null}
      {defaultsError ? <div className="grid justify-items-start gap-2"><p className="m-0 text-sm leading-relaxed text-danger" role="alert">{defaultsError}</p><button className={buttonClass} disabled={saving} onClick={() => setReload((value) => value + 1)} type="button">Try loading defaults again</button></div> : null}
      {saveError ? <p className="m-0 text-sm leading-relaxed text-danger" role="alert">{saveError}</p> : null}
      {!isTauri ? <p className="m-0 text-xs leading-relaxed text-muted">Upload defaults are saved in the native app on this device.</p> : null}
    </section>
  );
}
