import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  beginDeletionAuthorization,
  cancelDeletionRequest,
  executeDeletionRequest,
  isTauri,
  listDeletionRequests,
  listRemoteVideos,
  loadConnectionSettings,
  requestVideoDeletion,
} from "../lib/local";
import type { DeletionRequest, RemoteVideo } from "../lib/types";

type DeletionReviewProps = { activeChannel?: string; busy?: boolean; onNotice: (notice: string) => void };
const isPending = (request: DeletionRequest) => request.status === "pending";

export function DeletionReview({ activeChannel, busy = false, onNotice }: DeletionReviewProps) {
  const [videos, setVideos] = useState<RemoteVideo[]>([]);
  const [requests, setRequests] = useState<DeletionRequest[]>([]);
  const [selected, setSelected] = useState<RemoteVideo>();
  const [confirmation, setConfirmation] = useState("");
  const [executionRequest, setExecutionRequest] = useState<DeletionRequest>();
  const [executionConfirmation, setExecutionConfirmation] = useState("");
  const [deletionAuthorized, setDeletionAuthorized] = useState(false);
  const [authorizing, setAuthorizing] = useState(false);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!isTauri || !activeChannel) {
      setVideos([]); setRequests([]); setDeletionAuthorized(false); return;
    }
    setLoading(true);
    try {
      const [nextVideos, nextRequests, settings] = await Promise.all([listRemoteVideos(), listDeletionRequests(), loadConnectionSettings()]);
      setVideos(nextVideos); setRequests(nextRequests); setDeletionAuthorized(settings.deletionAuthorized === true);
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
        setDeletionAuthorized(true); setAuthorizing(false);
        onNotice("Fresh YouTube re-authorization with deletion permission is active for this device. Each pending request still needs its exact video ID typed again before permanent deletion.");
      } catch { /* The authorization browser can still be open; avoid noisy polling errors. */ }
    };
    void checkAuthorization();
    const timer = window.setInterval(() => { void checkAuthorization(); }, 1_250);
    return () => { active = false; window.clearInterval(timer); };
  }, [authorizing, onNotice]);

  const closeRequestConfirmation = () => { setSelected(undefined); setConfirmation(""); };
  const closeExecutionConfirmation = () => { setExecutionRequest(undefined); setExecutionConfirmation(""); };
  const createRequest = async () => {
    if (!selected || confirmation !== selected.videoId) return;
    setLoading(true);
    try {
      const request = await requestVideoDeletion(selected.videoId, confirmation);
      setRequests((current) => [request, ...current.filter((candidate) => candidate.videoId !== request.videoId)]);
      closeRequestConfirmation();
      onNotice(`A local deletion request for ${request.videoId} was recorded. It has not deleted the YouTube video.`);
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
      onNotice("Google opened in your browser for fresh deletion re-authorization. Return here after consent; permanent execution stays unavailable until the native grant is confirmed.");
    } catch (error) {
      setAuthorizing(false);
      onNotice(error instanceof Error ? error.message : "Deletion authorization could not be started.");
    }
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
    } catch (error) { onNotice(error instanceof Error ? error.message : "YouTube did not confirm the deletion; the request remains pending."); }
    finally { setLoading(false); }
  };

  const pendingRequests = requests.filter(isPending);
  const requestedVideoIds = new Set(pendingRequests.map((request) => request.videoId));
  const disabled = busy || loading || authorizing;
  if (!activeChannel) return <p className="deletion-review__empty">Connect and sync a channel before reviewing locally saved YouTube videos.</p>;

  return <div className="deletion-review">
    <header className="deletion-review__warning">
      <strong>Permanent deletion is never automatic.</strong>
      <span>
        Every selected video gets a local review request. YouTube execution
        requires fresh Google re-authorization and a second typed-ID
        confirmation.
      </span>
    </header>
    <section className="deletion-authorization" aria-labelledby="deletion-authorization-heading">
      <div>
        <p className="eyebrow">FRESH RE-AUTHORIZATION</p>
        <h3 id="deletion-authorization-heading">YouTube deletion authority</h3>
        <p>
          {deletionAuthorized
            ? "Fresh re-authorization including deletion permission is active on this device. It only enables final confirmation for a saved request."
            : "Deletion permission is not granted. Creating and cancelling local requests never asks for this authority."}
        </p>
      </div>
      {deletionAuthorized ? <span className="connection-status connection-status--connected"><span aria-hidden="true" />Granted</span> : <button className="danger-button" disabled={disabled} onClick={() => void authorizeDeletion()} type="button">{authorizing ? "Waiting for Google…" : "Grant deletion permission"}</button>}
    </section>
    {pendingRequests.length > 0 && <section className="deletion-requests" aria-labelledby="deletion-requests-heading">
      <header>
        <p className="eyebrow">LOCAL REVIEW QUEUE</p>
        <h3 id="deletion-requests-heading">Pending local requests</h3>
      </header>
      <ul>
        {pendingRequests.map((request) => <li key={request.id}>
          <div>
            <strong>{request.title}</strong>
            <span>{request.videoId}</span>
            <p>{request.detail}</p>
          </div>
          <div className="deletion-request-actions">
            {deletionAuthorized && <button className="danger-button" disabled={disabled} onClick={() => { setExecutionRequest(request); setExecutionConfirmation(""); }} type="button">Execute permanent deletion</button>}
            <button className="text-button" disabled={disabled} onClick={() => void cancelRequest(request)} type="button">Cancel request</button>
          </div>
        </li>)}
      </ul>
    </section>}
    {selected && <section className="deletion-confirmation" aria-labelledby="deletion-confirmation-heading"><p className="eyebrow">REQUEST CONFIRMATION</p><h3 id="deletion-confirmation-heading">Request deletion for “{selected.title}”</h3><p>This creates a local pending request only. Type the full video ID below to prove this is the intended video.</p><code>{selected.videoId}</code><label htmlFor="delete-video-confirmation">Video ID confirmation</label><input id="delete-video-confirmation" autoComplete="off" autoFocus value={confirmation} onChange={(event) => setConfirmation(event.target.value)} placeholder={selected.videoId} spellCheck={false} /><div className="deletion-confirmation__actions"><button className="secondary-action" disabled={disabled} onClick={closeRequestConfirmation} type="button">Cancel</button><button className="danger-button" disabled={disabled || confirmation !== selected.videoId} onClick={() => void createRequest()} type="button">Create local request</button></div></section>}
    {executionRequest && <section className="deletion-confirmation deletion-confirmation--final" aria-labelledby="execution-confirmation-heading"><p className="eyebrow">PERMANENT, IRREVERSIBLE ACTION</p><h3 id="execution-confirmation-heading">Delete “{executionRequest.title}” from YouTube</h3><p>This calls YouTube’s delete endpoint. The video cannot be restored by this app. Type the exact video ID a second time to execute this permanent deletion.</p><code>{executionRequest.videoId}</code><label htmlFor="execute-delete-video-confirmation">Exact video ID for permanent deletion</label><input id="execute-delete-video-confirmation" autoComplete="off" autoFocus value={executionConfirmation} onChange={(event) => setExecutionConfirmation(event.target.value)} placeholder={executionRequest.videoId} spellCheck={false} /><div className="deletion-confirmation__actions"><button className="secondary-action" disabled={disabled} onClick={closeExecutionConfirmation} type="button">Keep video</button><button className="danger-button" disabled={disabled || executionConfirmation !== executionRequest.videoId} onClick={() => void executeRequest()} type="button">Delete permanently from YouTube</button></div></section>}
    <div className="remote-video-list" aria-busy={loading}>{videos.length === 0 && <p className="deletion-review__empty">{loading ? "Loading locally saved inventory…" : "Sync the YouTube library to review videos."}</p>}{videos.map((video) => <article className="remote-video-card" key={video.videoId}><div><span className="remote-video-card__scope">YouTube video</span><h3>{video.title}</h3><p>ID: <code>{video.videoId}</code>{video.privacyStatus ? ` · ${video.privacyStatus}` : ""}{video.duration ? ` · ${video.duration}` : ""}</p></div><button className="danger-button" disabled={disabled || requestedVideoIds.has(video.videoId)} onClick={() => { setSelected(video); setConfirmation(""); }} type="button">{requestedVideoIds.has(video.videoId) ? "Request pending" : "Request deletion"}</button></article>)}</div>
  </div>;
}
