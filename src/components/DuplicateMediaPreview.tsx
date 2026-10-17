import { useCallback, useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { RefreshCw, X } from "lucide-react";
import { youtubeComparisonEmbedUrl } from "../lib/comparison-controls";
import {
  prepareDuplicateMediaPreview,
  releaseDuplicateMediaPreview,
} from "../lib/duplicate-media-preview";
import { watchLocalVideoPicture } from "../lib/duplicate-playback-health";
import CompatibilityVideoPlayer from "./CompatibilityVideoPlayer";
import "./DuplicateMediaPreview.lazy.css";
import type {
  DuplicateMediaPreviewEvidence,
  DuplicateMediaPreviewTarget,
} from "../lib/types";

const buttonClass =
  "inline-flex cursor-pointer items-center justify-center gap-2 rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2";
const selectClass =
  "w-full min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2";

/** Owns each acquisition once, including a response received after the modal closes. */
export function createDuplicatePreviewSession(
  request: Promise<DuplicateMediaPreviewEvidence>,
  onReady: (evidence: DuplicateMediaPreviewEvidence) => void,
  onError: () => void,
  release: (tokens: string[]) => Promise<void> = releaseDuplicateMediaPreview,
): () => void {
  let active = true;
  let heldTokens: string[] = [];
  const revoke = (tokens: string[]) => {
    if (tokens.length > 0) void release(tokens).catch(() => undefined);
  };
  void request.then(
    (evidence) => {
      const tokens = [...new Set(evidence.localSources.map((source) => source.token))];
      if (!active) {
        revoke(tokens);
        return;
      }
      heldTokens = tokens;
      onReady(evidence);
    },
    () => {
      if (active) onError();
    },
  );
  return () => {
    active = false;
    revoke(heldTokens);
    heldTokens = [];
  };
}

function LocalVideoPlayer({
  source,
}: {
  source: DuplicateMediaPreviewEvidence["localSources"][number];
}) {
  const player = useRef<HTMLVideoElement>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  const [compatibility, setCompatibility] = useState<{ initialTime: number; resume: boolean }>();
  const useCompatibility = useCallback(() => {
    const video = player.current;
    setCompatibility({ initialTime: video?.currentTime ?? 0, resume: video ? !video.paused : false });
  }, []);

  useEffect(() => {
    if (compatibility) return;
    const video = player.current;
    if (!video) return;
    const stopHealthCheck = watchLocalVideoPicture(video, useCompatibility);
    // StrictMode replays effects on the same node after its cleanup removed src.
    if (video && video.getAttribute("src") !== source.url) {
      video.src = source.url;
      video.load();
    }
    return () => {
      stopHealthCheck();
      // Remove the source before revocation so no playback/request survives a switch.
      video?.pause();
      video?.removeAttribute("src");
      video?.load();
    };
  }, [source.url, compatibility, attempt, useCompatibility]);

  if (compatibility) return <>
    <CompatibilityVideoPlayer fileName={source.fileName} initialTime={compatibility.initialTime} resume={compatibility.resume} token={source.token} />
    <button className={`${buttonClass} mt-3`} onClick={() => { setLoading(true); setAttempt((value) => value + 1); setCompatibility(undefined); }} type="button"><RefreshCw aria-hidden="true" size={16} /> Try original player</button>
  </>;

  return (
    <>
      <div className="relative aspect-video w-full overflow-hidden rounded-lg bg-black">
        <video
          aria-label={`Local video: ${source.fileName}`}
          className="absolute inset-0 h-full w-full object-contain"
          controls
          onLoadedMetadata={() => setLoading(false)}
          playsInline
          preload="metadata"
          ref={player}
          src={source.url}
        />
        {loading && <p className="pointer-events-none absolute inset-x-3 top-3 m-0 rounded-md bg-black/75 px-3 py-2 text-sm text-white" role="status">Loading local video…</p>}
      </div>
      <p className="mt-3 mb-0 text-sm leading-relaxed text-muted">Plays directly from the original file. If the picture stays black or only audio plays, use compatibility playback.</p>
      <button className={`${buttonClass} mt-3`} onClick={useCompatibility} type="button">Use compatibility player</button>
    </>
  );
}

function YouTubeVideoPlayer({
  video,
}: {
  video: DuplicateMediaPreviewEvidence["youtubeVideos"][number];
}) {
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  return (
    <>
      <div className="aspect-video min-h-[200px] w-full overflow-hidden rounded-lg bg-black">
        <iframe
          allow="accelerometer; encrypted-media; gyroscope; picture-in-picture"
          allowFullScreen
          className="h-full min-h-[200px] w-full border-0"
          key={attempt}
          loading="lazy"
          onError={() => { setLoading(false); setFailed(true); }}
          onLoad={() => setLoading(false)}
          referrerPolicy="strict-origin-when-cross-origin"
          src={youtubeComparisonEmbedUrl(video.videoId)}
          title={`Matched YouTube video: ${video.title}`}
        />
      </div>
      {loading && !failed && <p className="mt-3 mb-0 text-sm text-muted" role="status">Loading YouTube player…</p>}
      {failed && <p className="mt-3 mb-0 text-sm leading-relaxed text-danger" role="alert">The YouTube player could not be loaded. Check your connection, then retry.</p>}
      <p className="mt-3 mb-0 text-sm leading-relaxed text-muted">Private videos may require YouTube sign-in. YouTube can also block playback when embedding is disabled.</p>
      <button className={`${buttonClass} mt-3`} onClick={() => { setFailed(false); setLoading(true); setAttempt((current) => current + 1); }} type="button"><RefreshCw aria-hidden="true" size={16} /> Retry YouTube player</button>
    </>
  );
}

/** Only selected media mounts; changing a selection tears down the previous player. */
export function DuplicateMediaPreviewContent({
  evidence,
}: {
  evidence: DuplicateMediaPreviewEvidence;
}) {
  const hasLocal = evidence.localSources.length > 0;
  const hasYouTube = evidence.youtubeVideos.length > 0;
  const unavailableLocalSources = evidence.unavailableLocalSources ?? 0;
  const onlineOnlyPair = !hasLocal && unavailableLocalSources === 0 && evidence.youtubeVideos.length > 1;
  const showSelectors = evidence.localSources.length > 1 || evidence.youtubeVideos.length > 1;
  return (
    <>
    {unavailableLocalSources > 0 && <p className="mt-0 mb-4 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="status">{unavailableLocalSources} local file{unavailableLocalSources === 1 ? " is" : "s are"} unavailable for playback. Available matches are shown below.</p>}
    <div className="grid min-w-0 gap-6 md:grid-cols-2">
      {!onlineOnlyPair ? (
        <LocalMediaPane evidence={evidence} label="Suspected local duplicate" selectorLabel="Local file to compare" showSelector={showSelectors} />
      ) : <YouTubeMediaPane evidence={evidence} label="YouTube upload" selectorLabel="First YouTube video to compare" showSelector={showSelectors} />}
      {hasYouTube ? (
        <YouTubeMediaPane evidence={evidence} initialIndex={onlineOnlyPair ? 1 : 0} label="Matching online upload" selectorLabel="YouTube match to compare" showSelector={showSelectors} />
      ) : hasLocal && evidence.localSources.length > 1 ? (
        <LocalMediaPane evidence={evidence} initialIndex={1} label="Matching local file" selectorLabel="Matching local file to compare" showSelector={showSelectors} />
      ) : <YouTubeMediaPane evidence={evidence} label="Matching online upload" selectorLabel="YouTube match to compare" showSelector={showSelectors} />}
    </div>
    </>
  );
}

function LocalMediaPane({ evidence, initialIndex = 0, label, selectorLabel, showSelector }: {
  evidence: DuplicateMediaPreviewEvidence;
  initialIndex?: number;
  label: string;
  selectorLabel: string;
  showSelector: boolean;
}) {
  const [localIndex, setLocalIndex] = useState(initialIndex);
  const localSource = evidence.localSources[localIndex];
  const id = useId();
  return (
      <section aria-labelledby={`${id}-local-heading`} className="min-w-0">
        <p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">{label}</p>
        <h3 className="mt-0 mb-3 wrap-anywhere text-base font-semibold text-ink" id={`${id}-local-heading`}>{localSource?.title || localSource?.fileName || "Local file"}</h3>
        {showSelector && evidence.localSources.length > 0 && (
          <label className="mb-3 grid gap-1.5 text-sm font-semibold text-ink">{selectorLabel}
            <select className={selectClass} disabled={evidence.localSources.length === 1} onChange={(event) => setLocalIndex(Number(event.target.value))} value={localIndex}>
              {evidence.localSources.map((source, index) => <option key={source.token} value={index}>{source.fileName}</option>)}
            </select>
          </label>
        )}
        {localSource ? <LocalVideoPlayer key={localSource.token} source={localSource} /> : <p className="m-0 rounded-lg bg-subtle-surface p-4 text-sm leading-relaxed text-muted">{(evidence.unavailableLocalSources ?? 0) > 0 ? "The original file could not be opened for playback." : "No available local file was found for this match."}</p>}
        {localSource && <p className="mt-2 mb-0 wrap-anywhere text-xs text-muted">{localSource.fileName}</p>}
      </section>
  );
}

function YouTubeMediaPane({ evidence, initialIndex = 0, label, selectorLabel, showSelector }: {
  evidence: DuplicateMediaPreviewEvidence;
  initialIndex?: number;
  label: string;
  selectorLabel: string;
  showSelector: boolean;
}) {
  const [youtubeIndex, setYouTubeIndex] = useState(initialIndex);
  const youtubeVideo = evidence.youtubeVideos[youtubeIndex];
  const id = useId();
  return (
      <section aria-labelledby={`${id}-youtube-heading`} className="min-w-0">
        <p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">{label}</p>
        <h3 className="mt-0 mb-3 wrap-anywhere text-base font-semibold text-ink" id={`${id}-youtube-heading`}>{youtubeVideo?.title || "YouTube video"}</h3>
        {showSelector && evidence.youtubeVideos.length > 0 && (
          <label className="mb-3 grid gap-1.5 text-sm font-semibold text-ink">{selectorLabel}
            <select className={selectClass} disabled={evidence.youtubeVideos.length === 1} onChange={(event) => setYouTubeIndex(Number(event.target.value))} value={youtubeIndex}>
              {evidence.youtubeVideos.map((video, index) => <option key={video.videoId} value={index}>{video.title}</option>)}
            </select>
          </label>
        )}
        {youtubeVideo ? <YouTubeVideoPlayer key={youtubeVideo.videoId} video={youtubeVideo} /> : <p className="m-0 rounded-lg bg-subtle-surface p-4 text-sm leading-relaxed text-muted">No matching YouTube upload is available for this review.</p>}
      </section>
  );
}

export function DuplicateMediaPreview({
  target,
  expectedChannelId,
  onClose,
  reviewEvidence,
}: {
  target: DuplicateMediaPreviewTarget;
  expectedChannelId?: string;
  onClose: () => void;
  reviewEvidence?: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const dispose = useRef<(() => void) | undefined>(undefined);
  const channelAtOpen = useRef(expectedChannelId);
  const closed = useRef(false);
  const [isClosed, setIsClosed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<{ key: string; evidence: DuplicateMediaPreviewEvidence }>();
  const [errorKey, setErrorKey] = useState<string>();
  const id = useId();
  const targetKey = target.kind === "upload" ? `upload:${target.itemId}` : target.kind === "candidate" ? `candidate:${target.candidateId}` : target.kind === "perceptual" ? `perceptual:${target.reviewId}:${target.candidateId}` : `preflight:${target.jobId}:${target.ordinal}`;
  const requestKey = `${targetKey}:${attempt}`;
  const channelMatches = channelAtOpen.current === expectedChannelId;
  const evidence = channelMatches && result?.key === requestKey ? result.evidence : undefined;

  const finish = useCallback(() => {
    if (closed.current) return;
    closed.current = true;
    dialog.current?.querySelectorAll("video").forEach((video) => {
      video.pause();
      video.removeAttribute("src");
      video.load();
    });
    dialog.current?.querySelectorAll("iframe").forEach((frame) => frame.removeAttribute("src"));
    dispose.current?.();
    setIsClosed(true);
    dialog.current?.close();
    onCloseRef.current();
  }, []);

  useEffect(() => {
    const element = dialog.current;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    element?.showModal();
    closeButton.current?.focus();
    return () => {
      element?.close();
      if (previousFocus?.isConnected) previousFocus.focus();
    };
  }, []);

  useEffect(() => {
    if (!channelMatches) finish();
  }, [channelMatches, finish]);

  useEffect(() => {
    if (closed.current || !channelMatches) return;
    const release = createDuplicatePreviewSession(
      prepareDuplicateMediaPreview(target, expectedChannelId),
      (value) => setResult({ key: requestKey, evidence: value }),
      () => setErrorKey(requestKey),
    );
    dispose.current = release;
    return release;
    // The primitive key represents the target, avoiding reacquisition for new prop objects.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetKey, attempt, expectedChannelId, channelMatches]);

  return (
    <dialog
      aria-describedby={`${id}-description`}
      aria-labelledby={`${id}-heading`}
      aria-modal="true"
      className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-6xl overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55"
      onCancel={(event) => { event.preventDefault(); finish(); }}
      onClose={() => { if (!dialog.current?.open) finish(); }}
      ref={dialog}
    >
      <header className="flex items-start justify-between gap-4 border-b border-line p-4 sm:p-6">
        <div className="min-w-0">
          <h2 className="m-0 text-lg font-bold tracking-tight" id={`${id}-heading`}>Compare suspected duplicate</h2>
          <p className="mt-2 mb-0 max-w-2xl text-sm leading-relaxed text-muted" id={`${id}-description`}>{target.kind === "perceptual" ? "Compare the two local originals with their sampled content evidence. A possible content match is separate from an exact byte match." : "Watch the original file beside its matching YouTube upload before returning to your duplicate decision."}</p>
        </div>
        <button aria-label="Close video comparison" className={`${buttonClass} shrink-0 p-2`} onClick={finish} ref={closeButton} title="Close comparison (Escape)" type="button"><X aria-hidden="true" size={20} /></button>
      </header>
      <div className="p-4 sm:p-6">
        {reviewEvidence}
        {!isClosed && (evidence ? <DuplicateMediaPreviewContent evidence={evidence} key={requestKey} /> : errorKey === requestKey ? (
          <div className="rounded-lg bg-warning-surface p-4 text-sm leading-relaxed text-warning-ink" role="alert">
            <p className="mt-0 mb-2 font-semibold">The comparison could not be opened.</p>
            <p className="mt-0 mb-4">The original file may be unavailable, the duplicate evidence may have changed, or the connected channel may have changed. Your upload decision is unchanged.</p>
            <button className={buttonClass} onClick={() => setAttempt((current) => current + 1)} type="button"><RefreshCw aria-hidden="true" size={16} /> Try again</button>
          </div>
        ) : <p className="m-0 py-10 text-sm text-muted" role="status">Preparing video comparison…</p>)}
      </div>
      <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-line bg-subtle-surface p-4 sm:px-6">
        <p className="m-0 max-w-2xl text-sm leading-relaxed text-muted">Playback only. Close this comparison to continue reviewing the duplicate.</p>
        <button className={buttonClass} onClick={finish} type="button">Close comparison</button>
      </footer>
    </dialog>
  );
}
