import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./PlaylistManager.lazy.css";
import {
  addRemoteVideosToPlaylist,
  beginDeletionAuthorization,
  createYouTubePlaylist,
  enableDeletionSudoMode,
  isTauri,
  listRemoteVideos,
  listYouTubePlaylists,
  loadConnectionSettings,
  sortPlaylistItemsByTitle,
} from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import { windowItems } from "../lib/list-windowing";
import { isPlaylistSortProgressForScope, type PlaylistSortProgress } from "../lib/playlist-sort-progress";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import type { PlaylistPrivacy, RemoteVideo, YouTubePlaylist } from "../lib/types";

type Props = { activeChannel?: string; activeChannelId?: string; onNotice: (message: string) => void; refreshVersion?: number };
type WorkspaceTab = "add" | "manage";
type Direction = "ascending" | "descending";

const buttonClass = "cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2 text-sm font-bold text-[#344a67] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";
const primaryButtonClass = "cursor-pointer rounded-md border border-[#1f5ea8] bg-[#1f5ea8] px-3 py-2 text-sm font-bold text-white hover:bg-[#174d8c] focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";

export default function PlaylistManager({ activeChannel, activeChannelId, onNotice, refreshVersion = 0 }: Props) {
  const [videos, setVideos] = useState<RemoteVideo[]>([]);
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [tab, setTab] = useState<WorkspaceTab>("add");
  const [addPlaylistId, setAddPlaylistId] = useState("");
  const [managePlaylistId, setManagePlaylistId] = useState("");
  const [newPlaylistTitle, setNewPlaylistTitle] = useState("");
  const [privacy, setPrivacy] = useState<PlaylistPrivacy>("private");
  const [videoDirection, setVideoDirection] = useState<Direction>("ascending");
  const [playlistDirection, setPlaylistDirection] = useState<Direction>("ascending");
  const [selectedVideoIds, setSelectedVideoIds] = useState<Set<string>>(() => new Set());
  const [authorized, setAuthorized] = useState(false);
  const [managementMode, setManagementMode] = useState(false);
  const [busy, setBusy] = useState(false);
  const [sortProgress, setSortProgress] = useState<PlaylistSortProgress | null>(null);
  const [sortLog, setSortLog] = useState<PlaylistSortProgress[]>([]);
  const [loadedChannelId, setLoadedChannelId] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [page, setPage] = useState(1);
  const loadFence = useWorkspaceRequestFence(activeChannelId);
  const operationFence = useWorkspaceRequestFence(activeChannelId);
  const operationRunning = useRef(false);
  const sorting = useRef<{ current: () => boolean; channelId: string; playlistId: string } | null>(null);
  const currentLibrary = Boolean(activeChannelId && loadedChannelId === activeChannelId);

  const refresh = useCallback(async () => {
    const current = loadFence.begin();
    if (!isTauri || !activeChannelId) return;
    setLoading(true); setLoadError("");
    try {
      const before = await loadConnectionSettings();
      assertWorkspaceChannel(before, activeChannelId);
      if (!current()) return;
      const [remoteVideos, remotePlaylists] = await Promise.all([listRemoteVideos(), listYouTubePlaylists()]);
      const settings = await loadConnectionSettings();
      assertWorkspaceChannel(settings, activeChannelId);
      if (!current()) return;
      setVideos(remoteVideos); setPlaylists(remotePlaylists); setLoadedChannelId(activeChannelId);
      const remoteIds = new Set(remoteVideos.map((video) => video.videoId));
      setSelectedVideoIds((selected) => new Set([...selected].filter((id) => remoteIds.has(id))));
      setAuthorized(settings.deletionAuthorized === true); setManagementMode(settings.deletionSudoActive === true);
    } catch (error) {
      if (current()) setLoadError(workspaceErrorMessage(error, "The YouTube library could not be loaded."));
    } finally { if (current()) setLoading(false); }
  }, [activeChannelId, loadFence]);

  useEffect(() => {
    setVideos([]); setPlaylists([]); setLoadedChannelId(undefined);
    setSelectedVideoIds(new Set()); setAddPlaylistId(""); setManagePlaylistId("");
    setAuthorized(false); setManagementMode(false); setSortProgress(null); setSortLog([]); setPage(1);
    sorting.current = null;
  }, [activeChannelId]);
  useEffect(() => { void refresh(); }, [refresh, refreshVersion]);

  useEffect(() => {
    if (!isTauri) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<PlaylistSortProgress>("playlist-sort-progress", (event) => {
      const job = sorting.current;
      if (!active || !job?.current() || !isPlaylistSortProgressForScope(event.payload, job.channelId, job.playlistId)) return;
      setSortProgress(event.payload);
      setSortLog((previous) => [...previous.slice(-31), event.payload]);
    }).then((stop) => { if (active) unlisten = stop; else stop(); }).catch(() => undefined);
    return () => { active = false; unlisten?.(); };
  }, []);

  const orderedVideos = useMemo(() => [...(currentLibrary ? videos : [])].sort((left, right) => {
    const result = left.title.localeCompare(right.title, undefined, { numeric: true, sensitivity: "base" });
    return videoDirection === "ascending" ? result : -result;
  }), [currentLibrary, videoDirection, videos]);
  const videoWindow = windowItems(orderedVideos, page);
  const visiblePlaylists = currentLibrary ? playlists : [];
  const sortPercent = sortProgress?.total ? Math.round((sortProgress.completed / sortProgress.total) * 100) : 0;
  const handleTabKey = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    const next = event.key === "Home" ? "add" : event.key === "End" ? "manage" : event.key === "ArrowLeft" || event.key === "ArrowRight" ? tab === "add" ? "manage" : "add" : undefined;
    if (!next) return;
    event.preventDefault(); setTab(next);
    event.currentTarget.parentElement?.querySelector<HTMLButtonElement>(`#playlist-tab-${next}`)?.focus();
  };

  const run = async (action: (current: () => boolean) => Promise<void>) => {
    if (operationRunning.current || !currentLibrary) return;
    operationRunning.current = true;
    const current = operationFence.begin();
    setBusy(true);
    try { assertWorkspaceChannel(await loadConnectionSettings(), activeChannelId); if (current()) await action(current); }
    catch (error) { if (current()) onNotice(workspaceErrorMessage(error, "Playlist operation failed.")); }
    finally { operationRunning.current = false; setBusy(false); }
  };
  const grantManagement = () => run(async (current) => { const { authorizationUrl } = await beginDeletionAuthorization(); if (!current()) return; await openAndCopyGoogleAuthorization(authorizationUrl); if (current()) onNotice("Grant YouTube video-management permission, then return here."); });
  const enterManagementMode = () => run(async (current) => { const settings = await enableDeletionSudoMode(); if (current()) setManagementMode(settings.deletionSudoActive === true); });
  const openStudioSettings = () => run(async (current) => { if (!managePlaylistId) return; const url = `https://studio.youtube.com/playlist/${encodeURIComponent(managePlaylistId)}/edit`; if (isTauri) { const { openUrl } = await import("@tauri-apps/plugin-opener"); if (current()) await openUrl(url); } else { window.open(url, "_blank", "noopener,noreferrer"); } });
  const updateSelection = (videoId: string) => setSelectedVideoIds((current) => { const next = new Set(current); if (next.has(videoId)) next.delete(videoId); else next.add(videoId); return next; });
  const applyCustomOrder = () => run(async (current) => {
    const job = { current, channelId: activeChannelId ?? "", playlistId: managePlaylistId };
    sorting.current = job;
    const initial: PlaylistSortProgress = { channelId: job.channelId, playlistId: job.playlistId, completed: 0, total: 0, currentTitle: "", status: "running", detail: "Loading playlist videos from YouTube…" };
    setSortProgress(initial);
    setSortLog([initial]);
    try {
      const count = await sortPlaylistItemsByTitle(managePlaylistId, playlistDirection);
      if (current()) {
        const completed: PlaylistSortProgress = { channelId: job.channelId, playlistId: job.playlistId, completed: count, total: count, currentTitle: "", status: "completed", detail: `YouTube confirmed the requested custom order for ${count} videos.` };
        setSortProgress(completed);
        setSortLog((log) => [...log.slice(-31), completed]);
        onNotice(`Verified ${playlistDirection === "ascending" ? "A–Z" : "Z–A"} custom order for ${count} videos.`);
      }
    } catch (error) {
      if (current()) setSortProgress((progress) => ({ channelId: job.channelId, playlistId: job.playlistId, completed: progress?.completed ?? 0, total: progress?.total ?? 0, currentTitle: "", status: "error", detail: workspaceErrorMessage(error, "Playlist order could not be verified.") }));
      throw error;
    } finally { if (sorting.current === job) sorting.current = null; }
  });

  if (!activeChannel) return <section className="mt-4 rounded-xl border border-line bg-white p-5"><h2>Playlists</h2><p className="ui-subtle">Connect and sync a YouTube channel first.</p></section>;
  const managementGate = !authorized ? <button className={primaryButtonClass} disabled={busy || loading || !currentLibrary} onClick={() => void grantManagement()} type="button">Grant playlist-management permission</button> : !managementMode ? <button className={primaryButtonClass} disabled={busy || loading || !currentLibrary} onClick={() => void enterManagementMode()} type="button">Enter playlist-management mode</button> : null;

  return <section className="mt-4 grid gap-4 rounded-xl border border-line bg-white p-5">
    <header><p className="ui-eyebrow">ACTIVE CHANNEL LIBRARY</p><h2>Playlists</h2><p className="ui-subtle">Create and fill playlists separately from managing an existing playlist's order.</p></header>
    <button className={`${buttonClass} justify-self-start`} disabled={loading || busy} onClick={() => void refresh()} type="button">Refresh library &amp; permission</button>
    {loading && <p className="ui-subtle" role="status">Loading this channel’s library and playlists…</p>}
    {loadError && <div className="grid gap-2" role="alert"><p className="text-sm font-semibold text-danger">{loadError}</p><button className={buttonClass} disabled={loading || busy} onClick={() => void refresh()} type="button">Retry library load</button></div>}
    <div aria-label="Playlist workspace" className="flex flex-wrap gap-2 border-b border-line pb-3" role="tablist">
      <button aria-controls="playlist-panel-add" aria-selected={tab === "add"} className={tab === "add" ? primaryButtonClass : buttonClass} id="playlist-tab-add" onClick={() => setTab("add")} onKeyDown={handleTabKey} role="tab" tabIndex={tab === "add" ? 0 : -1} type="button">Create &amp; add videos</button>
      <button aria-controls="playlist-panel-manage" aria-selected={tab === "manage"} className={tab === "manage" ? primaryButtonClass : buttonClass} id="playlist-tab-manage" onClick={() => setTab("manage")} onKeyDown={handleTabKey} role="tab" tabIndex={tab === "manage" ? 0 : -1} type="button">Manage &amp; sort playlist</button>
    </div>
    {tab === "add" ? <div aria-labelledby="playlist-tab-add" className="grid gap-4" id="playlist-panel-add" role="tabpanel">
      <section className="grid gap-3 rounded-lg border border-line p-4"><div><h3 className="text-base font-bold text-[#182b49]">1. Choose where to add videos</h3><p className="ui-subtle">Select an existing playlist, or create one first.</p></div>
        <label className="grid gap-1 text-sm font-semibold text-[#344a67]">Existing playlist<select onChange={(event) => setAddPlaylistId(event.target.value)} value={addPlaylistId}><option value="">Choose a playlist</option>{visiblePlaylists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}</select></label>
        <div className="grid gap-2 border-t border-line pt-3 md:grid-cols-[1fr_auto_auto]"><label className="grid gap-1 text-sm font-semibold text-[#344a67]">New playlist name<input onChange={(event) => setNewPlaylistTitle(event.target.value)} placeholder="Playlist name" value={newPlaylistTitle} /></label><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Visibility<select onChange={(event) => setPrivacy(event.target.value as PlaylistPrivacy)} value={privacy}><option value="private">Private</option><option value="unlisted">Unlisted</option><option value="public">Public</option></select></label><button className={buttonClass} disabled={!newPlaylistTitle.trim() || busy || loading || !currentLibrary} onClick={() => void run(async (current) => { const playlist = await createYouTubePlaylist(newPlaylistTitle, privacy); if (!current()) return; setPlaylists((current) => [...current, playlist]); setAddPlaylistId(playlist.id); setNewPlaylistTitle(""); onNotice(`Created ${privacy} playlist “${playlist.title}”.`); })} type="button">Create playlist</button></div>
      </section>
      <section className="grid gap-3 rounded-lg border border-line p-4"><div className="flex flex-wrap items-end justify-between gap-3"><div><h3 className="text-base font-bold text-[#182b49]">2. Select videos to add</h3><p className="ui-subtle">{selectedVideoIds.size} selected from your current channel library.</p></div><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Display order<select onChange={(event) => { setVideoDirection(event.target.value as Direction); setPage(1); }} value={videoDirection}><option value="ascending">Title A–Z</option><option value="descending">Title Z–A</option></select></label></div><div className="flex flex-wrap gap-2"><button className={buttonClass} disabled={busy || loading || !currentLibrary || videos.length === 0} onClick={() => setSelectedVideoIds(new Set(videos.map((video) => video.videoId)))} type="button">Select all</button><button className={buttonClass} disabled={busy || selectedVideoIds.size === 0} onClick={() => setSelectedVideoIds(new Set())} type="button">Clear selection</button></div><div aria-label="Available videos" className="grid max-h-80 gap-2 overflow-auto">{videoWindow.items.map((video) => <label className="flex cursor-pointer gap-2 rounded-md border border-line p-3 text-sm text-[#182b49]" key={video.videoId}><input checked={selectedVideoIds.has(video.videoId)} disabled={busy || loading} onChange={() => updateSelection(video.videoId)} type="checkbox" /><span>{video.title}</span></label>)}</div>{currentLibrary && !loading && videos.length === 0 && <p className="ui-subtle">No saved videos. Sync the YouTube library before selecting videos to add.</p>}{videoWindow.pageCount > 1 && <nav aria-label="Available video pages" className="flex flex-wrap items-center justify-between gap-2"><span className="ui-subtle">{videoWindow.start + 1}–{videoWindow.end} of {videoWindow.total} videos</span><div className="flex gap-2"><button className={buttonClass} disabled={videoWindow.page === 1} onClick={() => setPage(videoWindow.page - 1)} type="button">Previous</button><button className={buttonClass} disabled={videoWindow.page === videoWindow.pageCount} onClick={() => setPage(videoWindow.page + 1)} type="button">Next</button></div></nav>}<footer className="flex flex-wrap items-center gap-2 border-t border-line pt-3">{managementGate ?? <button className={primaryButtonClass} disabled={!addPlaylistId || !selectedVideoIds.size || busy || loading || !currentLibrary} onClick={() => void run(async (current) => { const count = await addRemoteVideosToPlaylist(addPlaylistId, [...selectedVideoIds]); if (!current()) return; setSelectedVideoIds(new Set()); onNotice(`Added ${count} videos to the selected playlist.`); })} type="button">Add selected videos</button>}</footer></section>
    </div> : <div aria-labelledby="playlist-tab-manage" className="grid gap-4" id="playlist-panel-manage" role="tabpanel">
      <section className="grid gap-3 rounded-lg border border-line p-4"><div><h3 className="text-base font-bold text-[#182b49]">Select a playlist to manage</h3><p className="ui-subtle">This tab changes the selected playlist only; it does not add videos.</p></div><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Playlist<select onChange={(event) => setManagePlaylistId(event.target.value)} value={managePlaylistId}><option value="">Choose a playlist</option>{visiblePlaylists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}</select></label><button className={buttonClass} disabled={!managePlaylistId || busy || loading || !currentLibrary} onClick={() => void openStudioSettings()} type="button">Open playlist settings in YouTube Studio</button></section>
      <section className="grid gap-3 rounded-lg border border-line p-4"><div><h3 className="text-base font-bold text-[#182b49]">Custom title order</h3><p className="ui-subtle">In YouTube Studio, set the playlist order to Manual first. The app verifies the final order after every request.</p></div><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Order playlist videos by title<select onChange={(event) => setPlaylistDirection(event.target.value as Direction)} value={playlistDirection}><option value="ascending">Ascending — A–Z</option><option value="descending">Descending — Z–A</option></select></label>
        {sortProgress && <section aria-live="polite" className={`grid gap-2 rounded-lg border p-3 ${sortProgress.status === "error" ? "border-[#f0d0cc] bg-[#fff7f6]" : "border-[#d8e5f8] bg-[#f6f9ff]"}`}><div className="flex flex-wrap items-baseline justify-between gap-2"><strong className="text-sm text-[#30496c]">Sort progress {sortProgress.total ? `${sortProgress.completed} of ${sortProgress.total}` : ""}</strong><span className="text-xs text-[#60728c]">{sortProgress.currentTitle ? `Working on “${sortProgress.currentTitle}”` : sortProgress.detail}</span></div><div aria-label="Playlist custom order progress" aria-valuemax={sortProgress.total || 1} aria-valuemin={0} aria-valuenow={sortProgress.completed} className="h-2 overflow-hidden rounded-full bg-[#d7e2f2]" role="progressbar"><span className={`block h-full rounded-[inherit] transition-[width] duration-200 ${sortProgress.status === "error" ? "bg-[#c95146]" : sortProgress.status === "completed" ? "bg-[#39866a]" : "bg-[#2463df]"}`} style={{ width: `${sortPercent}%` }} /></div><details className="text-xs text-[#445d80]"><summary className="cursor-pointer font-bold">Recent sort activity ({sortLog.length}, up to 32)</summary><ol className="mt-2 grid list-none gap-1 p-0">{sortLog.map((entry, index) => <li className="rounded border border-line bg-white px-2 py-1.5" key={`${index}-${entry.completed}-${entry.detail}`}><strong>{entry.status === "error" ? "Needs attention" : entry.status === "completed" ? "Verified" : entry.completed ? `Video ${entry.completed} of ${entry.total}` : "Preparing"}</strong><span className="ml-2">{entry.detail}</span></li>)}</ol></details></section>}
        <footer className="flex flex-wrap gap-2 border-t border-line pt-3">{managementGate ?? <><button className={buttonClass} disabled={busy || loading || !currentLibrary} onClick={() => void grantManagement()} type="button">Reauthorize management with Google</button><button className={primaryButtonClass} disabled={!managePlaylistId || busy || loading || !currentLibrary} onClick={() => void applyCustomOrder()} type="button">{busy && sortProgress?.status === "running" ? "Applying custom order…" : "Apply custom order"}</button></>}</footer>
      </section>
    </div>}
  </section>;
}
