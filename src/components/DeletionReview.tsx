import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  beginDeletionAuthorization,
  cancelDeletionRequest,
  clearDeletionRequests,
  disableDeletionSudoMode,
  enableDeletionSudoMode,
  executeDeletionRequest,
  isTauri,
  listDeletionRequests,
  listRemoteVideos,
  loadConnectionSettings,
  requestVideoDeletion,
} from "../lib/local";
import { windowItems } from "../lib/list-windowing";
import { useRetainedWorkspaceState } from "../lib/retained-workspace-state";
import type { DeletionRequest, RemoteVideo } from "../lib/types";
import { PaginationControls } from "./PaginationControls";
import { subscribeLocalStateChanges } from "../lib/state-events";

type DeletionReviewProps = {
  activeChannel?: string;
  activeChannelId?: string;
  busy?: boolean;
  initialRequests?: DeletionRequest[];
  initialVideos?: RemoteVideo[];
  onNotice: (notice: string) => void;
  refreshVersion?: number;
};
const isPending = (request: DeletionRequest) => request.status === "pending" || request.status === "needs_reconciliation";

export function DeletionReview({ activeChannel, activeChannelId, busy = false, initialRequests = [], initialVideos = [], onNotice, refreshVersion = 0 }: DeletionReviewProps) {
  const [videos, setVideos] = useState<RemoteVideo[]>(initialVideos);
  const [requests, setRequests] = useState<DeletionRequest[]>(initialRequests);
  const [selected, setSelected] = useState<RemoteVideo>();
  const [reviewQueue, setReviewQueue] = useState<RemoteVideo[]>([]);
  const [selectedVideoIds, setSelectedVideoIds] =
    useRetainedWorkspaceState<Set<string>>(
      "deletion.selected-video-ids",
      () => new Set(),
    );
  const [confirmation, setConfirmation] = useState("");
  const [executionRequest, setExecutionRequest] = useState<DeletionRequest>();
  const [executionConfirmation, setExecutionConfirmation] = useState("");
  const [deletionAuthorized, setDeletionAuthorized] = useState(false);
  const [deletionSudoActive, setDeletionSudoActive] = useState(false);
  const [deletionSudoExpiresAt, setDeletionSudoExpiresAt] = useState<string>();
  const [authorizing, setAuthorizing] = useRetainedWorkspaceState(
    "deletion.authorizing",
    false,
  );
  const [loading, setLoading] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [inventorySearch, setInventorySearch] = useRetainedWorkspaceState(
    "deletion.inventory-search",
    "",
  );
  const [requestSearch, setRequestSearch] = useRetainedWorkspaceState(
    "deletion.request-search",
    "",
  );
  const [inventoryPage, setInventoryPage] = useRetainedWorkspaceState(
    "deletion.inventory-page",
    1,
  );
  const [requestPage, setRequestPage] = useRetainedWorkspaceState(
    "deletion.request-page",
    1,
  );
  const deferredInventorySearch = useDeferredValue(inventorySearch);
  const deferredRequestSearch = useDeferredValue(requestSearch);

  const refresh = useCallback(async () => {
    if (!isTauri || !activeChannel) {
      setVideos([]); setRequests([]); setDeletionAuthorized(false); setDeletionSudoActive(false); setDeletionSudoExpiresAt(undefined); return;
    }
    setLoading(true);
    try {
      const [nextVideos, nextRequests, settings] = await Promise.all([listRemoteVideos(), listDeletionRequests(), loadConnectionSettings()]);
      setVideos(nextVideos); setRequests(nextRequests); setDeletionAuthorized(settings.deletionAuthorized === true); setDeletionSudoActive(settings.deletionSudoActive === true); setDeletionSudoExpiresAt(settings.deletionSudoExpiresAt);
      if (settings.deletionAuthorized) setAuthorizing(false);
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "Saved deletion review data could not be loaded.");
    } finally { setLoading(false); }
  }, [activeChannel, onNotice, setAuthorizing]);

  useEffect(() => { void refresh(); }, [refresh, refreshVersion]);
  useEffect(() => {
    if (!isTauri || !activeChannelId) return;
    let active = true;
    let unsubscribe: (() => void) | undefined;
    void subscribeLocalStateChanges((batch) => {
      if (
        batch.changes.some(
          (change) =>
            change.channelId === activeChannelId &&
            ["connection", "deletion", "inventory"].includes(change.surface),
        )
      )
        void refresh();
    })
      .then((stop) => {
        if (active) unsubscribe = stop;
        else stop();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [activeChannelId, refresh]);

  useEffect(() => {
    if (!authorizing || !isTauri) return;
    let active = true;
    let timeout: number | undefined;
    const delays = [250, 500, 1_000, 2_000, 4_000, 8_000, 16_000];
    const checkAuthorization = async (attempt: number) => {
      try {
        const settings = await loadConnectionSettings();
        if (!active) return;
        if (settings.deletionAuthorized) {
          setDeletionAuthorized(true);
          setDeletionSudoActive(settings.deletionSudoActive === true);
          setDeletionSudoExpiresAt(settings.deletionSudoExpiresAt);
          setAuthorizing(false);
          onNotice("YouTube deletion permission is active for this device. Each video still requires its exact ID before permanent deletion.");
          return;
        }
      } catch { /* Event delivery remains primary; this fallback is intentionally bounded. */ }
      if (active && attempt + 1 < delays.length)
        timeout = window.setTimeout(
          () => void checkAuthorization(attempt + 1),
          delays[attempt + 1],
        );
    };
    timeout = window.setTimeout(() => void checkAuthorization(0), delays[0]);
    return () => {
      active = false;
      if (timeout !== undefined) window.clearTimeout(timeout);
    };
  }, [authorizing, onNotice, setAuthorizing]);

  const closeRequestConfirmation = () => { setSelected(undefined); setReviewQueue([]); setConfirmation(""); };
  const closeExecutionConfirmation = () => { setExecutionRequest(undefined); setExecutionConfirmation(""); };
  const createRequest = async () => {
    if (!selected || confirmation !== selected.videoId) return;
    setLoading(true);
    try {
      const request = await requestVideoDeletion(selected.videoId, confirmation);
      setRequests((current) => [request, ...current.filter((candidate) => candidate.videoId !== request.videoId)]);
      setSelectedVideoIds((current) => {
        const next = new Set(current);
        next.delete(request.videoId);
        return next;
      });
      const [nextVideo, ...remaining] = reviewQueue;
      if (nextVideo) {
        setSelected(nextVideo);
        setReviewQueue(remaining);
        setConfirmation("");
        onNotice(`A local deletion request for ${request.videoId} was recorded. Review the next selected video; no YouTube video has been deleted.`);
      } else {
        closeRequestConfirmation();
        onNotice(`A local deletion request for ${request.videoId} was recorded. It has not deleted the YouTube video.`);
      }
    } catch (error) { onNotice(error instanceof Error ? error.message : "The deletion request could not be recorded."); }
    finally { setLoading(false); }
  };
  const cancelRequest = async (request: DeletionRequest) => {
    setLoading(true);
    try {
      await cancelDeletionRequest(request.id);
      setRequests((current) => current.map((candidate) => candidate.id === request.id ? { ...candidate, status: "cancelled", detail: "Cancelled locally before execution." } : candidate));
      closeExecutionConfirmation();
      onNotice(`Deletion request for ${request.videoId} was cancelled locally. The YouTube video remains unchanged.`);
    } catch (error) { onNotice(error instanceof Error ? error.message : "The deletion request could not be cancelled."); }
    finally { setLoading(false); }
  };
  const authorizeDeletion = async () => {
    if (!isTauri) return;
    setAuthorizing(true);
    try {
      const { authorizationUrl } = await beginDeletionAuthorization();
      const url = new URL(authorizationUrl);
      if (url.protocol !== "https:") throw new Error("The deletion authorization request must use HTTPS.");
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url.toString());
      onNotice("Google opened in your browser to grant deletion permission. After consent, temporary deletion mode will be enabled for 15 minutes.");
    } catch (error) {
      setAuthorizing(false);
      onNotice(error instanceof Error ? error.message : "Deletion authorization could not be started.");
    }
  };
  const clearRequests = async () => {
    setLoading(true);
    try {
      const cleared = await clearDeletionRequests();
      setRequests((current) => current.map((request) => isPending(request) ? { ...request, status: "cancelled", detail: "Cleared locally. The YouTube video remains unchanged." } : request));
      onNotice(`${cleared} local deletion request${cleared === 1 ? " was" : "s were"} cleared. No YouTube video was changed.`);
    } catch (error) { onNotice(error instanceof Error ? error.message : "The deletion request queue could not be cleared."); }
    finally { setLoading(false); }
  };
  const setDeletionMode = async (enabled: boolean) => {
    setLoading(true);
    try {
      const settings = enabled ? await enableDeletionSudoMode() : await disableDeletionSudoMode();
      setDeletionSudoActive(settings.deletionSudoActive === true);
      setDeletionSudoExpiresAt(settings.deletionSudoExpiresAt);
      onNotice(enabled ? "Temporary deletion mode is active for 15 minutes. Each video still requires confirmation." : "Temporary deletion mode ended. No further videos can be deleted until it is enabled again.");
    } catch (error) { onNotice(error instanceof Error ? error.message : "Deletion mode could not be updated."); }
    finally { setLoading(false); }
  };
  const executeRequest = async () => {
    if (!executionRequest || executionConfirmation !== executionRequest.videoId) return;
    setExecuting(true);
    try {
      const completed = await executeDeletionRequest(executionRequest.id, executionConfirmation);
      setRequests((current) => current.map((candidate) => candidate.id === completed.id ? completed : candidate));
      setVideos((current) => current.filter((video) => video.videoId !== completed.videoId));
      closeExecutionConfirmation();
      onNotice(`YouTube confirmed permanent deletion of ${completed.videoId}. The local execution receipt was saved.`);
    } catch (error) { onNotice(error instanceof Error ? error.message : "YouTube did not confirm the deletion; the saved request will be reconciled rather than restarted."); }
    finally { setExecuting(false); }
  };

  const pendingRequests = useMemo(() => requests.filter(isPending), [requests]);
  const filteredPendingRequests = useMemo(() => {
    const query = deferredRequestSearch.trim().toLocaleLowerCase();
    return query
      ? pendingRequests.filter((request) =>
          request.title.toLocaleLowerCase().includes(query),
        )
      : pendingRequests;
  }, [deferredRequestSearch, pendingRequests]);
  const requestedVideoIds = useMemo(
    () => new Set(pendingRequests.map((request) => request.videoId)),
    [pendingRequests],
  );
  const filteredVideos = useMemo(() => {
    const query = deferredInventorySearch.trim().toLocaleLowerCase();
    return query
      ? videos.filter((video) => video.title.toLocaleLowerCase().includes(query))
      : videos;
  }, [deferredInventorySearch, videos]);
  const filteredSelectableVideos = useMemo(
    () =>
      filteredVideos.filter((video) => !requestedVideoIds.has(video.videoId)),
    [filteredVideos, requestedVideoIds],
  );
  const visibleRequests = useMemo(
    () => windowItems(filteredPendingRequests, requestPage),
    [filteredPendingRequests, requestPage],
  );
  const visibleVideos = useMemo(
    () => windowItems(filteredVideos, inventoryPage),
    [filteredVideos, inventoryPage],
  );
  const selectedFilteredVideoCount = filteredSelectableVideos.filter((video) => selectedVideoIds.has(video.videoId)).length;
  const allSelectableSelected = filteredSelectableVideos.length > 0 && filteredSelectableVideos.every((video) => selectedVideoIds.has(video.videoId));
  const disabled = busy || loading || authorizing;
  const toggleVideo = (videoId: string) => {
    setSelectedVideoIds((current) => {
      const next = new Set(current);
      if (next.has(videoId)) next.delete(videoId); else next.add(videoId);
      return next;
    });
  };
  const toggleSelectAll = () => {
    setSelectedVideoIds((current) => {
      const next = new Set(current);
      for (const video of filteredSelectableVideos) {
        if (allSelectableSelected) next.delete(video.videoId); else next.add(video.videoId);
      }
      return next;
    });
  };
  const reviewSelected = () => {
    const selectedVideos = filteredSelectableVideos.filter((video) => selectedVideoIds.has(video.videoId));
    const [first, ...remaining] = selectedVideos;
    if (!first) return;
    setSelected(first);
    setReviewQueue(remaining);
    setConfirmation("");
  };
  if (!activeChannel) return <p className="m-0 text-[0.78rem] leading-[1.45] text-[#69768a]">Connect and sync a channel before reviewing locally saved YouTube videos.</p>;

  return <div className="grid gap-3.5">
    <header className="grid gap-1 rounded-lg border border-[#f3e2a9] bg-[#fffbeb] px-3.5 py-3 text-[#755c18]">
      <strong className="text-[0.82rem] text-[#614b12]">Permanent deletion is never automatic.</strong>
      <span className="text-[0.78rem] leading-[1.45] text-[#69768a]">
        Every selected video gets a local review request. Permanent execution
        requires an active temporary deletion mode and a typed-ID confirmation.
      </span>
    </header>
    <section className="flex items-center justify-between gap-4 rounded-[0.55rem] border border-[#e0e5eb] bg-[#fafbfc] p-3.5 max-compact:flex-col max-compact:items-stretch" aria-labelledby="deletion-authorization-heading">
      <div>
        <p className="mb-[0.45rem] text-[0.67rem] leading-[1.2] font-bold tracking-[0.1em] text-[#68748a] uppercase">TEMPORARY DELETION MODE</p>
        <h3 className="my-[0.12rem] text-[0.88rem] text-[#2b3850]" id="deletion-authorization-heading">YouTube deletion authority</h3>
        <p className="mt-[0.3rem] mb-0 text-[0.75rem] leading-[1.45] text-[#677489]">
          {!deletionAuthorized
            ? "Grant deletion permission once for this device. Creating and cancelling local requests never asks for this authority."
            : deletionSudoActive
              ? `Deletion mode is active${deletionSudoExpiresAt ? ` until ${new Date(deletionSudoExpiresAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : ""}. Every video still requires confirmation.`
              : "Deletion permission is granted, but deletion mode is off. Enable it to delete confirmed videos for 15 minutes."}
        </p>
      </div>
      {!deletionAuthorized ? <button className="rounded-md bg-[#a4413b] px-3 py-2 text-sm font-semibold text-white transition-colors hover:bg-[#86342f] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled} onClick={() => void authorizeDeletion()} type="button">{authorizing ? "Waiting for Google…" : "Grant deletion permission"}</button> : deletionSudoActive ? <button className="rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-sm font-semibold text-[#34405a] transition-colors hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled} onClick={() => void setDeletionMode(false)} type="button">Exit deletion mode</button> : <button className="rounded-md bg-[#a4413b] px-3 py-2 text-sm font-semibold text-white transition-colors hover:bg-[#86342f] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled} onClick={() => void setDeletionMode(true)} type="button">Enter deletion mode (15 min)</button>}
    </section>
    {pendingRequests.length > 0 && (
      <section
        aria-busy={requestSearch !== deferredRequestSearch}
        aria-labelledby="deletion-requests-heading"
        className="rounded-[0.55rem] border border-[#e0e5eb] bg-[#fafbfc] p-3.5"
      >
        <header>
          <p className="mb-[0.45rem] text-[0.67rem] leading-[1.2] font-bold tracking-[0.1em] text-[#68748a] uppercase">LOCAL REVIEW QUEUE</p>
          <h3 className="my-[0.12rem] text-[0.88rem] text-[#2b3850]" id="deletion-requests-heading">Pending and recoverable requests</h3>
          <button className="text-sm font-semibold text-[#315389] underline decoration-[#315389]/35 underline-offset-2 transition-colors hover:text-[#1b54c6] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled} onClick={() => void clearRequests()} type="button">Clear deletion requests</button>
        </header>
        <label className="mt-1 grid gap-1.5 text-[0.74rem] font-bold text-[#465775]">
          Search recoverable deletion requests by title
          <input
            aria-label="Search recoverable deletion requests by title"
            autoComplete="off"
            onChange={(event) => {
              setRequestSearch(event.target.value);
              setRequestPage(1);
            }}
            placeholder="Search request titles"
            className="w-full max-w-[32rem] rounded-md border border-[#cbd5e3] bg-white px-2.5 py-2 text-inherit font-medium text-[#27344a] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15"
            type="search"
            value={requestSearch}
          />
        </label>
        {filteredPendingRequests.length === 0 && <p className="m-0 text-[0.78rem] leading-[1.45] text-[#69768a]">No recoverable deletion request titles match “{requestSearch}”.</p>}
        <ul className="mt-3 grid list-none gap-2.5 p-0">
          {visibleRequests.items.map((request) => <li className="flex items-center justify-between gap-4 border-t border-[#e3e7ed] pt-2.5 max-compact:flex-col max-compact:items-stretch" data-deletion-request-record key={request.id}>
            <div>
              <strong className="block">{request.title}</strong>
              <span className="mt-[0.17rem] block text-[0.72rem] text-[#758196]">{request.videoId}</span>
              <p className="mt-[0.17rem] mb-0 text-[0.72rem] text-[#758196]">{request.detail}</p>
            </div>
            <div className="flex flex-wrap justify-end gap-2 max-compact:flex-col max-compact:items-stretch">
              {deletionSudoActive && <button className="rounded-md bg-[#a4413b] px-3 py-2 text-sm font-semibold text-white transition-colors hover:bg-[#86342f] disabled:cursor-not-allowed disabled:opacity-55 max-compact:w-full" disabled={disabled} onClick={() => { setExecutionRequest(request); setExecutionConfirmation(""); }} type="button">{request.status === "needs_reconciliation" ? "Reconcile deletion" : "Execute permanent deletion"}</button>}
              <button className="text-sm font-semibold text-[#315389] underline decoration-[#315389]/35 underline-offset-2 transition-colors hover:text-[#1b54c6] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled} onClick={() => void cancelRequest(request)} type="button">Cancel request</button>
            </div>
          </li>)}
        </ul>
        <PaginationControls
          end={visibleRequests.end}
          label="Deletion requests"
          onPageChange={setRequestPage}
          page={visibleRequests.page}
          pageCount={visibleRequests.pageCount}
          start={visibleRequests.start}
          total={visibleRequests.total}
        />
      </section>
    )}
    {selected && <section className="rounded-[0.55rem] border border-[#e0e5eb] bg-[#fafbfc] p-3.5" aria-labelledby="deletion-confirmation-heading"><p className="mb-[0.45rem] text-[0.67rem] leading-[1.2] font-bold tracking-[0.1em] text-[#68748a] uppercase">REQUEST CONFIRMATION</p><h3 className="my-[0.12rem] text-[0.88rem] text-[#2b3850]" id="deletion-confirmation-heading">Request deletion for “{selected.title}”</h3><p className="mt-[0.3rem] mb-0 text-[0.75rem] leading-[1.45] text-[#677489]">{reviewQueue.length > 0 ? `${reviewQueue.length + 1} selected videos remain in this review queue. Confirm each video ID separately.` : "This creates a local pending request only. Type the full video ID below to prove this is the intended video."}</p><code className="my-2.5 block w-fit rounded-md border border-[#dce2ea] bg-[#f0f3f7] px-2 py-1.5 font-mono text-[0.72rem] text-[#355b9d]">{selected.videoId}</code><label className="mt-2.5 mb-1.5 block text-[0.74rem] font-bold text-[#3e4b62]" htmlFor="delete-video-confirmation">Video ID confirmation</label><input className="w-full max-w-[30rem] rounded-md border border-[#cbd3df] bg-white px-2.5 py-2 text-[#172033] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15" id="delete-video-confirmation" autoComplete="off" autoFocus value={confirmation} onChange={(event) => setConfirmation(event.target.value)} placeholder={selected.videoId} spellCheck={false} /><div className="mt-3 flex flex-wrap justify-start gap-2"><button className="rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-sm font-semibold text-[#34405a] transition-colors hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled} onClick={closeRequestConfirmation} type="button">Cancel selected review</button><button className="rounded-md bg-[#a4413b] px-3 py-2 text-sm font-semibold text-white transition-colors hover:bg-[#86342f] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled || confirmation !== selected.videoId} onClick={() => void createRequest()} type="button">Create local request{reviewQueue.length > 0 ? " and continue" : ""}</button></div></section>}
    {executionRequest && <section aria-busy={executing} className="rounded-[0.55rem] border border-[#edcbc8] bg-[#fff7f6] p-3.5" aria-labelledby="execution-confirmation-heading"><p className="mb-[0.45rem] text-[0.67rem] leading-[1.2] font-bold tracking-[0.1em] text-[#68748a] uppercase">PERMANENT, IRREVERSIBLE ACTION</p><h3 className="my-[0.12rem] text-[0.88rem] text-[#2b3850]" id="execution-confirmation-heading">Delete “{executionRequest.title}” from YouTube</h3><p className="mt-[0.3rem] mb-0 text-[0.75rem] leading-[1.45] text-[#677489]">{executing ? "Deleting on YouTube. You can keep reviewing the local library while this request completes." : "This calls YouTube’s delete endpoint. The video cannot be restored by this app. Type the exact video ID a second time to execute this permanent deletion."}</p><code className="my-2.5 block w-fit rounded-md border border-[#dce2ea] bg-[#f0f3f7] px-2 py-1.5 font-mono text-[0.72rem] text-[#355b9d]">{executionRequest.videoId}</code><label className="mt-2.5 mb-1.5 block text-[0.74rem] font-bold text-[#3e4b62]" htmlFor="execute-delete-video-confirmation">Exact video ID for permanent deletion</label><input className="w-full max-w-[30rem] rounded-md border border-[#cbd3df] bg-white px-2.5 py-2 text-[#172033] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15" id="execute-delete-video-confirmation" autoComplete="off" autoFocus disabled={executing} value={executionConfirmation} onChange={(event) => setExecutionConfirmation(event.target.value)} placeholder={executionRequest.videoId} spellCheck={false} /><div className="mt-3 flex flex-wrap justify-start gap-2"><button className="rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-sm font-semibold text-[#34405a] transition-colors hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled || executing} onClick={closeExecutionConfirmation} type="button">Keep video</button><button className="rounded-md bg-[#a4413b] px-3 py-2 text-sm font-semibold text-white transition-colors hover:bg-[#86342f] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled || executing || executionConfirmation !== executionRequest.videoId} onClick={() => void executeRequest()} type="button">{executing ? "Deleting…" : "Delete permanently from YouTube"}</button></div></section>}
    <div
      aria-busy={loading || inventorySearch !== deferredInventorySearch}
       className="grid gap-2"
    >
       {videos.length === 0 && <p className="m-0 text-[0.78rem] leading-[1.45] text-[#69768a]">{loading ? "Loading locally saved inventory…" : "Sync the YouTube library to review videos."}</p>}
       {videos.length > 0 && <label className="mt-1 grid gap-1.5 text-[0.74rem] font-bold text-[#465775]">Search saved YouTube videos by title<input className="w-full max-w-[32rem] rounded-md border border-[#cbd5e3] bg-white px-2.5 py-2 text-inherit font-medium text-[#27344a] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15" aria-label="Search saved YouTube videos by title" autoComplete="off" onChange={(event) => { setInventorySearch(event.target.value); setInventoryPage(1); }} placeholder="Search video titles" type="search" value={inventorySearch} /></label>}
       {filteredSelectableVideos.length > 0 && <div className="flex flex-wrap items-center justify-between gap-3.5 rounded-[0.55rem] border border-[#d8e4f7] bg-[#f2f6fd] px-3 py-2.5"><label className="inline-flex cursor-pointer items-center gap-2 text-[0.75rem] font-semibold text-[#3c5274]"><input className="m-0 size-4 accent-[#2463df]" checked={allSelectableSelected} disabled={disabled} onChange={toggleSelectAll} type="checkbox" /> Select all filtered videos (all pages)</label><span className="ml-auto text-[0.72rem] text-[#63728a]">{selectedFilteredVideoCount} selected</span><button className="rounded-md bg-[#a4413b] px-3 py-2 text-sm font-semibold text-white transition-colors hover:bg-[#86342f] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled || selectedFilteredVideoCount === 0} onClick={reviewSelected} type="button">Delete selected ({selectedFilteredVideoCount})</button></div>}
       {videos.length > 0 && filteredVideos.length === 0 && <p className="m-0 text-[0.78rem] leading-[1.45] text-[#69768a]">No saved YouTube video titles match “{inventorySearch}”.</p>}
       {visibleVideos.items.map((video) => <article className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-4 rounded-[0.55rem] border border-[#e0e5eb] bg-[#fafbfc] p-3.5 max-compact:grid-cols-1 max-compact:items-stretch" data-deletion-inventory-record key={video.videoId}><label className="self-stretch inline-flex cursor-pointer items-center gap-2 border-r border-[#e2e7ee] pr-3 text-[0.75rem] font-semibold text-[#3c5274] max-compact:border-r-0 max-compact:border-b max-compact:pb-3" aria-label={`Select ${video.title} for deletion review`}><input className="m-0 size-4 accent-[#2463df] disabled:cursor-not-allowed" checked={selectedVideoIds.has(video.videoId)} disabled={disabled || requestedVideoIds.has(video.videoId)} onChange={() => toggleVideo(video.videoId)} type="checkbox" /></label><div className="min-w-0"><span className="text-[0.64rem] font-bold tracking-[0.07em] text-[#5271a3] uppercase">YouTube video</span><h3 className="my-[0.12rem] wrap-anywhere text-[0.88rem] text-[#2b3850]">{video.title}</h3><p className="mt-[0.15rem] mb-0 wrap-anywhere text-[0.73rem] text-[#718095]">ID: <code className="font-mono text-[0.72rem] text-[#355b9d]">{video.videoId}</code>{video.privacyStatus ? ` · ${video.privacyStatus}` : ""}{video.duration ? ` · ${video.duration}` : ""}</p></div><button className="rounded-md bg-[#a4413b] px-3 py-2 text-sm font-semibold text-white transition-colors hover:bg-[#86342f] disabled:cursor-not-allowed disabled:opacity-55" disabled={disabled || requestedVideoIds.has(video.videoId)} onClick={() => { setReviewQueue([]); setSelected(video); setConfirmation(""); }} type="button">{requestedVideoIds.has(video.videoId) ? "Request pending" : "Delete"}</button></article>)}
      <PaginationControls
        end={visibleVideos.end}
        label="Saved YouTube videos"
        onPageChange={setInventoryPage}
        page={visibleVideos.page}
        pageCount={visibleVideos.pageCount}
        start={visibleVideos.start}
        total={visibleVideos.total}
      />
    </div>
  </div>;
}
