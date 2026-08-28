import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
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
import type { PlaylistPrivacy, RemoteVideo, YouTubePlaylist } from "../lib/types";

type Props = { activeChannel?: string; onNotice: (message: string) => void; refreshVersion?: number };
type WorkspaceTab = "add" | "manage";
type Direction = "ascending" | "descending";
type PlaylistSortProgress = { completed: number; total: number; currentTitle: string; status: "running" | "completed" | "error"; detail: string };

const buttonClass = "cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2 text-sm font-bold text-[#344a67] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50";
const primaryButtonClass = "cursor-pointer rounded-md border border-[#1f5ea8] bg-[#1f5ea8] px-3 py-2 text-sm font-bold text-white hover:bg-[#174d8c] disabled:cursor-not-allowed disabled:opacity-50";

export default function PlaylistManager({ activeChannel, onNotice, refreshVersion = 0 }: Props) {
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

  const refresh = useCallback(async () => {
    if (!isTauri || !activeChannel) return;
    const [remoteVideos, remotePlaylists, settings] = await Promise.all([listRemoteVideos(), listYouTubePlaylists(), loadConnectionSettings()]);
    setVideos(remoteVideos); setPlaylists(remotePlaylists);
    setAuthorized(settings.deletionAuthorized === true); setManagementMode(settings.deletionSudoActive === true);
  }, [activeChannel]);

  useEffect(() => { void refresh().catch((error) => onNotice(error instanceof Error ? error.message : "The YouTube library could not be loaded.")); }, [onNotice, refresh, refreshVersion]);

  useEffect(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    void listen<PlaylistSortProgress>("playlist-sort-progress", (event) => {
      setSortProgress(event.payload);
      setSortLog((previous) => [...previous, event.payload]);
    }).then((stop) => { unlisten = stop; }).catch(() => undefined);
    return () => unlisten?.();
  }, []);

  const orderedVideos = useMemo(() => [...videos].sort((left, right) => {
    const result = left.title.localeCompare(right.title, undefined, { numeric: true, sensitivity: "base" });
    return videoDirection === "ascending" ? result : -result;
  }), [videoDirection, videos]);
  const sortPercent = sortProgress?.total ? Math.round((sortProgress.completed / sortProgress.total) * 100) : 0;

  const run = async (action: () => Promise<void>) => { setBusy(true); try { await action(); } catch (error) { onNotice(error instanceof Error ? error.message : "Playlist operation failed."); } finally { setBusy(false); } };
  const grantManagement = () => run(async () => { const { authorizationUrl } = await beginDeletionAuthorization(); await openAndCopyGoogleAuthorization(authorizationUrl); onNotice("Grant YouTube video-management permission, then return here."); });
  const enterManagementMode = () => run(async () => { const settings = await enableDeletionSudoMode(); setManagementMode(settings.deletionSudoActive === true); });
  const openStudioSettings = () => run(async () => { if (!managePlaylistId) return; const url = `https://studio.youtube.com/playlist/${encodeURIComponent(managePlaylistId)}/edit`; if (isTauri) { const { openUrl } = await import("@tauri-apps/plugin-opener"); await openUrl(url); } else { window.open(url, "_blank", "noopener,noreferrer"); } });
  const updateSelection = (videoId: string) => setSelectedVideoIds((current) => { const next = new Set(current); if (next.has(videoId)) next.delete(videoId); else next.add(videoId); return next; });
  const applyCustomOrder = () => run(async () => {
    setSortProgress({ completed: 0, total: 0, currentTitle: "", status: "running", detail: "Loading playlist videos from YouTube…" });
    setSortLog([{ completed: 0, total: 0, currentTitle: "", status: "running", detail: "Loading playlist videos from YouTube…" }]);
    const count = await sortPlaylistItemsByTitle(managePlaylistId, playlistDirection);
    onNotice(`Verified ${playlistDirection === "ascending" ? "A–Z" : "Z–A"} custom order for ${count} videos.`);
  });

  if (!activeChannel) return <section className="mt-4 rounded-xl border border-line bg-white p-5"><h2>Playlists</h2><p className="ui-subtle">Connect and sync a YouTube channel first.</p></section>;
  const managementGate = !authorized ? <button className={primaryButtonClass} disabled={busy} onClick={() => void grantManagement()} type="button">Grant playlist-management permission</button> : !managementMode ? <button className={primaryButtonClass} disabled={busy} onClick={() => void enterManagementMode()} type="button">Enter playlist-management mode</button> : null;

  return <section className="mt-4 grid gap-4 rounded-xl border border-line bg-white p-5">
    <header><p className="ui-eyebrow">ACTIVE CHANNEL LIBRARY</p><h2>Playlists</h2><p className="ui-subtle">Create and fill playlists separately from managing an existing playlist's order.</p></header>
    <div aria-label="Playlist workspace" className="flex flex-wrap gap-2 border-b border-line pb-3" role="tablist">
      <button aria-selected={tab === "add"} className={tab === "add" ? primaryButtonClass : buttonClass} onClick={() => setTab("add")} role="tab" type="button">Create &amp; add videos</button>
      <button aria-selected={tab === "manage"} className={tab === "manage" ? primaryButtonClass : buttonClass} onClick={() => setTab("manage")} role="tab" type="button">Manage &amp; sort playlist</button>
    </div>
    {tab === "add" ? <div className="grid gap-4" role="tabpanel">
      <section className="grid gap-3 rounded-lg border border-line p-4"><div><h3 className="text-base font-bold text-[#182b49]">1. Choose where to add videos</h3><p className="ui-subtle">Select an existing playlist, or create one first.</p></div>
        <label className="grid gap-1 text-sm font-semibold text-[#344a67]">Existing playlist<select onChange={(event) => setAddPlaylistId(event.target.value)} value={addPlaylistId}><option value="">Choose a playlist</option>{playlists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}</select></label>
        <div className="grid gap-2 border-t border-line pt-3 md:grid-cols-[1fr_auto_auto]"><label className="grid gap-1 text-sm font-semibold text-[#344a67]">New playlist name<input onChange={(event) => setNewPlaylistTitle(event.target.value)} placeholder="Playlist name" value={newPlaylistTitle} /></label><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Visibility<select onChange={(event) => setPrivacy(event.target.value as PlaylistPrivacy)} value={privacy}><option value="private">Private</option><option value="unlisted">Unlisted</option><option value="public">Public</option></select></label><button className={buttonClass} disabled={!newPlaylistTitle.trim() || busy} onClick={() => void run(async () => { const playlist = await createYouTubePlaylist(newPlaylistTitle, privacy); setPlaylists((current) => [...current, playlist]); setAddPlaylistId(playlist.id); setNewPlaylistTitle(""); onNotice(`Created ${privacy} playlist “${playlist.title}”.`); })} type="button">Create playlist</button></div>
      </section>
      <section className="grid gap-3 rounded-lg border border-line p-4"><div className="flex flex-wrap items-end justify-between gap-3"><div><h3 className="text-base font-bold text-[#182b49]">2. Select videos to add</h3><p className="ui-subtle">{selectedVideoIds.size} selected from your current channel library.</p></div><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Display order<select onChange={(event) => setVideoDirection(event.target.value as Direction)} value={videoDirection}><option value="ascending">Title A–Z</option><option value="descending">Title Z–A</option></select></label></div><div className="flex flex-wrap gap-2"><button className={buttonClass} onClick={() => setSelectedVideoIds(new Set(videos.map((video) => video.videoId)))} type="button">Select all</button><button className={buttonClass} onClick={() => setSelectedVideoIds(new Set())} type="button">Clear selection</button></div><div aria-label="Available videos" className="grid max-h-80 gap-2 overflow-auto">{orderedVideos.map((video) => <label className="flex cursor-pointer gap-2 rounded-md border border-line p-3 text-sm text-[#182b49]" key={video.videoId}><input checked={selectedVideoIds.has(video.videoId)} onChange={() => updateSelection(video.videoId)} type="checkbox" /><span>{video.title}</span></label>)}</div><footer className="flex flex-wrap items-center gap-2 border-t border-line pt-3">{managementGate ?? <button className={primaryButtonClass} disabled={!addPlaylistId || !selectedVideoIds.size || busy} onClick={() => void run(async () => { const count = await addRemoteVideosToPlaylist(addPlaylistId, [...selectedVideoIds]); setSelectedVideoIds(new Set()); onNotice(`Added ${count} videos to the selected playlist.`); })} type="button">Add selected videos</button>}</footer></section>
    </div> : <div className="grid gap-4" role="tabpanel">
      <section className="grid gap-3 rounded-lg border border-line p-4"><div><h3 className="text-base font-bold text-[#182b49]">Select a playlist to manage</h3><p className="ui-subtle">This tab changes the selected playlist only; it does not add videos.</p></div><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Playlist<select onChange={(event) => setManagePlaylistId(event.target.value)} value={managePlaylistId}><option value="">Choose a playlist</option>{playlists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}</select></label><button className={buttonClass} disabled={!managePlaylistId || busy} onClick={() => void openStudioSettings()} type="button">Open playlist settings in YouTube Studio</button></section>
      <section className="grid gap-3 rounded-lg border border-line p-4"><div><h3 className="text-base font-bold text-[#182b49]">Custom title order</h3><p className="ui-subtle">In YouTube Studio, set the playlist order to Manual first. The app verifies the final order after every request.</p></div><label className="grid gap-1 text-sm font-semibold text-[#344a67]">Order playlist videos by title<select onChange={(event) => setPlaylistDirection(event.target.value as Direction)} value={playlistDirection}><option value="ascending">Ascending — A–Z</option><option value="descending">Descending — Z–A</option></select></label>
        {sortProgress && <section aria-live="polite" className={`grid gap-2 rounded-lg border p-3 ${sortProgress.status === "error" ? "border-[#f0d0cc] bg-[#fff7f6]" : "border-[#d8e5f8] bg-[#f6f9ff]"}`}><div className="flex flex-wrap items-baseline justify-between gap-2"><strong className="text-sm text-[#30496c]">Sort progress {sortProgress.total ? `${sortProgress.completed} of ${sortProgress.total}` : ""}</strong><span className="text-xs text-[#60728c]">{sortProgress.currentTitle ? `Working on “${sortProgress.currentTitle}”` : sortProgress.detail}</span></div><div aria-valuemax={sortProgress.total || 1} aria-valuemin={0} aria-valuenow={sortProgress.completed} className="h-2 overflow-hidden rounded-full bg-[#d7e2f2]" role="progressbar"><span className={`block h-full rounded-[inherit] transition-[width] duration-200 ${sortProgress.status === "error" ? "bg-[#c95146]" : sortProgress.status === "completed" ? "bg-[#39866a]" : "bg-[#2463df]"}`} style={{ width: `${sortPercent}%` }} /></div><details className="text-xs text-[#445d80]"><summary className="cursor-pointer font-bold">Sort activity log ({sortLog.length})</summary><ol className="mt-2 grid list-none gap-1 p-0">{sortLog.map((entry, index) => <li className="rounded border border-line bg-white px-2 py-1.5" key={`${index}-${entry.completed}-${entry.detail}`}><strong>{entry.status === "error" ? "Needs attention" : entry.status === "completed" ? "Verified" : entry.completed ? `Video ${entry.completed} of ${entry.total}` : "Preparing"}</strong><span className="ml-2">{entry.detail}</span></li>)}</ol></details></section>}
        <footer className="flex flex-wrap gap-2 border-t border-line pt-3">{managementGate ?? <><button className={buttonClass} disabled={busy} onClick={() => void grantManagement()} type="button">Reauthorize management with Google</button><button className={primaryButtonClass} disabled={!managePlaylistId || busy} onClick={() => void applyCustomOrder()} type="button">{busy ? "Applying custom order…" : "Apply custom order"}</button></>}</footer>
      </section>
    </div>}
  </section>;
}
