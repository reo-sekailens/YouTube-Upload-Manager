import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
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
import type { DeletionRequest, RemoteVideo } from "../lib/types";

type DeletionReviewProps = { activeChannel?: string; busy?: boolean; onNotice: (notice: string) => void };
const isPending = (request: DeletionRequest) => request.status === "pending" || request.status === "needs_reconciliation";

export function DeletionReview({ activeChannel, busy = false, onNotice }: DeletionReviewProps) {
  const [videos, setVideos] = useState<RemoteVideo[]>([]);
  const [requests, setRequests] = useState<DeletionRequest[]>([]);
  const [selected, setSelected] = useState<RemoteVideo>();
  const [reviewQueue, setReviewQueue] = useState<RemoteVideo[]>([]);
  const [selectedVideoIds, setSelectedVideoIds] = useState<Set<string>>(new Set());
  const [confirmation, setConfirmation] = useState("");
  const [executionRequest, setExecutionRequest] = useState<DeletionRequest>();
  const [executionConfirmation, setExecutionConfirmation] = useState("");
  const [deletionAuthorized, setDeletionAuthorized] = useState(false);
  const [deletionSudoActive, setDeletionSudoActive] = useState(false);
  const [deletionSudoExpiresAt, setDeletionSudoExpiresAt] = useState<string>();
  const [authorizing, setAuthorizing] = useState(false);
  const [loading, setLoading] = useState(false);
  const [inventorySearch, setInventorySearch] = useState("");
  const [requestSearch, setRequestSearch] = useState("");

  const refresh = useCallback(async () => {
    if (!isTauri || !activeChannel) {
      setVideos([]); setRequests([]); setDeletionAuthorized(false); setDeletionSudoActive(false); setDeletionSudoExpiresAt(undefined); return;
    }
    setLoading(true);
    try {
      const [nextVideos, nextRequests, settings] = await Promise.all([listRemoteVideos(), listDeletionRequests(), loadConnectionSettings()]);
      setVideos(nextVideos); setRequests(nextRequests); setDeletionAuthorized(settings.deletionAuthorized === true); setDeletionSudoActive(settings.deletionSudoActive === true); setDeletionSudoExpiresAt(settings.deletionSudoExpiresAt);
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "Saved deletion review data could not be loaded.");
    } finally { setLoading(false); }
  }, [activeChannel, onNotice]);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    if (!authorizing || !isTauri) return;
    let active = true;
    const checkAuthorization = async () => {
      try {
        const settings = await loadConnectionSettings();
        if (!active || !settings.deletionAuthorized) return;
        setDeletionAuthorized(true); setDeletionSudoActive(settings.deletionSudoActive === true); setDeletionSudoExpiresAt(settings.deletionSudoExpiresAt); setAuthorizing(false);
        onNotice("YouTube deletion permission is active and temporary deletion mode is enabled for this device. Each video still requires its exact ID before permanent deletion.");
      } catch { /* The authorization browser can still be open; avoid noisy polling errors. */ }
    };
    void checkAuthorization();
    const timer = window.setInterval(() => { void checkAuthorization(); }, 1_250);
    return () => { active = false; window.clearInterval(timer); };
  }, [authorizing, onNotice]);

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
    setLoading(true);
    try {
      const completed = await executeDeletionRequest(executionRequest.id, executionConfirmation);
      setRequests((current) => current.map((candidate) => candidate.id === completed.id ? completed : candidate));
      setVideos((current) => current.filter((video) => video.videoId !== completed.videoId));
      closeExecutionConfirmation();
      onNotice(`YouTube confirmed permanent deletion of ${completed.videoId}. The local execution receipt was saved.`);
    } catch (error) { onNotice(error instanceof Error ? error.message : "YouTube did not confirm the deletion; the saved request will be reconciled rather than restarted."); }
    finally { setLoading(false); }
  };

  const normalizedInventorySearch = inventorySearch.trim().toLocaleLowerCase();
  const normalizedRequestSearch = requestSearch.trim().toLocaleLowerCase();
  const pendingRequests = requests.filter(isPending);
  const filteredPendingRequests = pendingRequests.filter((request) => request.title.toLocaleLowerCase().includes(normalizedRequestSearch));
  const requestedVideoIds = new Set(pendingRequests.map((request) => request.videoId));
  const filteredVideos = videos.filter((video) => video.title.toLocaleLowerCase().includes(normalizedInventorySearch));
  const filteredSelectableVideos = filteredVideos.filter((video) => !requestedVideoIds.has(video.videoId));
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
  if (!activeChannel) return <p className="deletion-review__empty">Connect and sync a channel before reviewing locally saved YouTube videos.</p>;

  return <div className="deletion-review">
    <header className="deletion-review__warning">
      <strong>Permanent deletion is never automatic.</strong>
      <span>
        Every selected video gets a local review request. Permanent execution
        requires an active temporary deletion mode and a typed-ID confirmation.
      </span>
    </header>
    <section className="deletion-authorization" aria-labelledby="deletion-authorization-heading">
      <div>
        <p className="eyebrow">TEMPORARY DELETION MODE</p>
        <h3 id="deletion-authorization-heading">YouTube deletion authority</h3>
        <p>
          {!deletionAuthorized
            ? "Grant deletion permission once for this device. Creating and cancelling local requests never asks for this authority."
            : deletionSudoActive
              ? `Deletion mode is active${deletionSudoExpiresAt ? ` until ${new Date(deletionSudoExpiresAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : ""}. Every video still requires confirmation.`
              : "Deletion permission is granted, but deletion mode is off. Enable it to delete confirmed videos for 15 minutes."}
        </p>
      </div>
      {!deletionAuthorized ? <button className="danger-button" disabled={disabled} onClick={() => void authorizeDeletion()} type="button">{authorizing ? "Waiting for Google…" : "Grant deletion permission"}</button> : deletionSudoActive ? <button className="secondary-action" disabled={disabled} onClick={() => void setDeletionMode(false)} type="button">Exit deletion mode</button> : <button className="danger-button" disabled={disabled} onClick={() => void setDeletionMode(true)} type="button">Enter deletion mode (15 min)</button>}
    </section>
    {pendingRequests.length > 0 && <section className="deletion-requests" aria-labelledby="deletion-requests-heading">
      <header>
        <p className="eyebrow">LOCAL REVIEW QUEUE</p>
        <h3 id="deletion-requests-heading">Pending and recoverable requests</h3>
        <button className="text-button" disabled={disabled} onClick={() => void clearRequests()} type="button">Clear deletion requests</button>
      </header>
      <label>
        Search recoverable deletion requests by title
        <input aria-label="Search recoverable deletion requests by title" autoComplete="off" onChange={(event) => setRequestSearch(event.target.value)} placeholder="Search request titles" type="search" value={requestSearch} />
      </label>
      {filteredPendingRequests.length === 0 && <p className="deletion-review__empty">No recoverable deletion request titles match “{requestSearch}”.</p>}
      <ul>
        {filteredPendingRequests.map((request) => <li key={request.id}>
          <div>
            <strong>{request.title}</strong>
            <span>{request.videoId}</span>
            <p>{request.detail}</p>
          </div>
          <div className="deletion-request-actions">
            {deletionSudoActive && <button className="danger-button" disabled={disabled} onClick={() => { setExecutionRequest(request); setExecutionConfirmation(""); }} type="button">{request.status === "needs_reconciliation" ? "Reconcile deletion" : "Execute permanent deletion"}</button>}
            <button className="text-button" disabled={disabled} onClick={() => void cancelRequest(request)} type="button">Cancel request</button>
          </div>
        </li>)}
      </ul>
    </section>}
    {selected && <section className="deletion-confirmation" aria-labelledby="deletion-confirmation-heading"><p className="eyebrow">REQUEST CONFIRMATION</p><h3 id="deletion-confirmation-heading">Request deletion for “{selected.title}”</h3><p>{reviewQueue.length > 0 ? `${reviewQueue.length + 1} selected videos remain in this review queue. Confirm each video ID separately.` : "This creates a local pending request only. Type the full video ID below to prove this is the intended video."}</p><code>{selected.videoId}</code><label htmlFor="delete-video-confirmation">Video ID confirmation</label><input id="delete-video-confirmation" autoComplete="off" autoFocus value={confirmation} onChange={(event) => setConfirmation(event.target.value)} placeholder={selected.videoId} spellCheck={false} /><div className="deletion-confirmation__actions"><button className="secondary-action" disabled={disabled} onClick={closeRequestConfirmation} type="button">Cancel selected review</button><button className="danger-button" disabled={disabled || confirmation !== selected.videoId} onClick={() => void createRequest()} type="button">Create local request{reviewQueue.length > 0 ? " and continue" : ""}</button></div></section>}
    {executionRequest && <section className="deletion-confirmation deletion-confirmation--final" aria-labelledby="execution-confirmation-heading"><p className="eyebrow">PERMANENT, IRREVERSIBLE ACTION</p><h3 id="execution-confirmation-heading">Delete “{executionRequest.title}” from YouTube</h3><p>This calls YouTube’s delete endpoint. The video cannot be restored by this app. Type the exact video ID a second time to execute this permanent deletion.</p><code>{executionRequest.videoId}</code><label htmlFor="execute-delete-video-confirmation">Exact video ID for permanent deletion</label><input id="execute-delete-video-confirmation" autoComplete="off" autoFocus value={executionConfirmation} onChange={(event) => setExecutionConfirmation(event.target.value)} placeholder={executionRequest.videoId} spellCheck={false} /><div className="deletion-confirmation__actions"><button className="secondary-action" disabled={disabled} onClick={closeExecutionConfirmation} type="button">Keep video</button><button className="danger-button" disabled={disabled || executionConfirmation !== executionRequest.videoId} onClick={() => void executeRequest()} type="button">Delete permanently from YouTube</button></div></section>}
    <div className="remote-video-list" aria-busy={loading}>{videos.length === 0 && <p className="deletion-review__empty">{loading ? "Loading locally saved inventory…" : "Sync the YouTube library to review videos."}</p>}{videos.length > 0 && <label>Search saved YouTube videos by title<input aria-label="Search saved YouTube videos by title" autoComplete="off" onChange={(event) => setInventorySearch(event.target.value)} placeholder="Search video titles" type="search" value={inventorySearch} /></label>}{filteredSelectableVideos.length > 0 && <div className="deletion-selection-toolbar"><label><input checked={allSelectableSelected} disabled={disabled} onChange={toggleSelectAll} type="checkbox" /> Select all available videos</label><span>{selectedFilteredVideoCount} selected</span><button className="danger-button" disabled={disabled || selectedFilteredVideoCount === 0} onClick={reviewSelected} type="button">Review selected ({selectedFilteredVideoCount})</button></div>}{videos.length > 0 && filteredVideos.length === 0 && <p className="deletion-review__empty">No saved YouTube video titles match “{inventorySearch}”.</p>}{filteredVideos.map((video) => <article className="remote-video-card" key={video.videoId}><label className="remote-video-card__select" aria-label={`Select ${video.title} for deletion review`}><input checked={selectedVideoIds.has(video.videoId)} disabled={disabled || requestedVideoIds.has(video.videoId)} onChange={() => toggleVideo(video.videoId)} type="checkbox" /></label><div><span className="remote-video-card__scope">YouTube video</span><h3>{video.title}</h3><p>ID: <code>{video.videoId}</code>{video.privacyStatus ? ` · ${video.privacyStatus}` : ""}{video.duration ? ` · ${video.duration}` : ""}</p></div><button className="danger-button" disabled={disabled || requestedVideoIds.has(video.videoId)} onClick={() => { setReviewQueue([]); setSelected(video); setConfirmation(""); }} type="button">{requestedVideoIds.has(video.videoId) ? "Request pending" : "Review one"}</button></article>)}</div>
  </div>;
}
