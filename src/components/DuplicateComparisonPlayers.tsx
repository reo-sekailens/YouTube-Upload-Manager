import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { youtubeComparisonEmbedUrl } from "../lib/comparison-controls";
import { createHtmlComparisonPlayer, createYouTubeComparisonPlayer } from "../lib/comparison-sync";
import type { ComparisonPlayer } from "../lib/comparison-sync";
import { watchLocalVideoPicture } from "../lib/duplicate-playback-health";
import { prepareRemotePreview, releaseRemotePreview } from "../lib/remote-preview";
import type { DuplicateMediaPreviewEvidence, DuplicateMediaPreviewTarget } from "../lib/types";
import CompatibilityVideoPlayer from "./CompatibilityVideoPlayer";

export const comparisonButtonClass = "inline-flex min-h-10 cursor-pointer items-center justify-center gap-2 rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";
export type RegisterComparisonPlayer = (player: ComparisonPlayer | undefined) => void;

/** Cancels extraction as well as an acquired stream, including late native replies. */
export function createRemoteComparisonSession(
  target: DuplicateMediaPreviewTarget,
  expectedChannelId: string | undefined,
  videoId: string,
  onReady: (grant: { token: string; url: string }) => void,
  onError: () => void,
  prepare = prepareRemotePreview,
  release = releaseRemotePreview,
): () => void {
  const requestId = crypto.randomUUID();
  let active = true;
  const released = new Set<string>();
  const held = new Set<string>([requestId]);
  const revoke = (tokens: string[]) => {
    const fresh = tokens.filter((token) => !released.has(token));
    fresh.forEach((token) => released.add(token));
    if (fresh.length) void release(fresh).catch(() => undefined);
  };
  void prepare(target, expectedChannelId, videoId, requestId).then((grant) => {
    held.add(grant.token);
    if (!active) revoke([grant.token]);
    else onReady(grant);
  }, () => { if (active) onError(); });
  return () => { active = false; revoke([...held]); };
}

export function LocalComparisonPlayer({ source, onPlayer }: {
  source: DuplicateMediaPreviewEvidence["localSources"][number];
  onPlayer: RegisterComparisonPlayer;
}) {
  const player = useRef<HTMLVideoElement>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  const [compatibility, setCompatibility] = useState<{ initialTime: number; resume: boolean }>();
  const compatibilityAdapter = useRef<ComparisonPlayer | undefined>(undefined);
  const useCompatibility = useCallback(() => {
    const video = player.current;
    setCompatibility({ initialTime: video?.currentTime ?? 0, resume: false });
  }, []);
  const registerCompatibility = useCallback((video: HTMLVideoElement | null) => {
    compatibilityAdapter.current?.dispose();
    compatibilityAdapter.current = video ? createHtmlComparisonPlayer(video) : undefined;
    onPlayer(compatibilityAdapter.current);
  }, [onPlayer]);

  useEffect(() => {
    if (compatibility) return;
    const video = player.current;
    if (!video) return;
    const adapter = createHtmlComparisonPlayer(video);
    onPlayer(adapter);
    const stopHealthCheck = watchLocalVideoPicture(video, useCompatibility);
    if (video.getAttribute("src") !== source.url) { video.src = source.url; video.load(); }
    return () => {
      stopHealthCheck();
      onPlayer(undefined);
      adapter.dispose();
      video.pause();
      video.removeAttribute("src");
      video.load();
    };
  }, [source.url, compatibility, attempt, useCompatibility, onPlayer]);

  if (compatibility) return <>
    <CompatibilityVideoPlayer fileName={source.fileName} initialTime={compatibility.initialTime} resume={compatibility.resume} token={source.token} onPlayer={registerCompatibility} />
    <button className={`${comparisonButtonClass} mt-3`} onClick={() => { setLoading(true); setAttempt((value) => value + 1); setCompatibility(undefined); }} type="button"><RefreshCw aria-hidden="true" size={16} /> Try original player</button>
  </>;

  return <>
    <div className="relative aspect-video w-full overflow-hidden rounded-lg bg-black">
      <video aria-label={`Local video: ${source.fileName}`} className="absolute inset-0 h-full w-full object-contain" controls onLoadedMetadata={() => setLoading(false)} playsInline preload="metadata" ref={player} src={source.url} />
      {loading && <p className="pointer-events-none absolute inset-x-3 top-3 m-0 rounded-md bg-black/75 px-3 py-2 text-sm text-white" role="status">Loading local video…</p>}
    </div>
    <p className="mt-3 mb-0 text-sm leading-relaxed text-muted">Plays directly from the original file. If the picture stays black or only audio plays, use compatibility playback.</p>
    <button className={`${comparisonButtonClass} mt-3`} onClick={useCompatibility} type="button">Use compatibility player</button>
  </>;
}

function EmbedComparisonPlayer({ video, onPlayer, initialTime }: {
  video: DuplicateMediaPreviewEvidence["youtubeVideos"][number];
  onPlayer: RegisterComparisonPlayer;
  initialTime: number;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [failed, setFailed] = useState(false);
  const [status, setStatus] = useState<"loading" | "ready" | "unavailable">("loading");
  const [attempt, setAttempt] = useState(0);
  const start = useRef(Math.max(0, Math.floor(initialTime)));
  useEffect(() => {
    const element = frame.current;
    if (!element) return;
    const expectedUrl = `${youtubeComparisonEmbedUrl(video.videoId)}${start.current ? `&start=${start.current}` : ""}`;
    // StrictMode replays the effect on the same DOM node after source removal.
    if (element.getAttribute("src") !== expectedUrl) element.src = expectedUrl;
    const adapter = createYouTubeComparisonPlayer(element);
    onPlayer(adapter);
    const deadline = Date.now() + 12_000;
    const statusTimer = setInterval(() => {
      const snapshot = adapter.snapshot();
      if (snapshot.failed) setFailed(true);
      else if (snapshot.ready) setStatus("ready");
      else if (Date.now() > deadline) setStatus("unavailable");
    }, 500);
    return () => { clearInterval(statusTimer); onPlayer(undefined); adapter.dispose(); element.removeAttribute("src"); };
  }, [onPlayer, attempt, video.videoId]);
  return <>
    <div className="aspect-video min-h-[200px] w-full overflow-hidden rounded-lg bg-black">
      <iframe allow="autoplay; accelerometer; encrypted-media; gyroscope; picture-in-picture" allowFullScreen className="h-full min-h-[200px] w-full border-0" key={attempt} loading="lazy" onError={() => setFailed(true)} ref={frame} referrerPolicy="strict-origin-when-cross-origin" src={`${youtubeComparisonEmbedUrl(video.videoId)}${start.current ? `&start=${start.current}` : ""}`} title={`Matched YouTube video: ${video.title}`} />
    </div>
    {failed && <p className="mt-3 mb-0 text-sm leading-relaxed text-danger" role="alert">The YouTube player could not be loaded. Check your connection, then retry.</p>}
    {!failed && status === "loading" && <p className="mt-3 mb-0 text-xs text-muted" role="status">Waiting for the YouTube player to become ready…</p>}
    {!failed && status === "unavailable" && <p className="mt-3 mb-0 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="status">The embed has not confirmed playback readiness. Check its message or retry the YouTube player; linked controls wait for a ready player.</p>}
    <p className="mt-3 mb-0 text-sm leading-relaxed text-muted">YouTube embed · approximate synchronization. Private videos may require YouTube sign-in. YouTube can also block playback when embedding is disabled.</p>
    <button className={`${comparisonButtonClass} mt-3`} onClick={() => { setFailed(false); setStatus("loading"); setAttempt((current) => current + 1); }} type="button"><RefreshCw aria-hidden="true" size={16} /> Retry YouTube player</button>
  </>;
}

export function OnlineComparisonPlayer({ video, target, expectedChannelId, configured, configurationVersion, onPlayer }: {
  video: DuplicateMediaPreviewEvidence["youtubeVideos"][number];
  target?: DuplicateMediaPreviewTarget;
  expectedChannelId?: string;
  configured: boolean;
  configurationVersion: number;
  onPlayer: RegisterComparisonPlayer;
}) {
  const player = useRef<HTMLVideoElement>(null);
  const position = useRef(0);
  const [mode, setMode] = useState<"stream" | "embed">(configured && target ? "stream" : "embed");
  const [grant, setGrant] = useState<{ token: string; url: string }>();
  const [attempt, setAttempt] = useState(0);
  const [notice, setNotice] = useState("");
  const [loading, setLoading] = useState(true);
  const fallback = useCallback((message: string) => {
    if (player.current) position.current = player.current.currentTime || position.current;
    setNotice(message);
    setMode("embed");
  }, []);
  useEffect(() => {
    if (configured && target) { setLoading(true); setGrant(undefined); setMode("stream"); }
  // Configuration changes deliberately retry direct mode; ordinary parent renders do not.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [configured, configurationVersion]);
  const targetKey = target ? JSON.stringify(target) : "";
  useEffect(() => {
    if (mode !== "stream" || !configured || !target) return;
    setGrant(undefined);
    setLoading(true);
    return createRemoteComparisonSession(target, expectedChannelId, video.videoId, setGrant, () => fallback("Direct streaming is unavailable for this video. The YouTube embed is available below; your comparison position and alignment are retained."));
  // The stable target key prevents extraction from restarting on unrelated parent renders.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, configured, configurationVersion, targetKey, expectedChannelId, video.videoId, attempt, fallback]);
  useEffect(() => {
    const videoElement = player.current;
    if (mode !== "stream" || !grant || !videoElement) return;
    if (videoElement.getAttribute("src") !== grant.url) { videoElement.src = grant.url; videoElement.load(); }
    const adapter = createHtmlComparisonPlayer(videoElement);
    onPlayer(adapter);
    const stopPictureCheck = watchLocalVideoPicture(videoElement, () => fallback("This stream could not display a video picture. Switched to the YouTube embed at the retained comparison position."));
    return () => {
      position.current = videoElement.currentTime || position.current;
      stopPictureCheck(); onPlayer(undefined); adapter.dispose();
      videoElement.pause(); videoElement.removeAttribute("src"); videoElement.load();
    };
  }, [mode, grant, onPlayer, fallback]);
  const retryStream = () => { setNotice(""); setLoading(true); setGrant(undefined); setAttempt((value) => value + 1); setMode("stream"); };
  return <>
    {notice && <p className="mt-0 mb-3 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="status">{notice}</p>}
    {mode === "stream" ? <>
      <div className="relative aspect-video min-h-[200px] w-full overflow-hidden rounded-lg bg-black">
        {grant && <video aria-label={`Streamed YouTube video: ${video.title}`} className="absolute inset-0 h-full w-full object-contain" controls onError={() => fallback("The direct stream stopped or could not be decoded. Switched to the YouTube embed; your alignment is retained.")} onLoadedMetadata={() => { setLoading(false); if (player.current && position.current) player.current.currentTime = position.current; }} playsInline preload="metadata" ref={player} src={grant.url} />}
        {loading && <p className="pointer-events-none absolute inset-x-3 top-3 m-0 rounded-md bg-black/75 px-3 py-2 text-sm text-white" role="status">{grant ? "Loading direct video stream…" : "Preparing direct stream with yt-dlp…"}</p>}
      </div>
      <p className="mt-3 mb-0 text-sm leading-relaxed text-muted">Direct stream · yt-dlp. Video is reviewed in the app without saving a full video file.</p>
      <button className={`${comparisonButtonClass} mt-3`} onClick={() => fallback("Using the YouTube embed. Your comparison position and alignment are retained.")} type="button">Use YouTube embed</button>
    </> : <>
      <EmbedComparisonPlayer video={video} initialTime={position.current} onPlayer={onPlayer} />
      {configured && target && <button className={`${comparisonButtonClass} mt-3 ml-2`} onClick={retryStream} type="button"><RefreshCw aria-hidden="true" size={16} /> Retry direct stream</button>}
    </>}
  </>;
}
