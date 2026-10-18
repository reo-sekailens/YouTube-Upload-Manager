import { useEffect, useId, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, FolderOpen, Link2, Pause, Play, SkipBack, SkipForward, X } from "lucide-react";
import { configureRemotePreviewTool } from "../lib/remote-preview";
import type { ComparisonSyncState } from "../lib/comparison-sync";
import { comparisonButtonClass } from "./DuplicateComparisonPlayers";

const controlClass = "min-h-10 min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2";
const primaryButtonClass = "inline-flex min-h-10 cursor-pointer items-center justify-center gap-2 rounded-md border border-brand bg-brand px-3 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";
type SyncControls = {
  play: () => void;
  pause: () => void;
  seek: (time: number) => void;
  setOffset: (offset: number) => void;
  setLinked: (linked: boolean) => void;
  setAudio: (audio: "left" | "right" | "none") => void;
};

export function comparisonTimeLabel(seconds: number): string {
  const safe = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  return `${hours ? `${hours}:` : ""}${String(minutes).padStart(hours ? 2 : 1, "0")}:${String(safe % 60).padStart(2, "0")}`;
}

export function DuplicateComparisonControls({ state, controls, hasPair }: {
  state: ComparisonSyncState;
  controls?: SyncControls;
  hasPair: boolean;
}) {
  const helpId = useId();
  const [offsetDraft, setOffsetDraft] = useState(String(state.offset));
  useEffect(() => { setOffsetDraft(String(state.offset)); }, [state.offset]);
  const available = Boolean(controls) && state.ready;
  const hasRange = available && state.duration > state.start;
  return <section aria-label="Synchronized comparison controls" className="mb-5 rounded-lg border border-line bg-subtle-surface p-3 sm:p-4">
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className="min-w-0">
        <p className="m-0 text-sm font-semibold text-ink">Compare at the same moment</p>
        <p className="mt-1 mb-0 text-xs leading-relaxed text-muted" role="status">{!hasPair ? "Two available videos are needed for linked playback." : !state.ready ? "Waiting for both players to become ready. Individual player controls remain available." : state.waiting ? "Waiting for video to buffer…" : state.linked ? "Linked playback · synchronization is approximate." : "Players are independent. Link them to compare timestamps."}</p>
      </div>
      <label className="flex min-h-10 cursor-pointer items-center gap-2 text-sm font-semibold text-ink"><input checked={state.linked} className="h-4 w-4 accent-brand" disabled={!controls} onChange={(event) => controls?.setLinked(event.target.checked)} type="checkbox" /><Link2 aria-hidden="true" size={16} /> Link players</label>
    </div>
    <div className="mt-4 flex flex-wrap items-center gap-2">
      <button aria-label="Back 5 seconds" className={comparisonButtonClass} disabled={!available} onClick={() => controls?.seek(state.time - 5)} type="button"><SkipBack aria-hidden="true" size={16} /><span className="hidden sm:inline">5 sec</span></button>
      <button className={`${primaryButtonClass} min-w-32`} disabled={!available || !state.linked} onClick={() => state.playing ? controls?.pause() : controls?.play()} type="button">{state.playing ? <Pause aria-hidden="true" size={16} /> : <Play aria-hidden="true" size={16} />}{state.playing ? "Pause both" : "Play both"}</button>
      <button aria-label="Forward 5 seconds" className={comparisonButtonClass} disabled={!available} onClick={() => controls?.seek(state.time + 5)} type="button"><span className="hidden sm:inline">5 sec</span><SkipForward aria-hidden="true" size={16} /></button>
      <output aria-label="Comparison position" className="ml-auto shrink-0 text-sm font-semibold tabular-nums text-ink">{comparisonTimeLabel(state.time)} <span className="font-normal text-muted">/ {hasRange ? comparisonTimeLabel(state.duration) : "—"}</span></output>
    </div>
    <input aria-label="Comparison timeline" className="mt-3 min-h-7 w-full cursor-pointer accent-brand disabled:cursor-not-allowed" disabled={!hasRange} max={hasRange ? state.duration : 1} min={hasRange ? state.start : 0} onChange={(event) => controls?.seek(Number(event.target.value))} step="0.1" type="range" value={hasRange ? Math.min(state.duration, Math.max(state.start, state.time)) : 0} />
    <details className="mt-2">
      <summary className="min-h-10 cursor-pointer rounded-md py-2 text-sm font-semibold text-ink focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2">Alignment and audio{state.offset !== 0 ? ` · ${state.offset > 0 ? "+" : ""}${state.offset}s offset` : ""}</summary>
      <div className="mt-2 grid gap-3 sm:grid-cols-2">
        <label className="grid gap-1 text-xs font-semibold text-muted">Right video offset (seconds)
          <input aria-describedby={helpId} className={`${controlClass} w-full`} disabled={!controls} max="86400" min="-86400" onBlur={() => setOffsetDraft(String(state.offset))} onChange={(event) => { const draft = event.target.value; setOffsetDraft(draft); const value = Number(draft); if (draft.trim() && Number.isFinite(value)) controls?.setOffset(value); }} step="0.1" type="number" value={offsetDraft} />
        </label>
        <label className="grid gap-1 text-xs font-semibold text-muted">Audio
          <select className={`${controlClass} w-full`} disabled={!controls} onChange={(event) => controls?.setAudio(event.target.value as "left" | "right" | "none")} value={state.audio}><option value="left">Left video</option><option value="right">Right video</option><option value="none">Mute both</option></select>
        </label>
      </div>
      <p className="mt-2 mb-0 text-xs leading-relaxed text-muted" id={helpId}>A +5 offset compares 0:10 on the left with 0:15 on the right. Use it to align trims or added intros.</p>
    </details>
  </section>;
}

function selectedName(path: string): string { return path.split(/[\\/]/).pop() || path; }

export function RemoteComparisonSetup({ runtimeConfigured, onSaved, onClose }: {
  runtimeConfigured: boolean;
  onSaved: () => void;
  onClose: () => void;
}) {
  const [step, setStep] = useState(1);
  const [toolPath, setToolPath] = useState("");
  const [runtimePath, setRuntimePath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const active = useRef(true);
  const pending = useRef(false);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  useEffect(() => { heading.current?.focus(); }, [step]);
  const choose = async (runtime: boolean) => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError("");
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ title: runtime ? "Choose Deno or Node.js JavaScript runtime" : "Choose yt-dlp executable", multiple: false, directory: false });
      if (active.current && typeof path === "string") { if (runtime) setRuntimePath(path); else setToolPath(path); }
    } catch { if (active.current) setError("The executable could not be selected. Try again."); }
    finally { pending.current = false; if (active.current) setBusy(false); }
  };
  const save = async () => {
    if (pending.current || !toolPath) return;
    pending.current = true; setBusy(true); setError("");
    try {
      await configureRemotePreviewTool(toolPath, runtimePath || undefined);
      if (active.current) onSaved();
    } catch { if (active.current) setError("The streaming tools could not be configured. Choose a supported yt-dlp executable and optional Deno or Node.js runtime, then retry. The YouTube embed remains available."); }
    finally { pending.current = false; if (active.current) setBusy(false); }
  };
  return <section aria-label="Set up direct comparison streaming" className="mb-5 rounded-lg border border-line bg-subtle-surface p-4 sm:p-5">
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0"><p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">Step {step} of 2 · Direct streaming setup</p><h3 className="m-0 text-base font-semibold text-ink outline-none" ref={heading} tabIndex={-1}>{step === 1 ? "Choose local streaming tools" : "Review streaming setup"}</h3></div>
      <button aria-label="Cancel streaming setup" className={`${comparisonButtonClass} shrink-0 p-2`} disabled={busy} onClick={onClose} type="button"><X aria-hidden="true" size={18} /></button>
    </div>
    {step === 1 ? <>
      <p className="mt-2 mb-4 max-w-2xl text-sm leading-relaxed text-muted">Choose yt-dlp already installed on this device. For YouTube JavaScript challenges, choose Deno (recommended) or Node.js 22 or later. A runtime is optional and does not guarantee access to every video. The app does not download tools. Browser session access is a separate, optional setup after streaming is configured.</p>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="min-w-0"><p className="mt-0 mb-2 text-sm font-semibold">yt-dlp executable</p><button className={comparisonButtonClass} disabled={busy} onClick={() => void choose(false)} type="button"><FolderOpen aria-hidden="true" size={16} />{toolPath ? "Change yt-dlp" : "Choose yt-dlp"}</button><p className="mt-2 mb-0 wrap-anywhere text-xs text-muted">{toolPath ? selectedName(toolPath) : "Required for direct streaming"}</p></div>
        <div className="min-w-0"><p className="mt-0 mb-2 text-sm font-semibold">JavaScript runtime <span className="font-normal text-muted">(optional)</span></p><button className={comparisonButtonClass} disabled={busy} onClick={() => void choose(true)} type="button"><FolderOpen aria-hidden="true" size={16} />{runtimePath ? "Change runtime" : "Choose runtime"}</button>{runtimePath && <button className={`${comparisonButtonClass} ml-2`} disabled={busy} onClick={() => setRuntimePath("")} type="button">Clear</button>}<p className="mt-2 mb-0 wrap-anywhere text-xs text-muted">{runtimePath ? selectedName(runtimePath) : runtimeConfigured ? "Choose a runtime again to keep JavaScript challenge support" : "No JavaScript runtime selected"}</p></div>
      </div>
    </> : <>
      <dl className="mt-4 mb-0 grid gap-3 text-sm sm:grid-cols-2"><div><dt className="text-xs font-semibold text-muted">yt-dlp</dt><dd className="mt-1 ml-0 wrap-anywhere font-semibold">{selectedName(toolPath)}</dd></div><div><dt className="text-xs font-semibold text-muted">JavaScript runtime</dt><dd className="mt-1 ml-0 wrap-anywhere">{runtimePath ? selectedName(runtimePath) : runtimeConfigured ? "No runtime selected (replaces current runtime)" : "Not configured"}</dd></div></dl>
      <p className="mt-4 mb-0 max-w-2xl text-sm leading-relaxed text-muted">The selected tool runs locally to resolve the YouTube video in this comparison. Playback streams through the app without saving a full video. For private videos, you can separately enable a signed-in browser session. Unsupported or inaccessible videos fall back to the YouTube embed. Your upload and duplicate decisions stay unchanged.</p>
    </>}
    {error && <p className="mt-4 mb-0 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">{error}</p>}
    <div className="mt-5 flex flex-wrap items-center justify-between gap-3"><button className={comparisonButtonClass} disabled={busy} onClick={step === 1 ? onClose : () => setStep(1)} type="button">{step === 2 && <ArrowLeft aria-hidden="true" size={16} />}{step === 1 ? "Cancel" : "Back"}</button>{step === 1 ? <button className={primaryButtonClass} disabled={busy || !toolPath} onClick={() => setStep(2)} type="button">Review setup<ArrowRight aria-hidden="true" size={16} /></button> : <button className={primaryButtonClass} disabled={busy} onClick={() => void save()} type="button">{busy ? "Configuring…" : "Save and try direct streaming"}</button>}</div>
  </section>;
}
