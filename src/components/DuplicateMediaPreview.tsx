import { useCallback, useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { RefreshCw, X } from "lucide-react";
import { createComparisonSync } from "../lib/comparison-sync";
import type { ComparisonPlayer, ComparisonSyncState } from "../lib/comparison-sync";
import {
  prepareDuplicateMediaPreview,
  releaseDuplicateMediaPreview,
} from "../lib/duplicate-media-preview";
import { getRemotePreviewConfiguration } from "../lib/remote-preview";
import type { RemotePreviewConfiguration } from "../lib/remote-preview";
import { BrowserSessionSetup } from "./BrowserSessionSetup";
import { LocalComparisonPlayer, OnlineComparisonPlayer } from "./DuplicateComparisonPlayers";
import type { RegisterComparisonPlayer } from "./DuplicateComparisonPlayers";
import { DuplicateComparisonControls, RemoteComparisonSetup } from "./DuplicateComparisonTools";
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

const initialComparisonState: ComparisonSyncState = {
  time: 0, duration: 0, start: 0, playing: false, waiting: false,
  ready: false, offset: 0, linked: true, audio: "left",
};

/** One controller owns the pair; changing a source never resets its review selection. */
export function DuplicateMediaPreviewContent({ evidence, target, expectedChannelId }: {
  evidence: DuplicateMediaPreviewEvidence;
  target?: DuplicateMediaPreviewTarget;
  expectedChannelId?: string;
}) {
  const hasLocal = evidence.localSources.length > 0;
  const hasYouTube = evidence.youtubeVideos.length > 0;
  const unavailableLocalSources = evidence.unavailableLocalSources ?? 0;
  const onlineOnlyPair = !hasLocal && unavailableLocalSources === 0 && evidence.youtubeVideos.length > 1;
  const showSelectors = evidence.localSources.length > 1 || evidence.youtubeVideos.length > 1;
  const [left, setLeft] = useState<ComparisonPlayer>();
  const [right, setRight] = useState<ComparisonPlayer>();
  const registerLeft = useCallback<RegisterComparisonPlayer>((player) => setLeft(player), []);
  const registerRight = useCallback<RegisterComparisonPlayer>((player) => setRight(player), []);
  const [state, setState] = useState<ComparisonSyncState>(initialComparisonState);
  const retainedState = useRef(initialComparisonState);
  const [controller, setController] = useState<ReturnType<typeof createComparisonSync>>();
  const [configuration, setConfiguration] = useState<RemotePreviewConfiguration>();
  const [configurationVersion, setConfigurationVersion] = useState(0);
  const [setup, setSetup] = useState<"tools" | "browser">();
  const toolsButton = useRef<HTMLButtonElement>(null);
  const browserButton = useRef<HTMLButtonElement>(null);
  const active = useRef(true);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  useEffect(() => {
    if (!left || !right) { setController(undefined); setState((value) => ({ ...value, ready: false, playing: false })); return; }
    const saved = { ...retainedState.current };
    const next = createComparisonSync(left, right, (value) => { retainedState.current = value; setState(value); });
    next.setOffset(saved.offset);
    next.setLinked(saved.linked);
    next.setAudio(saved.audio);
    next.seek(saved.time);
    setController(next);
    return () => next.dispose();
  }, [left, right]);
  useEffect(() => {
    if (!target || !hasYouTube) return;
    let current = true;
    void getRemotePreviewConfiguration(expectedChannelId).then((value) => { if (current) setConfiguration(value); }, () => { if (current) setConfiguration({ configured: false, runtimeConfigured: false, supported: false }); });
    return () => { current = false; };
  // Only a scoped modal requests native configuration; static evidence fixtures stay usable.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasYouTube, Boolean(target), expectedChannelId]);
  const configured = Boolean(configuration?.configured && configuration.supported && target);
  const closeSetup = () => {
    const returnToBrowser = setup === "browser";
    setSetup(undefined);
    requestAnimationFrame(() => { if (active.current) (returnToBrowser ? browserButton : toolsButton).current?.focus(); });
  };
  const savedSetup = () => {
    closeSetup();
    void getRemotePreviewConfiguration(expectedChannelId).then((value) => {
      if (!active.current) return;
      setConfiguration(value);
      setConfigurationVersion((value) => value + 1);
    }, () => { if (active.current) setConfiguration({ configured: false, runtimeConfigured: false, supported: false }); });
  };
  const savedBrowserSession = (value: RemotePreviewConfiguration) => {
    closeSetup();
    setConfiguration(value);
    setConfigurationVersion((version) => version + 1);
  };
  const online = { evidence, showSelector: showSelectors, target, expectedChannelId, configured, configurationVersion };
  return <>
    {unavailableLocalSources > 0 && <p className="mt-0 mb-4 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="status">{unavailableLocalSources} local file{unavailableLocalSources === 1 ? " is" : "s are"} unavailable for playback. Available matches are shown below.</p>}
    {setup === "tools" && <RemoteComparisonSetup runtimeConfigured={configuration?.runtimeConfigured ?? false} onClose={closeSetup} onSaved={savedSetup} />}
    {setup === "browser" && expectedChannelId && <BrowserSessionSetup expectedChannelId={expectedChannelId} initialSession={configuration?.browserSession} onClose={closeSetup} onSaved={savedBrowserSession} />}
    <DuplicateComparisonControls state={state} controls={controller} hasPair={Boolean(left && right)} />
    {hasYouTube && target && <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
      <p className="m-0 text-xs leading-relaxed text-muted">{configured ? configuration?.browserSession ? "Direct streaming preferred · browser session enabled for this channel · YouTube embed fallback" : "Direct streaming preferred · YouTube embed available as fallback" : configuration?.supported === false ? "Direct streaming is unavailable on this device. Use the YouTube embed." : "YouTube embed · set up yt-dlp for direct in-app streaming"}</p>
      {configuration?.supported !== false && !setup && <div className="flex flex-wrap items-center gap-2"><button className={buttonClass} disabled={!configuration} onClick={() => { controller?.pause(); setSetup("tools"); }} ref={toolsButton} type="button">{configured ? "Streaming setup" : "Set up direct streaming"}</button>{configured && expectedChannelId && <button className={buttonClass} onClick={() => { controller?.pause(); setSetup("browser"); }} ref={browserButton} type="button">{configuration?.browserSession ? "Browser session settings" : "Use browser session"}</button>}</div>}
    </div>}
    <div className="grid min-w-0 gap-6 md:grid-cols-2">
      {!onlineOnlyPair ? <LocalMediaPane evidence={evidence} label="Suspected local duplicate" selectorLabel="Local file to compare" showSelector={showSelectors} onPlayer={registerLeft} /> : <YouTubeMediaPane {...online} label="YouTube upload" selectorLabel="First YouTube video to compare" onPlayer={registerLeft} />}
      {hasYouTube ? <YouTubeMediaPane {...online} initialIndex={onlineOnlyPair ? 1 : 0} label="Matching online upload" selectorLabel="YouTube match to compare" onPlayer={registerRight} /> : hasLocal && evidence.localSources.length > 1 ? <LocalMediaPane evidence={evidence} initialIndex={1} label="Matching local file" selectorLabel="Matching local file to compare" showSelector={showSelectors} onPlayer={registerRight} /> : <YouTubeMediaPane {...online} label="Matching online upload" selectorLabel="YouTube match to compare" onPlayer={registerRight} />}
    </div>
  </>;
}

function LocalMediaPane({ evidence, initialIndex = 0, label, selectorLabel, showSelector, onPlayer }: {
  evidence: DuplicateMediaPreviewEvidence;
  initialIndex?: number;
  label: string;
  selectorLabel: string;
  showSelector: boolean;
  onPlayer: RegisterComparisonPlayer;
}) {
  const [localIndex, setLocalIndex] = useState(initialIndex);
  const localSource = evidence.localSources[localIndex];
  const id = useId();
  return <section aria-labelledby={`${id}-local-heading`} className="min-w-0">
    <p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">{label}</p>
    <h3 className="mt-0 mb-3 wrap-anywhere text-base font-semibold text-ink" id={`${id}-local-heading`}>{localSource?.title || localSource?.fileName || "Local file"}</h3>
    {showSelector && evidence.localSources.length > 0 && <label className="mb-3 grid gap-1.5 text-sm font-semibold text-ink">{selectorLabel}<select className={selectClass} disabled={evidence.localSources.length === 1} onChange={(event) => setLocalIndex(Number(event.target.value))} value={localIndex}>{evidence.localSources.map((source, index) => <option key={source.token} value={index}>{source.fileName}</option>)}</select></label>}
    {localSource ? <LocalComparisonPlayer key={localSource.token} source={localSource} onPlayer={onPlayer} /> : <p className="m-0 rounded-lg bg-subtle-surface p-4 text-sm leading-relaxed text-muted">{(evidence.unavailableLocalSources ?? 0) > 0 ? "The original file could not be opened for playback." : "No available local file was found for this match."}</p>}
    {localSource && <p className="mt-2 mb-0 wrap-anywhere text-xs text-muted">{localSource.fileName}</p>}
  </section>;
}

function YouTubeMediaPane({ evidence, initialIndex = 0, label, selectorLabel, showSelector, target, expectedChannelId, configured, configurationVersion, onPlayer }: {
  evidence: DuplicateMediaPreviewEvidence;
  initialIndex?: number;
  label: string;
  selectorLabel: string;
  showSelector: boolean;
  target?: DuplicateMediaPreviewTarget;
  expectedChannelId?: string;
  configured: boolean;
  configurationVersion: number;
  onPlayer: RegisterComparisonPlayer;
}) {
  const [youtubeIndex, setYouTubeIndex] = useState(initialIndex);
  const youtubeVideo = evidence.youtubeVideos[youtubeIndex];
  const id = useId();
  return <section aria-labelledby={`${id}-youtube-heading`} className="min-w-0">
    <p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">{label}</p>
    <h3 className="mt-0 mb-3 wrap-anywhere text-base font-semibold text-ink" id={`${id}-youtube-heading`}>{youtubeVideo?.title || "YouTube video"}</h3>
    {showSelector && evidence.youtubeVideos.length > 0 && <label className="mb-3 grid gap-1.5 text-sm font-semibold text-ink">{selectorLabel}<select className={selectClass} disabled={evidence.youtubeVideos.length === 1} onChange={(event) => setYouTubeIndex(Number(event.target.value))} value={youtubeIndex}>{evidence.youtubeVideos.map((video, index) => <option key={video.videoId} value={index}>{video.title}</option>)}</select></label>}
    {youtubeVideo ? <OnlineComparisonPlayer key={youtubeVideo.videoId} video={youtubeVideo} target={target} expectedChannelId={expectedChannelId} configured={configured} configurationVersion={configurationVersion} onPlayer={onPlayer} /> : <p className="m-0 rounded-lg bg-subtle-surface p-4 text-sm leading-relaxed text-muted">No matching YouTube upload is available for this review.</p>}
  </section>;
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
        {!isClosed && (evidence ? <DuplicateMediaPreviewContent evidence={evidence} target={target} expectedChannelId={expectedChannelId} key={requestKey} /> : errorKey === requestKey ? (
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
