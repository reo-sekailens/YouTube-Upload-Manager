import { ArrowLeft, ArrowRight, FileSearch, LoaderCircle, Play, RefreshCw, Square, X } from "lucide-react";
import { lazy, Suspense, useCallback, useDeferredValue, useEffect, useId, useMemo, useRef, useState } from "react";
import { loadConnectionSettings } from "../lib/local";
import { cancelPerceptualReview, loadPerceptualReview, startPerceptualReview, type PerceptualCandidate, type PerceptualReview } from "../lib/perceptual-review";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import "./PerceptualReviewDialog.lazy.css";

const DuplicateMediaPreview = lazy(() => import("./DuplicateMediaPreview").then((module) => ({ default: module.DuplicateMediaPreview })));
const pageSize = 10;
const maxSelected = 16;
const button = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const control = "min-h-11 w-full min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring";
const steps = ["Select originals", "Analyze", "Review candidates"];

export type PerceptualReviewDialogProps = {
  expectedChannelId: string;
  channelName?: string;
  items: Array<{ id: string; title: string; fileName: string }>;
  onClose: () => void;
};

export function validPerceptualSelection(itemIds: string[]) {
  return itemIds.length > 0 && itemIds.length <= maxSelected && new Set(itemIds).size === itemIds.length && itemIds.every((itemId) => Boolean(itemId.trim()));
}

function validateReview(review: PerceptualReview, channelId: string, itemIds: string[], reviewId?: string) {
  if (review.channelId !== channelId || !review.reviewId || (reviewId && review.reviewId !== reviewId)
    || review.total !== itemIds.length || review.sources.length !== itemIds.length
    || !Number.isInteger(review.processed) || review.processed < 0 || review.processed > review.total
    || new Set(review.sources.map((source) => source.itemId)).size !== itemIds.length
    || review.sources.some((source) => !itemIds.includes(source.itemId))
    || review.candidates.length > 100 || new Set(review.candidates.map((candidate) => candidate.candidateId)).size !== review.candidates.length
    || review.candidates.some((candidate) => candidate.leftItemId === candidate.rightItemId || !candidate.candidateId
      || (!itemIds.includes(candidate.leftItemId) && !itemIds.includes(candidate.rightItemId))))
    throw new Error("The local analysis result no longer matches this channel and selected originals. Reopen the review.");
  return review;
}

/** A late analysis still owns native work; cancel it without adopting its results. */
export async function startPerceptualForChannel(channelId: string, itemIds: string[], approved: boolean, current: () => boolean) {
  if (!approved || !validPerceptualSelection(itemIds)) throw new Error("Select and approve 1–16 original videos before starting local analysis.");
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const review = await startPerceptualReview(channelId, itemIds, approved);
  try {
    if (!current()) { await cancelPerceptualReview(channelId, review.reviewId); return undefined; }
    validateReview(review, channelId, itemIds);
    assertWorkspaceChannel(await loadConnectionSettings(), channelId);
    if (!current()) { await cancelPerceptualReview(channelId, review.reviewId); return undefined; }
    return review;
  } catch (error) {
    await cancelPerceptualReview(channelId, review.reviewId).catch(() => undefined);
    throw error;
  }
}

export async function loadPerceptualForChannel(channelId: string, reviewId: string, itemIds: string[], current: () => boolean) {
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const review = await loadPerceptualReview(channelId, reviewId);
  if (!current()) return undefined;
  validateReview(review, channelId, itemIds, reviewId);
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  return current() ? review : undefined;
}

export async function cancelPerceptualForChannel(channelId: string, reviewId: string, itemIds: string[], current: () => boolean) {
  const review = await cancelPerceptualReview(channelId, reviewId);
  return current() ? validateReview(review, channelId, itemIds, reviewId) : undefined;
}

function secondsLabel(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) return "Unknown";
  const whole = Math.floor(seconds);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}

export function PerceptualCandidateCard({ candidate, algorithmVersion, onCompare }: { candidate: PerceptualCandidate; algorithmVersion: string; onCompare?: () => void }) {
  return <article className="grid min-w-0 gap-4 rounded-lg border border-line bg-subtle-surface p-4">
    <div className="flex flex-col items-start gap-3 sm:flex-row sm:justify-between"><div className="min-w-0 flex-1"><p className="mt-0 mb-2 text-xs font-semibold text-warning-ink">{candidate.evidence === "limited" ? "Limited evidence · inspect carefully" : "Possible content match"}</p><h3 className="m-0 break-words text-base font-semibold">{candidate.leftTitle}</h3><p className="mt-2 mb-0 break-words text-sm text-muted">Compared with <strong className="font-semibold text-ink">{candidate.rightTitle}</strong></p></div>{onCompare ? <button className={button} onClick={onCompare} type="button"><Play aria-hidden="true" size={16} />Compare originals</button> : null}</div>
    <dl className="m-0 grid gap-3 text-sm sm:grid-cols-2"><div><dt className="text-xs text-muted">Original durations</dt><dd className="mt-1 ml-0 font-semibold">{secondsLabel(candidate.leftDurationSeconds)} / {secondsLabel(candidate.rightDurationSeconds)}<span className="ml-2 text-xs font-normal text-muted">{candidate.durationDifferencePercent.toFixed(2)}% difference</span></dd></div><div><dt className="text-xs text-muted">Aligned informative samples</dt><dd className="mt-1 ml-0 font-semibold">{candidate.alignedSamples} of {candidate.totalSamples}<span className="mt-1 block text-xs font-normal text-muted">Samples at corresponding positions in each video</span></dd></div><div><dt className="text-xs text-muted">Fingerprint distance</dt><dd className="mt-1 ml-0">Median {candidate.medianDistance} · maximum {candidate.maxDistance}<span className="mt-1 block text-xs text-muted">Lower distances indicate more similar sampled frames; this is not a probability.</span></dd></div><div><dt className="text-xs text-muted">Original evidence</dt><dd className="mt-1 ml-0">{candidate.sourceEvidenceCurrent ? "Both originals verified in this analysis" : "Retained fingerprint · current original not verified"}{candidate.retainedEvidenceAsOf ? <span className="mt-1 block break-words text-xs text-muted">Fingerprint recorded {candidate.retainedEvidenceAsOf}</span> : null}</dd></div></dl>
    <p className="m-0 text-sm leading-relaxed text-muted">Sample similarity suggests footage to inspect. It does not establish a full byte-hash match or verify YouTube’s remote content.</p>
    {candidate.warnings.length ? <ul className="m-0 grid list-disc gap-2 pl-5 text-sm leading-relaxed text-warning-ink">{candidate.warnings.map((warning, index) => <li className="break-words" key={`${index}-${warning}`}>{warning}</li>)}</ul> : null}
    <details className="min-w-0 text-xs"><summary className="cursor-pointer font-semibold">Alignment and sample evidence</summary><div className="mt-3 overflow-x-auto"><table className="w-full border-collapse text-left"><caption className="mb-3 text-left text-muted">{algorithmVersion} · sampled local frames only</caption><thead><tr className="border-b border-line text-muted"><th className="px-2 py-2 font-semibold">Position</th><th className="px-2 py-2 font-semibold">First video</th><th className="px-2 py-2 font-semibold">Second video</th><th className="px-2 py-2 font-semibold">Distance</th><th className="px-2 py-2 font-semibold">Evidence</th></tr></thead><tbody>{candidate.samples.slice(0, 16).map((sample, index) => <tr className="border-b border-line" key={index}><td className="px-2 py-2">{(sample.position * 100).toFixed(0)}%</td><td className="px-2 py-2">{secondsLabel(sample.leftTimeSeconds)}</td><td className="px-2 py-2">{secondsLabel(sample.rightTimeSeconds)}</td><td className="px-2 py-2">{sample.distance}</td><td className="px-2 py-2">{sample.informative ? "Informative" : "Limited scene"}</td></tr>)}</tbody></table></div></details>
  </article>;
}

export function PerceptualCandidateList({ review, page, onPage, onCompare }: { review: PerceptualReview; page: number; onPage: (page: number) => void; onCompare?: (candidate: PerceptualCandidate) => void }) {
  const pages = Math.max(1, Math.ceil(review.candidates.length / pageSize));
  const visiblePage = Math.max(0, Math.min(page, pages - 1));
  return <section className="grid min-w-0 gap-4" aria-label="Possible content matches">
    {review.candidates.length ? <><p className="m-0 text-sm font-semibold">{review.candidates.length} candidate{review.candidates.length === 1 ? "" : "s"} for human review</p>{review.candidates.slice(visiblePage * pageSize, (visiblePage + 1) * pageSize).map((candidate) => <PerceptualCandidateCard algorithmVersion={review.algorithmVersion} candidate={candidate} key={candidate.candidateId} onCompare={onCompare ? () => onCompare(candidate) : undefined} />)}{pages > 1 ? <nav aria-label="Candidate pages" className="flex flex-wrap items-center justify-between gap-3"><button className={button} disabled={visiblePage === 0} onClick={() => onPage(visiblePage - 1)} type="button">Previous candidates</button><span className="text-xs text-muted">Page {visiblePage + 1} of {pages}</span><button className={button} disabled={visiblePage === pages - 1} onClick={() => onPage(visiblePage + 1)} type="button">Next candidates</button></nav> : null}</> : <div className="rounded-lg bg-subtle-surface p-4"><h3 className="m-0 text-sm font-semibold">No candidate matches were found</h3><p className="mt-2 mb-0 text-sm leading-relaxed text-muted">This bounded sample analysis cannot prove that the videos contain different footage. Review the per-file results for unavailable originals or limited samples.</p></div>}
  </section>;
}

export function PerceptualReviewDialog({ expectedChannelId, channelName, items, onClose }: PerceptualReviewDialogProps) {
  const id = useId(); const dialog = useRef<HTMLDialogElement>(null); const heading = useRef<HTMLHeadingElement>(null);
  const openedChannel = useRef(expectedChannelId); const callbacks = useRef({ onClose }); callbacks.current = { onClose };
  const closed = useRef(false); const pending = useRef(false); const reviewRef = useRef<PerceptualReview | undefined>(undefined); const analysisIds = useRef<string[]>([]);
  const comparisonTrigger = useRef<HTMLElement | null>(null);
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState(0); const [query, setQuery] = useState(""); const [selected, setSelected] = useState<Set<string>>(new Set());
  const [selectionPage, setSelectionPage] = useState(0); const [sourcePage, setSourcePage] = useState(0); const [candidatePage, setCandidatePage] = useState(0);
  const [approved, setApproved] = useState(false); const [review, setReview] = useState<PerceptualReview>(); const [busy, setBusy] = useState(false);
  const [error, setError] = useState(""); const [preview, setPreview] = useState<PerceptualCandidate>();
  const deferredQuery = useDeferredValue(query.trim().toLocaleLowerCase());
  const filtered = useMemo(() => items.filter((item) => `${item.title} ${item.fileName}`.toLocaleLowerCase().includes(deferredQuery)), [items, deferredQuery]);
  const selectionPages = Math.max(1, Math.ceil(filtered.length / pageSize)); const visibleSelectionPage = Math.min(selectionPage, selectionPages - 1);
  const visibleItems = filtered.slice(visibleSelectionPage * pageSize, (visibleSelectionPage + 1) * pageSize);
  const selectedItems = items.filter((item) => selected.has(item.id));
  useEffect(() => {
    const available = new Set(items.map((item) => item.id));
    if ([...selected].some((itemId) => !available.has(itemId))) {
      setSelected(new Set([...selected].filter((itemId) => available.has(itemId))));
      setApproved(false);
    }
  }, [items, selected]);
  const sourcePages = Math.max(1, Math.ceil((review?.sources.length || selectedItems.length) / pageSize)); const visibleSourcePage = Math.min(sourcePage, sourcePages - 1);
  const scopeMatches = openedChannel.current === expectedChannelId;
  const running = review?.state === "running";
  const adopt = (next: PerceptualReview) => { reviewRef.current = next; setReview(next); };
  const finish = useCallback(() => {
    if (closed.current) return; closed.current = true; fence.invalidate(); setPreview(undefined);
    const active = reviewRef.current;
    reviewRef.current = undefined;
    if (active?.state === "running") void cancelPerceptualReview(active.channelId, active.reviewId).catch(() => undefined);
    dialog.current?.close(); callbacks.current.onClose();
  }, [fence]);
  useEffect(() => {
    const element = dialog.current; const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    element?.showModal(); heading.current?.focus();
    return () => { element?.close(); const active = reviewRef.current; if (active?.state === "running") void cancelPerceptualReview(active.channelId, active.reviewId).catch(() => undefined); if (previous?.isConnected) previous.focus(); };
  }, []);
  useEffect(() => { if (!scopeMatches) finish(); }, [scopeMatches, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step]);
  const run = useCallback(async (operation: (current: () => boolean) => Promise<void>) => {
    if (pending.current || closed.current || !scopeMatches) return;
    pending.current = true; setBusy(true); setError(""); const request = fence.begin(); const current = () => !closed.current && request();
    try { await operation(current); } catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "The local analysis could not finish. Saved uploads and duplicate decisions are unchanged.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  }, [fence, scopeMatches]);
  const refresh = useCallback(() => run(async (current) => {
    const active = reviewRef.current; if (!active) return;
    const result = await loadPerceptualForChannel(expectedChannelId, active.reviewId, analysisIds.current, current);
    if (result) { reviewRef.current = result; setReview(result); }
  }), [expectedChannelId, run]);
  useEffect(() => {
    if (!running || busy || error || preview) return;
    const timer = window.setTimeout(() => void refresh(), 2000);
    return () => window.clearTimeout(timer);
  }, [running, busy, error, preview, review?.processed, refresh]);
  const start = () => run(async (current) => {
    const itemIds = selectedItems.map((item) => item.id);
    const result = await startPerceptualForChannel(expectedChannelId, itemIds, approved, current);
    if (result) { analysisIds.current = itemIds; adopt(result); setSourcePage(0); }
  });
  const stop = () => run(async (current) => {
    const active = reviewRef.current; if (!active || active.state !== "running") return;
    const result = await cancelPerceptualForChannel(expectedChannelId, active.reviewId, analysisIds.current, current);
    if (result) adopt(result);
  });
  const reset = () => { fence.invalidate(); reviewRef.current = undefined; setReview(undefined); setApproved(false); setError(""); setSourcePage(0); setCandidatePage(0); setStep(0); };
  const select = (itemId: string, checked: boolean) => { setSelected((previous) => { const next = new Set(previous); if (checked && next.size < maxSelected) next.add(itemId); else if (!checked) next.delete(itemId); return next; }); setApproved(false); };
  const compare = (candidate: PerceptualCandidate) => { comparisonTrigger.current = document.activeElement instanceof HTMLElement ? document.activeElement : null; setPreview(candidate); };
  const closePreview = () => {
    setPreview(undefined);
    window.requestAnimationFrame(() => { if (!closed.current && comparisonTrigger.current?.isConnected) comparisonTrigger.current.focus(); comparisonTrigger.current = null; });
  };

  return <>
    <dialog aria-describedby={`${id}-description`} aria-labelledby={`${id}-heading`} aria-modal="true" className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-4xl overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/60" inert={preview ? true : undefined} onCancel={(event) => { event.preventDefault(); finish(); }} ref={dialog}>
      <header className="border-b border-line px-5 pt-5 pb-4 sm:px-7"><div className="flex items-start justify-between gap-4"><div className="min-w-0"><p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">Local footage review · {channelName || "Connected channel"}</p><h2 className="m-0 text-xl font-semibold outline-none" id={`${id}-heading`} ref={heading} tabIndex={-1}>{step === 0 ? "Select originals to compare" : step === 1 ? "Analyze sampled local footage" : "Review possible content matches"}</h2></div><button aria-label="Close local footage review" className={`${button} shrink-0 px-3`} onClick={finish} type="button"><X aria-hidden="true" size={18} /></button></div><p className="mt-3 mb-0 max-w-2xl text-sm leading-relaxed text-muted" id={`${id}-description`}>Find possible matches after re-encoding by inspecting sampled frames. Originals stay in place, and uploads and duplicate decisions stay unchanged.</p><ol aria-label="Local footage review steps" className="mt-5 mb-0 flex list-none gap-2 p-0">{steps.map((label, index) => <li aria-current={step === index ? "step" : undefined} className={`flex min-w-0 flex-1 gap-2 rounded-md px-2 py-2 text-xs font-semibold ${step === index ? "bg-subtle-surface text-ink" : "text-muted"}`} key={label}><span>{index + 1}.</span><span>{label}</span></li>)}</ol></header>
      <div className="grid min-w-0 gap-5 px-5 py-5 sm:px-7">
        {error ? <p className="m-0 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">{error}</p> : null}
        {step === 0 ? <><div className="rounded-lg bg-subtle-surface p-4"><p className="m-0 text-sm font-semibold">Opt-in, device-local analysis</p><p className="mt-2 mb-0 text-sm leading-relaxed text-muted">Choose 1–16 saved original videos. Analysis compares their sampled frames and retained fingerprints from this channel, using bounded local work after uploads and recovery.</p><p className="mt-2 mb-0 text-sm leading-relaxed text-muted">Similar intros, still scenes, short clips and edits can produce misleading matches. No YouTube video is downloaded or remotely fingerprinted.</p></div>{items.length ? <><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-search`}>Find a saved original<input className={control} id={`${id}-search`} onChange={(event) => { setQuery(event.target.value); setSelectionPage(0); }} type="search" value={query} /></label><div className="flex flex-wrap items-center justify-between gap-3"><p className="m-0 text-sm font-semibold">{selectedItems.length} of {maxSelected} selected</p><button className={button} disabled={selected.size === 0} onClick={() => { setSelected(new Set()); setApproved(false); }} type="button">Clear selection</button></div><ul aria-label="Saved original videos" className="m-0 grid list-none gap-2 p-0">{visibleItems.map((item) => <li className="min-w-0" key={item.id}><label className="flex min-h-14 cursor-pointer items-start gap-3 rounded-md bg-subtle-surface p-3 text-sm"><input aria-label={`Analyze ${item.fileName}`} checked={selected.has(item.id)} className="mt-1 h-4 w-4 shrink-0 accent-brand" disabled={!selected.has(item.id) && selected.size >= maxSelected} onChange={(event) => select(item.id, event.target.checked)} type="checkbox" /><span className="min-w-0"><strong className="block break-words font-semibold">{item.title || item.fileName}</strong><span className="mt-1 block break-all text-xs text-muted">{item.fileName}</span></span></label></li>)}</ul>{!visibleItems.length ? <p className="m-0 text-sm text-muted">No saved originals match this search.</p> : null}{selectionPages > 1 ? <nav aria-label="Original selection pages" className="flex flex-wrap items-center justify-between gap-3"><button className={button} disabled={visibleSelectionPage === 0} onClick={() => setSelectionPage(visibleSelectionPage - 1)} type="button">Previous originals</button><span className="text-xs text-muted">Page {visibleSelectionPage + 1} of {selectionPages}</span><button className={button} disabled={visibleSelectionPage === selectionPages - 1} onClick={() => setSelectionPage(visibleSelectionPage + 1)} type="button">Next originals</button></nav> : null}</> : <div className="rounded-lg bg-subtle-surface p-4"><h3 className="m-0 text-sm font-semibold">No saved original videos</h3><p className="mt-2 mb-0 text-sm text-muted">Add videos through the normal upload review, then return here to select their saved originals.</p></div>}</> : null}
        {step === 1 ? <>{!review ? <><div className="rounded-lg bg-subtle-surface p-4"><h3 className="m-0 text-sm font-semibold">Review the selected originals</h3><ul aria-label="Originals approved for analysis" className="mt-3 mb-0 grid list-none gap-2 p-0 text-sm">{selectedItems.slice(visibleSourcePage * pageSize, (visibleSourcePage + 1) * pageSize).map((item) => <li className="break-words" key={item.id}>{item.title || item.fileName}<span className="mt-1 block break-all text-xs text-muted">{item.fileName}</span></li>)}</ul></div><label className="flex items-start gap-3 rounded-lg border border-line p-4 text-sm"><input checked={approved} className="mt-1 h-4 w-4 shrink-0 accent-brand" disabled={busy} onChange={(event) => setApproved(event.target.checked)} type="checkbox" /><span>I approve sampling these originals and comparing retained local fingerprints for this channel. The results are candidates for playback review and will not delete files, suppress uploads or change duplicate decisions.</span></label></> : <><section aria-label="Local analysis progress" className="grid gap-3 rounded-lg bg-subtle-surface p-4"><div className="flex flex-wrap items-baseline justify-between gap-2"><h3 className="m-0 text-sm font-semibold">{review.state === "running" ? "Local analysis is running" : review.state === "complete" ? "Local analysis completed" : review.state === "cancelled" ? "Local analysis cancelled" : review.state === "interrupted" ? "Local analysis was interrupted" : "Local analysis needs attention"}</h3><span className="text-xs text-muted">{review.processed} of {review.total} originals checked</span></div>{running ? <progress aria-label="Originals analyzed" className="h-2 w-full accent-brand" max={Math.max(1, review.total)} value={review.processed} /> : null}<p className="m-0 text-xs leading-relaxed text-muted">{review.algorithmVersion} · {review.decoderAvailable ? "Local decoder available" : "Local decoder unavailable"}</p><p className="m-0 text-sm leading-relaxed text-muted">Completed fingerprints and per-file results remain local. Cancelled or interrupted analysis may have incomplete coverage.</p>{running ? <button className={`${button} justify-self-start`} disabled={busy} onClick={() => void stop()} type="button"><Square aria-hidden="true" size={16} />Cancel analysis</button> : null}{error ? <button className={`${button} justify-self-start`} disabled={busy} onClick={() => void refresh()} type="button"><RefreshCw aria-hidden="true" size={16} />Refresh saved analysis</button> : null}</section><ul aria-label="Per-file analysis results" className="m-0 grid list-none gap-3 p-0">{review.sources.slice(visibleSourcePage * pageSize, (visibleSourcePage + 1) * pageSize).map((source) => <li className="grid min-w-0 gap-2 rounded-md bg-subtle-surface p-4" key={source.itemId}><strong className="break-words text-sm">{source.title || source.fileName}</strong><p className="m-0 break-all text-xs text-muted">{source.fileName}</p><p className="m-0 text-xs font-semibold">{source.state === "analyzed" ? "Analyzed original" : source.state === "retained" ? "Reused verified fingerprint" : source.state === "unavailable" ? "Original unavailable" : source.state === "failed" ? "Analysis failed" : "Waiting to analyze"}{source.durationSeconds == null ? "" : ` · ${secondsLabel(source.durationSeconds)}`} · {source.informativeSamples} informative samples of {source.sampleCount}</p><p className="m-0 text-sm leading-relaxed text-muted">{source.detail}</p></li>)}</ul></>}{sourcePages > 1 ? <nav aria-label="Analysis source pages" className="flex flex-wrap items-center justify-between gap-3"><button className={button} disabled={visibleSourcePage === 0} onClick={() => setSourcePage(visibleSourcePage - 1)} type="button">Previous files</button><span className="text-xs text-muted">Page {visibleSourcePage + 1} of {sourcePages}</span><button className={button} disabled={visibleSourcePage === sourcePages - 1} onClick={() => setSourcePage(visibleSourcePage + 1)} type="button">Next files</button></nav> : null}</> : null}
        {step === 2 && review ? <><div className="rounded-lg bg-notice-surface p-4 text-notice-ink"><h3 className="m-0 text-sm font-semibold">Playback review, with limited evidence</h3><p className="mt-2 mb-0 text-sm leading-relaxed">Compare both local originals in the app. A possible content match does not establish identical bytes. YouTube’s remote content evidence remains unavailable.</p><p className="mt-2 mb-0 text-sm leading-relaxed">This review has no deletion or upload-suppression action. Use the existing guarded workflows only after inspecting the originals.</p></div>{review.state !== "complete" ? <p className="m-0 rounded-md bg-warning-surface p-3 text-sm text-warning-ink" role="status">Analysis {review.state}; these saved results may be incomplete.</p> : null}<PerceptualCandidateList onCompare={compare} onPage={setCandidatePage} page={candidatePage} review={review} /></> : null}
        {review?.warnings.length ? <ul className="m-0 grid list-disc gap-2 pl-5 text-sm leading-relaxed text-warning-ink">{review.warnings.map((warning, index) => <li className="break-words" key={`${index}-${warning}`}>{warning}</li>)}</ul> : null}
      </div>
      <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-line px-5 py-4 sm:px-7"><div className="flex flex-wrap gap-2">{step > 0 && !running ? <button className={button} disabled={busy} onClick={() => { if (step === 2) { setStep(1); setSourcePage(0); } else reset(); }} type="button"><ArrowLeft aria-hidden="true" size={16} />Back</button> : null}<button className={button} onClick={finish} type="button">{running ? "Close and cancel analysis" : "Close"}</button></div>{step === 0 ? <button className={primary} disabled={!validPerceptualSelection(selectedItems.map((item) => item.id))} onClick={() => { setSourcePage(0); setStep(1); }} type="button">Next: Analyze<ArrowRight aria-hidden="true" size={16} /></button> : step === 1 && !review ? <button className={primary} disabled={busy || !approved || !validPerceptualSelection(selectedItems.map((item) => item.id))} onClick={() => void start()} type="button">{busy ? <LoaderCircle aria-hidden="true" className="animate-spin" size={16} /> : <FileSearch aria-hidden="true" size={16} />}{busy ? "Starting local analysis…" : "Start local analysis"}</button> : step === 1 && review ? <button className={primary} disabled={busy || running} onClick={() => { setCandidatePage(0); setStep(2); }} type="button">Next: Review candidates<ArrowRight aria-hidden="true" size={16} /></button> : <button className={primary} onClick={finish} type="button">Done reviewing</button>}</footer>
    </dialog>
    {preview && review ? <Suspense fallback={<p className="fixed right-4 bottom-4 z-50 rounded-md bg-surface p-3 text-sm text-ink shadow-xl" role="status">Opening local comparison…</p>}><DuplicateMediaPreview expectedChannelId={expectedChannelId} onClose={closePreview} reviewEvidence={<div className="mb-6"><PerceptualCandidateCard algorithmVersion={review.algorithmVersion} candidate={preview} /></div>} target={{ kind: "perceptual", reviewId: review.reviewId, candidateId: preview.candidateId }} /></Suspense> : null}
  </>;
}
