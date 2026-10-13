import { CalendarClock, Plus, RefreshCw } from "lucide-react";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import {
  listYouTubePlaylists,
  loadConnectionSettings,
} from "../lib/local";
import { loadManualUploadDefaults } from "../lib/manual-upload-defaults";
import { assertWorkspaceChannel, useWorkspaceRequestFence } from "../lib/workspace-requests";
import type {
  ManualUploadSettings,
  ManualUploadDefaults,
  UploadMetadata,
  UploadVisibility,
  YouTubePlaylist,
} from "../lib/types";

type UploadIntakeReviewProps = {
  paths: string[];
  activeChannelId?: string;
  onCancel: () => void;
  onConfirm: (settings: ManualUploadSettings) => void;
};

const PlaylistCreateDialog = lazy(() => import("./PlaylistCreateDialog").then((module) => ({ default: module.PlaylistCreateDialog })));
const UploadMetadataDialog = lazy(() => import("./UploadMetadataDialog").then((module) => ({ default: module.UploadMetadataDialog })));
const PublicationCalendarDialog = lazy(() => import("./PublicationCalendarDialog").then((module) => ({ default: module.PublicationCalendarDialog })));

/** Per-item metadata is a complete replacement, including deliberately empty overrides. */
export function intakePublicationTimes(paths: string[], metadata?: UploadMetadata, itemMetadata?: Record<string, UploadMetadata>): Record<string, string> {
  return Object.fromEntries(paths.flatMap((path) => {
    const selected = itemMetadata && Object.hasOwn(itemMetadata, path) ? itemMetadata[path] : metadata;
    return selected?.publishAt ? [[path, selected.publishAt]] : [];
  }));
}

export function applyIntakePublicationTimes(paths: string[], publishAtByPath: Record<string, string>, metadata?: UploadMetadata, itemMetadata?: Record<string, UploadMetadata>): Record<string, UploadMetadata> {
  if (!paths.length || new Set(paths).size !== paths.length || Object.keys(publishAtByPath).length !== paths.length || paths.some((path) => !Object.hasOwn(publishAtByPath, path) || !Number.isFinite(Date.parse(publishAtByPath[path])))) throw new Error("Review publication times for every selected original before applying.");
  return Object.fromEntries(paths.map((path) => [path, { ...(itemMetadata && Object.hasOwn(itemMetadata, path) ? itemMetadata[path] : metadata), publishAt: publishAtByPath[path] }]));
}

export function retainIntakePublicationContexts(previous: Record<string, string>, next: Record<string, string>, contexts: ManualUploadSettings["publicationContexts"]): ManualUploadSettings["publicationContexts"] {
  const retained = Object.fromEntries(Object.entries(contexts ?? {}).filter(([path]) => Object.hasOwn(previous, path) && Object.hasOwn(next, path) && Number.isFinite(Date.parse(next[path])) && Date.parse(previous[path]) === Date.parse(next[path])));
  return Object.keys(retained).length ? retained : undefined;
}

/** A playlist default must still belong to this reviewed channel and its current list. */
export function resolveDefaultUploadPlaylist(defaults: ManualUploadDefaults | undefined, channelId: string | undefined, playlists: YouTubePlaylist[]): YouTubePlaylist | undefined {
  if (!channelId || defaults?.playlistChannelId !== channelId || !defaults.playlistId) return undefined;
  return playlists.find((playlist) => playlist.id === defaults.playlistId);
}

export function UploadIntakeReview({
  paths,
  activeChannelId,
  onCancel,
  onConfirm,
}: UploadIntakeReviewProps) {
  const selectionKey = JSON.stringify(paths);
  const [madeForKids, setMadeForKids] = useState<boolean>();
  const [visibility, setVisibility] = useState<UploadVisibility>("private");
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistId, setPlaylistId] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [metadataOpen, setMetadataOpen] = useState(false);
  const [publicationOpen, setPublicationOpen] = useState(false);
  const [metadata, setMetadata] = useState<UploadMetadata>();
  const [itemMetadata, setItemMetadata] = useState<Record<string, UploadMetadata>>();
  const [publicationContexts, setPublicationContexts] = useState<ManualUploadSettings["publicationContexts"]>();
  const [playlistNotice, setPlaylistNotice] = useState("");
  const [playlistRefresh, setPlaylistRefresh] = useState(0);
  const [loadingPlaylists, setLoadingPlaylists] = useState(false);
  const createButton = useRef<HTMLButtonElement>(null);
  const creationWasOpen = useRef(false);
  const fence = useWorkspaceRequestFence(activeChannelId);
  const [deleteSourceAfterUpload, setDeleteSourceAfterUpload] = useState(false);
  const [playlistError, setPlaylistError] = useState("");
  const [savedDefaults, setSavedDefaults] = useState<ManualUploadDefaults>();
  const [loadingDefaults, setLoadingDefaults] = useState(true);
  const [defaultsError, setDefaultsError] = useState("");
  const [defaultPlaylistNotice, setDefaultPlaylistNotice] = useState("");
  const defaultsFence = useWorkspaceRequestFence(activeChannelId);
  const visibilityChosen = useRef(false);
  const playlistChosen = useRef(false);

  useEffect(() => {
    setPlaylists([]);
    setPlaylistId("");
    setPlaylistNotice("");
    setDefaultPlaylistNotice("");
    playlistChosen.current = false;
    setCreateOpen(false);
    setMetadataOpen(false);
    setPublicationOpen(false);
    setMetadata(undefined);
    setItemMetadata(undefined);
    setPublicationContexts(undefined);
  }, [activeChannelId, selectionKey]);

  useEffect(() => {
    const current = fence.begin();
    setLoadingPlaylists(Boolean(activeChannelId));
    setPlaylistError("");
    void (async () => {
      try {
        assertWorkspaceChannel(await loadConnectionSettings(), activeChannelId);
        if (!current()) return;
        const items = await listYouTubePlaylists();
        assertWorkspaceChannel(await loadConnectionSettings(), activeChannelId);
        if (current()) {
          setPlaylists(items);
          setPlaylistId((selected) => items.some((item) => item.id === selected) ? selected : "");
        }
      } catch {
        if (current()) setPlaylistError("Playlists could not be loaded. You can continue without one.");
      } finally {
        if (current()) setLoadingPlaylists(false);
      }
    })();
    return () => fence.invalidate();
  }, [activeChannelId, fence, playlistRefresh]);
  useEffect(() => {
    const current = defaultsFence.begin();
    setLoadingDefaults(true);
    setSavedDefaults(undefined);
    setDefaultsError("");
    void loadManualUploadDefaults(activeChannelId).then((defaults) => {
      if (!current()) return;
      setMadeForKids((choice) => choice ?? defaults.madeForKids);
      if (!visibilityChosen.current) setVisibility(defaults.visibility ?? "private");
      setSavedDefaults(defaults);
    }).catch(() => {
      if (current()) setDefaultsError("Saved defaults could not be loaded. Review these choices before importing.");
    }).finally(() => { if (current()) setLoadingDefaults(false); });
    return () => defaultsFence.invalidate();
  }, [activeChannelId, defaultsFence]);

  useEffect(() => {
    if (playlistChosen.current || loadingDefaults || loadingPlaylists) return;
    const defaultId = activeChannelId && savedDefaults?.playlistChannelId === activeChannelId ? savedDefaults?.playlistId : undefined;
    const available = resolveDefaultUploadPlaylist(savedDefaults, activeChannelId, playlists);
    setPlaylistId(available?.id ?? "");
    setDefaultPlaylistNotice(defaultId ? available
      ? `“${available.title}” is selected from your device default. You can change it for this batch.`
      : "Your default playlist is unavailable for this channel. Refresh playlists or review a different choice before importing."
      : "");
  }, [activeChannelId, loadingDefaults, loadingPlaylists, playlists, savedDefaults]);

  useEffect(() => {
    if (!createOpen && creationWasOpen.current) createButton.current?.focus();
    creationWasOpen.current = createOpen;
  }, [createOpen]);

  const selectedPlaylist = playlists.find(
    (playlist) => playlist.id === playlistId,
  );
  const publicationTimes = intakePublicationTimes(paths, metadata, itemMetadata);
  const publicationCount = Object.keys(publicationTimes).length;
  const nestedOpen = createOpen || metadataOpen || publicationOpen;
  const playlistCreated = (playlist: YouTubePlaylist) => {
    playlistChosen.current = true;
    fence.invalidate();
    setLoadingPlaylists(false);
    setPlaylists((current) => [...current.filter((item) => item.id !== playlist.id), playlist].sort((left, right) => left.title.localeCompare(right.title)));
    setPlaylistId(playlist.id);
    setPlaylistError("");
    setPlaylistNotice(`“${playlist.title}” is selected for these uploads.`);
  };
  return (
    <>
    <section
      className="fixed top-1/2 left-1/2 z-10 grid max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-[34rem] -translate-x-1/2 -translate-y-1/2 gap-3.5 overflow-y-auto rounded-xl border border-[#bfcfe7] bg-white p-5 shadow-[0_18px_42px_rgba(27,50,83,0.18)]"
      aria-labelledby="intake-review-heading"
      role="dialog"
      aria-modal={nestedOpen ? undefined : true}
      inert={nestedOpen}
    >
      <p className="mb-0 text-[0.67rem] font-bold tracking-[0.1em] text-[#68748a] uppercase">REQUIRED BEFORE IMPORT</p>
      <h2 className="-mt-1.5 mb-0 text-[1.15rem] text-[#1e2d48]" id="intake-review-heading">
        Review {paths.length} video{paths.length === 1 ? "" : "s"}
      </h2>
      <p className="-mt-1 mb-0 text-[0.78rem] leading-snug text-[#617086]">
        These choices apply before the original files are added to the saved
        queue. Each video uploads from its current location; keep it available
        and unchanged until upload or recovery completes.
      </p>
      <fieldset className="grid gap-2 rounded-lg border border-[#dce3ed] p-3">
        <legend className="px-1 text-[0.78rem] font-bold text-[#344a67]">Made for Kids</legend>
        <label className="flex items-center gap-2 text-[0.78rem] text-[#4e5f78]">
          <input className="accent-[#2463df]"
            checked={madeForKids === true}
            name="made-for-kids"
            onChange={() => setMadeForKids(true)}
            type="radio"
          />{" "}
          Yes, made for kids
        </label>
        <label className="flex items-center gap-2 text-[0.78rem] text-[#4e5f78]">
          <input className="accent-[#2463df]"
            checked={madeForKids === false}
            name="made-for-kids"
            onChange={() => setMadeForKids(false)}
            type="radio"
          />{" "}
          No, not made for kids
        </label>
      </fieldset>
      <label className="grid gap-1 text-[0.78rem] font-bold text-[#344a67]">
        Visibility
        <select className="rounded-md border border-[#ccd6e4] bg-white px-2.5 py-2 text-[#2d3f5d] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
          disabled={publicationCount > 0}
          onChange={(event) =>
            { visibilityChosen.current = true; setVisibility(event.target.value as UploadVisibility); }
          }
          value={visibility}
        >
          <option value="private">Private</option>
          <option value="unlisted">Unlisted</option>
          <option value="public">Public</option>
        </select>
      </label>
      {publicationCount > 0 ? <p className="m-0 text-sm leading-snug text-muted" role="status">Private visibility is required for {publicationCount} scheduled video{publicationCount === 1 ? "" : "s"}. YouTube publication is requested separately after upload processing succeeds.</p> : null}
      <div className="grid gap-2">
      <label className="grid gap-1 text-[0.78rem] font-bold text-[#344a67]">
        Add to playlist
        <select className="rounded-md border border-[#ccd6e4] bg-white px-2.5 py-2 text-[#2d3f5d] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
          onChange={(event) => { playlistChosen.current = true; setPlaylistId(event.target.value); setPlaylistNotice(""); setDefaultPlaylistNotice(""); }}
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
      <div className="flex flex-wrap gap-2">
      <button
        className="inline-flex min-h-10 items-center justify-center gap-2 justify-self-start rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50"
        disabled={!activeChannelId}
        onClick={() => setCreateOpen(true)}
        ref={createButton}
        type="button"
      ><Plus aria-hidden="true" size={16} /> Create playlist</button>
      <button
        className="inline-flex min-h-10 items-center justify-center gap-2 rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50"
        disabled={!activeChannelId || loadingPlaylists}
        onClick={() => setPlaylistRefresh((current) => current + 1)}
        type="button"
      ><RefreshCw aria-hidden="true" className={loadingPlaylists ? "animate-spin motion-reduce:animate-none" : undefined} size={16} />{loadingPlaylists ? "Loading playlists…" : "Refresh playlists"}</button>
      </div>
      {playlistNotice ? <p className="m-0 text-sm leading-snug text-success" role="status">{playlistNotice}</p> : null}
      {defaultPlaylistNotice ? <p className="m-0 text-sm leading-snug text-muted" role="status">{defaultPlaylistNotice}</p> : null}
      </div>
      <label className="flex items-start gap-1.5 text-[0.78rem] leading-snug font-semibold text-[#344a67]">
        <input className="mt-0.5 shrink-0 accent-[#2463df]"
          checked={deleteSourceAfterUpload}
          onChange={(event) => setDeleteSourceAfterUpload(event.target.checked)}
          type="checkbox"
        />{" "}
        Delete each original source file only after YouTube processing succeeds.
        No app-managed copy is created.
      </label>
      {playlistError && <p className="m-0 text-[0.72rem] text-[#a4413b]">{playlistError}</p>}
      {defaultsError && <p className="m-0 text-sm text-danger" role="alert">{defaultsError}</p>}
      {loadingDefaults && <p className="m-0 text-sm text-muted" role="status">Loading saved upload defaults…</p>}
      <div className="grid gap-2 rounded-lg border border-line bg-subtle-surface p-3">
        <button className="min-h-11 rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:opacity-50" disabled={!activeChannelId} onClick={() => setMetadataOpen(true)} type="button">Metadata, presets and manifest</button>
        <p className="m-0 text-sm text-muted">{metadata || itemMetadata ? "Reviewed metadata is saved for this batch. You can review or edit it before importing." : "Titles come from filenames. Add descriptions, tags, language, presets or a local CSV/JSON manifest."}</p>
      </div>
      <div className="grid gap-2 rounded-lg border border-line bg-subtle-surface p-3">
        <button className="inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:opacity-50" disabled={!activeChannelId || !paths.length} onClick={() => setPublicationOpen(true)} type="button"><CalendarClock aria-hidden="true" size={16} />Publication calendar</button>
        <p className="m-0 text-sm text-muted">{publicationCount ? "UTC publication times are retained for this draft. Review the timezone and each video’s date before importing." : "Choose a timezone and per-video publication dates in three steps. Scheduled videos upload privately."}</p>
      </div>
      <footer className="flex flex-wrap justify-end gap-2">
        <button className="rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] hover:bg-[#f0f3f7]" onClick={onCancel} type="button">
          Cancel drop
        </button>
        <button
          className="rounded-md border border-[#2463df] bg-[#2463df] px-3 py-2 text-[0.79rem] font-[680] text-white hover:border-[#1b54c6] hover:bg-[#1b54c6] disabled:cursor-not-allowed disabled:opacity-50"
          disabled={madeForKids === undefined || loadingDefaults || loadingPlaylists}
          onClick={() =>
            onConfirm({
              madeForKids: madeForKids!,
              visibility,
              playlistId: selectedPlaylist?.id,
              playlistTitle: selectedPlaylist?.title,
              deleteSourceAfterUpload,
              metadata,
              itemMetadata,
              publicationContexts,
              expectedChannelId: activeChannelId,
            })
          }
          type="button"
        >
          Import reviewed videos
        </button>
      </footer>
    </section>
    {createOpen ? <Suspense fallback={<p className="fixed bottom-4 left-1/2 z-20 -translate-x-1/2 rounded-md border border-line bg-surface px-4 py-3 text-sm text-ink shadow-lg" role="status">Opening playlist creation…</p>}><PlaylistCreateDialog expectedChannelId={activeChannelId} onClose={() => setCreateOpen(false)} onCreated={playlistCreated} /></Suspense> : null}
    {metadataOpen && activeChannelId ? <Suspense fallback={<p className="fixed bottom-4 left-1/2 z-20 -translate-x-1/2 rounded-md border border-line bg-surface px-4 py-3 text-sm text-ink shadow-lg" role="status">Opening metadata review…</p>}><UploadMetadataDialog expectedChannelId={activeChannelId} paths={paths} initialMetadata={metadata} initialItemMetadata={itemMetadata} onApply={(choices) => { const nextTimes = intakePublicationTimes(paths, choices.metadata, choices.itemMetadata); setPublicationContexts((previous) => retainIntakePublicationContexts(publicationTimes, nextTimes, previous)); setMetadata(choices.metadata); setItemMetadata(choices.itemMetadata); if (Object.keys(nextTimes).length) { visibilityChosen.current = true; setVisibility("private"); } setMetadataOpen(false); }} onClose={() => setMetadataOpen(false)} /></Suspense> : null}
    {publicationOpen && activeChannelId ? <Suspense fallback={<p className="fixed bottom-4 left-1/2 z-20 -translate-x-1/2 rounded-md border border-line bg-surface px-4 py-3 text-sm text-ink shadow-lg" role="status">Opening publication calendar…</p>}><PublicationCalendarDialog expectedChannelId={activeChannelId} paths={paths} initialTimesByPath={publicationTimes} initialTimeZone={paths.map((path) => publicationContexts?.[path]?.timeZone).find(Boolean)} onApply={(result) => { setItemMetadata(applyIntakePublicationTimes(paths, result.publishAtByPath, metadata, itemMetadata)); setPublicationContexts(Object.fromEntries(paths.map((path) => [path, { timeZone: result.timeZone, tzdbVersion: result.tzdbVersion }]))); visibilityChosen.current = true; setVisibility("private"); setPublicationOpen(false); }} onClose={() => setPublicationOpen(false)} /></Suspense> : null}
    </>
  );
}
