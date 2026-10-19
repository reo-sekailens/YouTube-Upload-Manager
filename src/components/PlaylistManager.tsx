import { listen } from "@tauri-apps/api/event";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./PlaylistManager.lazy.css";
import {
  addRemoteVideosToPlaylist,
  beginDeletionAuthorization,
  enableDeletionSudoMode,
  isTauri,
  listRemoteVideos,
  listYouTubePlaylists,
  loadConnectionSettings,
  sortPlaylistItemsByTitle,
} from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import { windowItems } from "../lib/list-windowing";
import { addPlaylistSelection, isPlaylistAddReceiptForSelection, PLAYLIST_SELECTION_LIMIT, playlistRangeIds } from "../lib/playlist-selection";
import { isPlaylistAddProgressForScope, type PlaylistAddProgress } from "../lib/playlist-add-progress";
import { isPlaylistSortProgressForScope, type PlaylistSortProgress } from "../lib/playlist-sort-progress";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import type { PlaylistAddReceipt, RemoteVideo, YouTubePlaylist } from "../lib/types";

const PlaylistCreateDialog = lazy(() => import("./PlaylistCreateDialog").then((module) => ({ default: module.PlaylistCreateDialog })));

type Props = { activeChannel?: string; activeChannelId?: string; onNotice: (message: string) => void; refreshVersion?: number };
type WorkspaceTab = "add" | "manage";
type Direction = "ascending" | "descending";

const buttonClass = "min-h-11 cursor-pointer rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";
const primaryButtonClass = "min-h-11 cursor-pointer rounded-md border border-brand bg-brand px-3 py-2 text-sm font-semibold text-white hover:bg-brand-strong focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";
const controlClass = "min-h-11 w-full min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";

export default function PlaylistManager({ activeChannel, activeChannelId, onNotice, refreshVersion = 0 }: Props) {
  const [videos, setVideos] = useState<RemoteVideo[]>([]);
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [tab, setTab] = useState<WorkspaceTab>("add");
  const [addPlaylistId, setAddPlaylistId] = useState("");
  const [managePlaylistId, setManagePlaylistId] = useState("");
  const [createChannelId, setCreateChannelId] = useState<string>();
  const [videoDirection, setVideoDirection] = useState<Direction>("ascending");
  const [playlistDirection, setPlaylistDirection] = useState<Direction>("ascending");
  const [selectedVideoIds, setSelectedVideoIds] = useState<Set<string>>(() => new Set());
  const [search, setSearch] = useState("");
  const [visibility, setVisibility] = useState("all");
  const [selectedOnly, setSelectedOnly] = useState(false);
  const [selectionDetail, setSelectionDetail] = useState("");
  const [addProgress, setAddProgress] = useState<PlaylistAddProgress | null>(null);
  const [addReceipt, setAddReceipt] = useState<PlaylistAddReceipt | null>(null);
  const [addError, setAddError] = useState("");
  const [receiptPlaylistTitle, setReceiptPlaylistTitle] = useState("");
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
  const permissionFence = useWorkspaceRequestFence(activeChannelId);
  const operationFence = useWorkspaceRequestFence(activeChannelId);
  const operationRunning = useRef(false);
  const sorting = useRef<{ current: () => boolean; channelId: string; playlistId: string } | null>(null);
  const adding = useRef<{ current: () => boolean; channelId: string; playlistId: string; requestId: string } | null>(null);
  const selectionAnchor = useRef<string | undefined>(undefined);
  const rangeGesture = useRef(false);
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
    setSearch(""); setVisibility("all"); setSelectedOnly(false); setSelectionDetail("");
    setAddProgress(null); setAddReceipt(null); setAddError(""); setReceiptPlaylistTitle(""); setCreateChannelId(undefined);
    setAuthorized(false); setManagementMode(false); setSortProgress(null); setSortLog([]); setPage(1);
    sorting.current = null;
    adding.current = null;
    selectionAnchor.current = undefined;
  }, [activeChannelId]);
  useEffect(() => { void refresh(); }, [refresh, refreshVersion]);
  useEffect(() => {
    setAuthorized(false); setManagementMode(false); setAddError(""); setBusy(false);
    setAddProgress(null); setSortProgress(null);
    sorting.current = null; adding.current = null;
  }, [permissionFence, operationFence]);

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

  useEffect(() => {
    if (!isTauri) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<PlaylistAddProgress>("playlist-add-progress", (event) => {
      const job = adding.current;
      if (!active || !job?.current() || !isPlaylistAddProgressForScope(event.payload, job.channelId, job.playlistId, job.requestId)) return;
      setAddProgress(event.payload);
    }).then((stop) => { if (active) unlisten = stop; else stop(); }).catch(() => undefined);
    return () => { active = false; unlisten?.(); };
  }, []);

  const orderedVideos = useMemo(() => [...(currentLibrary ? videos : [])].sort((left, right) => {
    const result = left.title.localeCompare(right.title, undefined, { numeric: true, sensitivity: "base" });
    return videoDirection === "ascending" ? result : -result;
  }), [currentLibrary, videoDirection, videos]);
  const matchingVideos = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    return orderedVideos.filter((video) => (!query || video.title.toLocaleLowerCase().includes(query))
      && (visibility === "all" || video.privacyStatus === visibility));
  }, [orderedVideos, search, visibility]);
  const filteredVideos = useMemo(() => selectedOnly ? matchingVideos.filter((video) => selectedVideoIds.has(video.videoId)) : matchingVideos, [matchingVideos, selectedOnly, selectedVideoIds]);
  const videoWindow = windowItems(filteredVideos, page);
  const filteredVideoIds = useMemo(() => filteredVideos.map((video) => video.videoId), [filteredVideos]);
  const selectedOutsideFilters = useMemo(() => {
    const matching = new Set(matchingVideos.map((video) => video.videoId));
    return [...selectedVideoIds].filter((id) => !matching.has(id)).length;
  }, [matchingVideos, selectedVideoIds]);
  const selectedOffPage = selectedVideoIds.size - videoWindow.items.filter((video) => selectedVideoIds.has(video.videoId)).length;
  const visiblePlaylists = currentLibrary ? playlists : [];
  const destination = visiblePlaylists.find((playlist) => playlist.id === addPlaylistId);
  const selectionDisabled = busy || loading || !currentLibrary;
  const addPercent = addProgress?.total ? Math.min(100, Math.round((addProgress.completed / addProgress.total) * 100)) : 0;
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
  const addSelection = (ids: string[]) => {
    const result = addPlaylistSelection(selectedVideoIds, ids);
    setSelectedVideoIds(result.selected);
    setSelectionDetail(result.omitted ? `${result.omitted} additional videos remain unselected. You can select up to ${PLAYLIST_SELECTION_LIMIT} videos per addition.` : "");
  };
  const updateSelection = (videoId: string, shiftKey: boolean) => {
    const ids = shiftKey ? playlistRangeIds(filteredVideoIds, selectionAnchor.current, videoId) : [videoId];
    if (selectedVideoIds.has(videoId)) {
      const next = new Set(selectedVideoIds);
      ids.forEach((id) => next.delete(id));
      setSelectedVideoIds(next);
      setSelectionDetail("");
    } else addSelection(ids);
    selectionAnchor.current = videoId;
  };
  const addSelectedVideos = () => {
    if (!destination || selectedVideoIds.size === 0) return;
    const playlistId = destination.id;
    const playlistTitle = destination.title;
    const requestedIds = orderedVideos.filter((video) => selectedVideoIds.has(video.videoId)).map((video) => video.videoId);
    return run(async (current) => {
      const job = { current, channelId: activeChannelId ?? "", playlistId, requestId: crypto.randomUUID() };
      adding.current = job;
      setReceiptPlaylistTitle(playlistTitle);
      setAddReceipt(null); setAddError("");
      setAddProgress({ channelId: job.channelId, playlistId, requestId: job.requestId, completed: 0, total: requestedIds.length, added: 0, skipped: 0, currentTitle: "", status: "running", detail: "Checking the playlist for videos already present…" });
      try {
        const receipt = await addRemoteVideosToPlaylist(playlistId, requestedIds, job.channelId, job.requestId);
        if (!current()) return;
        if (!isPlaylistAddReceiptForSelection(receipt, job.channelId, playlistId, requestedIds)) throw new Error("The playlist result could not be matched to this selection. Your selection is kept; refresh the playlist before retrying.");
        setAddReceipt(receipt);
        const confirmed = new Set([...receipt.addedVideoIds, ...receipt.skippedVideoIds]);
        setSelectedVideoIds((selected) => new Set([...selected].filter((id) => !confirmed.has(id))));
        const completed = receipt.addedVideoIds.length + receipt.skippedVideoIds.length;
        const detail = receipt.error ?? (receipt.remainingVideoIds.length ? "Some videos still need to be added." : "Finished adding the reviewed selection.");
        setAddProgress({ channelId: job.channelId, playlistId, requestId: job.requestId, completed, total: requestedIds.length, added: receipt.addedVideoIds.length, skipped: receipt.skippedVideoIds.length, currentTitle: "", status: receipt.remainingVideoIds.length ? "error" : "completed", detail });
        onNotice(`${receipt.addedVideoIds.length} added to “${playlistTitle}”; ${receipt.skippedVideoIds.length} already present${receipt.remainingVideoIds.length ? `; ${receipt.remainingVideoIds.length} remaining` : ""}.`);
      } catch (error) {
        if (current()) {
          const detail = workspaceErrorMessage(error, "The playlist addition could not be confirmed. Your selection is kept.");
          setAddError(detail);
          setAddProgress((progress) => progress ? { ...progress, status: "error", detail } : null);
        }
        throw error;
      } finally { if (adding.current === job) adding.current = null; }
    });
  };
  const refreshManagementPermission = async () => {
    const current = permissionFence.begin();
    if (!isTauri || !currentLibrary) return;
    try {
      const settings = await loadConnectionSettings();
      assertWorkspaceChannel(settings, activeChannelId);
      if (!current()) return;
      setAuthorized(settings.deletionAuthorized === true);
      setManagementMode(settings.deletionSudoActive === true);
    } catch (error) {
      if (current()) onNotice(workspaceErrorMessage(error, "Playlist permissions could not be refreshed. Refresh the library before adding videos."));
    }
  };
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

  return <section className="mt-4 grid gap-4 rounded-xl border border-line bg-surface p-4 text-ink sm:p-5 [&_h3]:m-0">
    <header><p className="ui-eyebrow">ACTIVE CHANNEL LIBRARY</p><h2>Playlists</h2><p className="ui-subtle">Create and fill playlists separately from managing an existing playlist's order.</p></header>
    <button className={`${buttonClass} justify-self-start`} disabled={loading || busy} onClick={() => void refresh()} type="button">Refresh library &amp; permission</button>
    {loading && <p className="ui-subtle" role="status">Loading this channel’s library and playlists…</p>}
    {loadError && <div className="grid gap-2" role="alert"><p className="text-sm font-semibold text-danger">{loadError}</p><button className={buttonClass} disabled={loading || busy} onClick={() => void refresh()} type="button">Retry library load</button></div>}
    <div aria-label="Playlist workspace" className="flex flex-wrap gap-2 border-b border-line pb-3" role="tablist">
      <button aria-controls="playlist-panel-add" aria-selected={tab === "add"} className={tab === "add" ? primaryButtonClass : buttonClass} id="playlist-tab-add" onClick={() => setTab("add")} onKeyDown={handleTabKey} role="tab" tabIndex={tab === "add" ? 0 : -1} type="button">Create &amp; add videos</button>
      <button aria-controls="playlist-panel-manage" aria-selected={tab === "manage"} className={tab === "manage" ? primaryButtonClass : buttonClass} id="playlist-tab-manage" onClick={() => setTab("manage")} onKeyDown={handleTabKey} role="tab" tabIndex={tab === "manage" ? 0 : -1} type="button">Manage &amp; sort playlist</button>
    </div>
    {tab === "add" ? <div aria-labelledby="playlist-tab-add" className="grid gap-4" id="playlist-panel-add" role="tabpanel">
      <section className="grid gap-3 rounded-lg border border-line p-4">
        <div><h3 className="text-base font-bold text-[#182b49]">1. Choose where to add videos</h3><p className="ui-subtle">Select a playlist in {activeChannel}, or create one here.</p></div>
        <div className="flex flex-wrap items-end gap-3">
          <label className="grid min-w-0 flex-1 gap-1 text-sm font-semibold text-[#344a67]">Destination playlist<select className={controlClass} disabled={selectionDisabled} onChange={(event) => setAddPlaylistId(event.target.value)} value={addPlaylistId}><option value="">Choose a playlist</option>{visiblePlaylists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}</select></label>
          <button className={buttonClass} disabled={selectionDisabled} onClick={() => setCreateChannelId(activeChannelId)} type="button">Create playlist</button>
        </div>
        {currentLibrary && !loading && visiblePlaylists.length === 0 && <p className="ui-subtle">No playlists yet. Create your first playlist, then add your selected videos.</p>}
      </section>
      <section className="grid gap-3 rounded-lg border border-line p-4">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div><h3 className="text-base font-bold text-[#182b49]">2. Select videos to add</h3><p className="ui-subtle">Selections stay checked across searches and pages. Shift-click or Shift-Space selects a range.</p></div>
          <label className="grid gap-1 text-sm font-semibold text-[#344a67]">Display order<select className={controlClass} disabled={busy} onChange={(event) => { setVideoDirection(event.target.value as Direction); setPage(1); selectionAnchor.current = undefined; }} value={videoDirection}><option value="ascending">Title A–Z</option><option value="descending">Title Z–A</option></select></label>
        </div>
        <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto]">
          <label className="grid gap-1 text-sm font-semibold text-[#344a67]">Search videos<input className={controlClass} disabled={selectionDisabled} onChange={(event) => { setSearch(event.target.value); setPage(1); selectionAnchor.current = undefined; }} placeholder="Search by video title" type="search" value={search} /></label>
          <label className="grid gap-1 text-sm font-semibold text-[#344a67]">Video visibility<select className={controlClass} disabled={selectionDisabled} onChange={(event) => { setVisibility(event.target.value); setPage(1); selectionAnchor.current = undefined; }} value={visibility}><option value="all">All visibility</option><option value="private">Private</option><option value="unlisted">Unlisted</option><option value="public">Public</option></select></label>
        </div>
        <div aria-live="polite" className="flex flex-wrap items-baseline gap-x-3 gap-y-1 rounded-md bg-[#f3f5f8] px-3 py-2 text-sm text-[#344a67]">
          <strong>{selectedVideoIds.size} of {PLAYLIST_SELECTION_LIMIT} selected</strong>
          <span>{filteredVideos.length} {selectedOnly ? "selected videos shown" : "matching videos"}</span>
          {selectedOffPage > 0 && <span>{selectedOffPage} selected not on this page{selectedOutsideFilters > 0 ? ` (${selectedOutsideFilters} outside filters)` : ""}</span>}
        </div>
        <div className="flex flex-wrap gap-2">
          <button className={buttonClass} disabled={selectionDisabled || videoWindow.items.length === 0} onClick={() => addSelection(videoWindow.items.map((video) => video.videoId))} type="button">Select this page ({videoWindow.items.length})</button>
          <button className={buttonClass} disabled={selectionDisabled || filteredVideos.length === 0} onClick={() => addSelection(filteredVideoIds)} type="button">Select matching ({filteredVideos.length})</button>
          <button aria-pressed={selectedOnly} className={buttonClass} disabled={selectionDisabled || (!selectedOnly && selectedVideoIds.size === 0)} onClick={() => { setSelectedOnly(!selectedOnly); setSearch(""); setVisibility("all"); setPage(1); selectionAnchor.current = undefined; }} type="button">{selectedOnly ? "Show all videos" : "Show selected"}</button>
          <button className={buttonClass} disabled={selectionDisabled || selectedVideoIds.size === 0} onClick={() => { setSelectedVideoIds(new Set()); setSelectionDetail(""); selectionAnchor.current = undefined; }} type="button">Clear selection</button>
        </div>
        {selectionDetail && <p className="text-sm font-semibold text-[#344a67]" role="status">{selectionDetail}</p>}
        <div aria-label="Available videos" className="grid max-h-80 gap-2 overflow-auto">
          {videoWindow.items.map((video) => <label className={`flex cursor-pointer items-start gap-3 rounded-md border p-3 text-sm text-[#182b49] ${selectedVideoIds.has(video.videoId) ? "border-[#b5cbed] bg-[#f6f9ff]" : "border-line"}`} key={video.videoId}>
            <input className="mt-1 size-4 shrink-0 accent-brand focus-visible:outline-2 focus-visible:outline-focus-ring" checked={selectedVideoIds.has(video.videoId)} disabled={selectionDisabled} onClick={(event) => { rangeGesture.current = event.shiftKey || rangeGesture.current; }} onKeyDown={(event) => { if (event.key === " ") rangeGesture.current = event.shiftKey; }} onChange={() => { updateSelection(video.videoId, rangeGesture.current); rangeGesture.current = false; }} type="checkbox" />
            <span className="min-w-0 flex-1 break-words">{video.title}<span className="mt-1 block text-xs text-[#60728c]">{video.privacyStatus ?? "Visibility unavailable"}{video.duration ? ` · ${video.duration}` : ""}</span></span>
          </label>)}
        </div>
        {currentLibrary && !loading && videos.length === 0 && <p className="ui-subtle">No saved videos. Sync the YouTube library before selecting videos to add.</p>}
        {currentLibrary && !loading && videos.length > 0 && filteredVideos.length === 0 && <div className="grid justify-items-start gap-2"><p className="ui-subtle">{selectedOnly && selectedVideoIds.size === 0 ? "No videos are selected. Show all videos to choose your next batch." : "No videos match these filters. Your other selections are kept."}</p>{(search || visibility !== "all") && <button className={buttonClass} disabled={busy} onClick={() => { setSearch(""); setVisibility("all"); setPage(1); }} type="button">Clear filters</button>}</div>}
        {videoWindow.pageCount > 1 && <nav aria-label="Available video pages" className="flex flex-wrap items-center justify-between gap-2"><span className="ui-subtle">{videoWindow.start + 1}–{videoWindow.end} of {videoWindow.total} videos · Page {videoWindow.page} of {videoWindow.pageCount}</span><div className="flex gap-2"><button className={buttonClass} disabled={busy || videoWindow.page === 1} onClick={() => setPage(videoWindow.page - 1)} type="button">Previous</button><button className={buttonClass} disabled={busy || videoWindow.page === videoWindow.pageCount} onClick={() => setPage(videoWindow.page + 1)} type="button">Next</button></div></nav>}
      </section>
      <section className="grid gap-3 rounded-lg border border-line p-4">
        <div><h3 className="text-base font-bold text-[#182b49]">3. Review &amp; add</h3><p className="ui-subtle">{destination ? `${selectedVideoIds.size} selected videos → “${destination.title}” in ${activeChannel}.` : "Choose a destination playlist and select videos above."}</p><p className="ui-subtle">Videos already in the playlist are skipped. Selected videos are added in {videoDirection === "ascending" ? "A–Z" : "Z–A"} title order.</p></div>
        {addProgress && <section aria-live="polite" className={`grid gap-2 rounded-lg border p-3 ${addProgress.status === "error" ? "border-[#f0d0cc] bg-[#fff7f6]" : "border-[#d8e5f8] bg-[#f6f9ff]"}`}>
          <strong className="break-words text-sm text-[#30496c]">{addProgress.status === "running" ? "Adding" : addProgress.status === "error" ? "Addition needs attention" : "Addition complete"} · {receiptPlaylistTitle}</strong>
          <p className="text-sm text-[#344a67]">{addReceipt ? `${addReceipt.addedVideoIds.length} added · ${addReceipt.skippedVideoIds.length} already present · ${addReceipt.remainingVideoIds.length} remaining` : `${addProgress.completed} of ${addProgress.total} checked · ${addProgress.added} added · ${addProgress.skipped} already present`}</p>
          <div aria-label="Playlist addition progress" aria-valuemax={addProgress.total || 1} aria-valuemin={0} aria-valuenow={addProgress.completed} className="h-2 overflow-hidden rounded-full bg-[#d7e2f2]" role="progressbar"><span className={`block h-full rounded-[inherit] transition-[width] duration-200 ${addProgress.status === "error" ? "bg-[#c95146]" : addProgress.status === "completed" ? "bg-[#39866a]" : "bg-[#2463df]"}`} style={{ width: `${addPercent}%` }} /></div>
          <p className="break-words text-xs text-[#445d80]">{addProgress.currentTitle ? `Working on “${addProgress.currentTitle}”. ` : ""}{addProgress.detail}</p>
          {(addError || Boolean(addReceipt?.remainingVideoIds.length)) && <p className="text-sm text-[#344a67]">{addReceipt?.remainingVideoIds.length ? "Unfinished videos stay selected." : "Your selection is kept because the result is unconfirmed."} Retrying checks the playlist before adding each video.{addReceipt?.uncertain ? " The last video's result needs confirmation." : ""}</p>}
        </section>}
        <footer className="flex flex-wrap items-center gap-2 border-t border-line pt-3">{managementGate ?? <button className={primaryButtonClass} disabled={!destination || !selectedVideoIds.size || selectionDisabled} onClick={() => void addSelectedVideos()} type="button">{busy && addProgress?.status === "running" ? "Adding videos…" : (addError || addReceipt?.remainingVideoIds.length) && addProgress?.playlistId === addPlaylistId ? `Retry ${selectedVideoIds.size} selected videos` : `Add ${selectedVideoIds.size} selected videos`}</button>}</footer>
      </section>
    </div> : <div aria-labelledby="playlist-tab-manage" className="grid gap-4" id="playlist-panel-manage" role="tabpanel">
      <section className="grid gap-3 rounded-lg border border-line p-4"><div><h3 className="text-base font-bold text-[#182b49]">Select a playlist to manage</h3><p className="ui-subtle">This tab changes the selected playlist only; it does not add videos.</p></div><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Playlist<select className={controlClass} onChange={(event) => setManagePlaylistId(event.target.value)} value={managePlaylistId}><option value="">Choose a playlist</option>{visiblePlaylists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}</select></label><button className={buttonClass} disabled={!managePlaylistId || busy || loading || !currentLibrary} onClick={() => void openStudioSettings()} type="button">Open playlist settings in YouTube Studio</button></section>
      <section className="grid gap-3 rounded-lg border border-line p-4"><div><h3 className="text-base font-bold text-[#182b49]">Custom title order</h3><p className="ui-subtle">In YouTube Studio, set the playlist order to Manual first. The app verifies the final order after every request.</p></div><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Order playlist videos by title<select className={controlClass} onChange={(event) => setPlaylistDirection(event.target.value as Direction)} value={playlistDirection}><option value="ascending">Ascending — A–Z</option><option value="descending">Descending — Z–A</option></select></label>
        {sortProgress && <section aria-live="polite" className={`grid gap-2 rounded-lg border p-3 ${sortProgress.status === "error" ? "border-[#f0d0cc] bg-[#fff7f6]" : "border-[#d8e5f8] bg-[#f6f9ff]"}`}><div className="flex flex-wrap items-baseline justify-between gap-2"><strong className="text-sm text-[#30496c]">Sort progress {sortProgress.total ? `${sortProgress.completed} of ${sortProgress.total}` : ""}</strong><span className="text-xs text-[#60728c]">{sortProgress.currentTitle ? `Working on “${sortProgress.currentTitle}”` : sortProgress.detail}</span></div><div aria-label="Playlist custom order progress" aria-valuemax={sortProgress.total || 1} aria-valuemin={0} aria-valuenow={sortProgress.completed} className="h-2 overflow-hidden rounded-full bg-[#d7e2f2]" role="progressbar"><span className={`block h-full rounded-[inherit] transition-[width] duration-200 ${sortProgress.status === "error" ? "bg-[#c95146]" : sortProgress.status === "completed" ? "bg-[#39866a]" : "bg-[#2463df]"}`} style={{ width: `${sortPercent}%` }} /></div><details className="text-xs text-[#445d80]"><summary className="cursor-pointer font-bold">Recent sort activity ({sortLog.length}, up to 32)</summary><ol className="mt-2 grid list-none gap-1 p-0">{sortLog.map((entry, index) => <li className="rounded border border-line bg-white px-2 py-1.5" key={`${index}-${entry.completed}-${entry.detail}`}><strong>{entry.status === "error" ? "Needs attention" : entry.status === "completed" ? "Verified" : entry.completed ? `Video ${entry.completed} of ${entry.total}` : "Preparing"}</strong><span className="ml-2">{entry.detail}</span></li>)}</ol></details></section>}
        <footer className="flex flex-wrap gap-2 border-t border-line pt-3">{managementGate ?? <><button className={buttonClass} disabled={busy || loading || !currentLibrary} onClick={() => void grantManagement()} type="button">Reauthorize management with Google</button><button className={primaryButtonClass} disabled={!managePlaylistId || busy || loading || !currentLibrary} onClick={() => void applyCustomOrder()} type="button">{busy && sortProgress?.status === "running" ? "Applying custom order…" : "Apply custom order"}</button></>}</footer>
      </section>
    </div>}
    {createChannelId && createChannelId === activeChannelId && <Suspense fallback={<p className="ui-subtle" role="status">Opening playlist creation…</p>}><PlaylistCreateDialog expectedChannelId={createChannelId} onClose={() => { setCreateChannelId(undefined); void refreshManagementPermission(); }} onCreated={(playlist) => {
      if (createChannelId !== activeChannelId || !currentLibrary) return;
      setPlaylists((current) => [...current.filter((existing) => existing.id !== playlist.id), playlist]);
      setAddPlaylistId(playlist.id);
      setCreateChannelId(undefined);
      onNotice(`Created “${playlist.title}”. It is selected as the destination; your video selection is kept.`);
    }} /></Suspense>}
  </section>;
}
