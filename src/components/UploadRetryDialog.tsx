import { ArrowLeft, ArrowRight, CheckCircle2, LoaderCircle, RotateCcw, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { loadConnectionSettings } from "../lib/local";
import { loadUploadRetryReview, retryUploadReview, type UploadRetryReceipt, type UploadRetryReview } from "../lib/upload-retry";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import "./UploadRetryDialog.lazy.css";

export type UploadRetryDialogProps = {
  expectedChannelId: string;
  anchorItemId: string;
  allMatching: boolean;
  channelName?: string;
  onRetried: (receipt: UploadRetryReceipt) => void;
  onClose: () => void;
};
const secondary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const steps = [
  { key: "choose", label: "Choose", heading: "Choose uploads to retry", description: "Review the saved uploads with this failure. The app checks the original files before queueing each retry." },
  { key: "review", label: "Review", heading: "Review the retry", description: "Only these selected uploads will be checked and returned to the queue." },
  { key: "results", label: "Results", heading: "Retry results", description: "Uploads that passed verification are queued. Any upload needing attention keeps its saved record." },
] as const;
type Step = typeof steps[number]["key"];

function validReview(review: UploadRetryReview, channelId: string, anchorItemId: string) {
  const eligible = review.items.filter((item) => item.eligible).length;
  return review.channelId === channelId && Boolean(review.reviewId && review.failureKey) &&
    Number.isFinite(Date.parse(review.expiresAt)) && Date.parse(review.expiresAt) > Date.now() &&
    review.items.length > 0 && review.items.length <= 200 && review.items.some((item) => item.itemId === anchorItemId) &&
    new Set(review.items.map((item) => item.itemId)).size === review.items.length && review.items.every((item) => item.itemId) &&
    review.eligibleCount === eligible && review.blockedCount === review.items.length - eligible;
}

export async function loadRetryForChannel(channelId: string, anchorItemId: string, allMatching: boolean, current: () => boolean) {
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const review = await loadUploadRetryReview(channelId, anchorItemId, allMatching);
  if (!current()) return undefined;
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  if (!validReview(review, channelId, anchorItemId) || (!allMatching && review.items.length !== 1))
    throw new Error("The retry review is stale or does not match this channel. Close it and review the queue again.");
  return review;
}

export async function retryForChannel(review: UploadRetryReview, channelId: string, itemIds: string[], approved: boolean, current: () => boolean) {
  if (!approved || review.channelId !== channelId || Date.parse(review.expiresAt) <= Date.now() || !Number.isFinite(Date.parse(review.expiresAt)) ||
    itemIds.length === 0 || itemIds.length > 200 || new Set(itemIds).size !== itemIds.length ||
    itemIds.some((id) => !review.items.some((item) => item.itemId === id && item.eligible)))
    throw new Error("Approve at least one eligible upload from this current review before retrying.");
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const receipt = await retryUploadReview(channelId, review.reviewId, itemIds);
  if (!current()) return undefined;
  const resultIds = [...receipt.queuedItemIds, ...receipt.blockedItems.map((item) => item.itemId)];
  if (receipt.channelId !== channelId || receipt.reviewId !== review.reviewId || resultIds.length !== itemIds.length ||
    new Set(resultIds).size !== resultIds.length || resultIds.some((id) => !itemIds.includes(id)))
    throw new Error("The retry result could not be matched to this review. Refresh the queue before retrying again.");
  return receipt;
}

export function UploadRetryList({ review, selected, onSelect, page, onPage, receipt }: { review: UploadRetryReview; selected: Set<string>; onSelect?: (id: string, checked: boolean) => void; page: number; onPage: (page: number) => void; receipt?: UploadRetryReceipt }) {
  const entries = receipt ? review.items.filter((item) => receipt.queuedItemIds.includes(item.itemId) || receipt.blockedItems.some((blocked) => blocked.itemId === item.itemId)) : review.items;
  const pages = Math.max(1, Math.ceil(entries.length / 10));
  const visiblePage = Math.min(page, pages);
  return <section aria-label="Uploads in retry review" className="grid min-w-0 gap-3">
    <ul className="m-0 grid list-none gap-2 p-0">{entries.slice((visiblePage - 1) * 10, visiblePage * 10).map((item) => {
      const queued = receipt?.queuedItemIds.includes(item.itemId);
      const blocked = receipt?.blockedItems.find((result) => result.itemId === item.itemId);
      return <li key={item.itemId} className="flex min-w-0 items-start gap-3 rounded-lg border border-line bg-subtle-surface p-4">
        {onSelect ? <input aria-label={`Retry ${item.title || item.fileName}`} className="mt-1 h-5 w-5 shrink-0 accent-brand" type="checkbox" checked={selected.has(item.itemId)} disabled={!item.eligible} onChange={(event) => onSelect(item.itemId, event.target.checked)} /> : null}
        <div className="grid min-w-0 flex-1 gap-1"><div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1"><strong className="break-words text-sm">{item.title || item.fileName}</strong><span className="text-xs font-semibold">{receipt ? queued ? "Queued for retry" : "Needs attention" : item.eligible ? "Ready for verification" : "Needs attention"}</span></div><span className="break-all text-xs text-muted">{item.fileName}</span><p className="mt-1 mb-0 text-sm leading-relaxed text-muted">{receipt ? queued ? "Existing upload and saved progress retained." : blocked?.detail : item.blockedReason || item.detail}</p></div>
      </li>;
    })}</ul>
    {pages > 1 ? <nav aria-label="Retry review pages" className="flex items-center justify-between gap-3"><button className={secondary} type="button" disabled={visiblePage === 1} onClick={() => onPage(visiblePage - 1)}>Previous</button><span className="text-xs text-muted">Page {visiblePage} of {pages}</span><button className={secondary} type="button" disabled={visiblePage === pages} onClick={() => onPage(visiblePage + 1)}>Next page</button></nav> : null}
  </section>;
}

export function UploadRetryDialog({ expectedChannelId, anchorItemId, allMatching, channelName, onRetried, onClose }: UploadRetryDialogProps) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const channelAtOpen = useRef(expectedChannelId);
  const initial = useRef({ anchorItemId, allMatching });
  const callbacks = useRef({ onRetried, onClose }); callbacks.current = { onRetried, onClose };
  const closed = useRef(false);
  const pending = useRef("");
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState<Step>("choose");
  const [review, setReview] = useState<UploadRetryReview>();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState("load");
  const [attempted, setAttempted] = useState(false);
  const [error, setError] = useState("");
  const [receipt, setReceipt] = useState<UploadRetryReceipt>();
  const [page, setPage] = useState(1);
  const details = steps.find((candidate) => candidate.key === step)!;
  const canClose = busy !== "retry";
  const finish = useCallback((force = false) => {
    if (closed.current || (!force && pending.current === "retry")) return;
    closed.current = true; fence.invalidate(); dialog.current?.close(); callbacks.current.onClose();
  }, [fence]);

  useEffect(() => {
    const element = dialog.current;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    element?.showModal(); heading.current?.focus();
    return () => { element?.close(); if (previous?.isConnected) previous.focus(); };
  }, []);
  useEffect(() => { if (channelAtOpen.current !== expectedChannelId) finish(true); }, [expectedChannelId, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step]);
  useEffect(() => {
    const active = fence.begin(); const current = () => active() && !closed.current;
    void loadRetryForChannel(channelAtOpen.current, initial.current.anchorItemId, initial.current.allMatching, current).then((loaded) => {
      if (!loaded || !current()) return;
      setReview(loaded); setSelected(new Set(loaded.items.filter((item) => item.eligible).map((item) => item.itemId)));
    }).catch((failure) => { if (current()) setError(workspaceErrorMessage(failure, "The saved uploads could not be reviewed.")); }).finally(() => { if (current()) setBusy(""); });
    return () => fence.invalidate();
  }, [fence]);

  const retry = async () => {
    if (pending.current || closed.current || !review || attempted) return;
    pending.current = "retry"; setBusy("retry"); setAttempted(true); setError("");
    const active = fence.begin(); const current = () => active() && !closed.current;
    try {
      const result = await retryForChannel(review, expectedChannelId, [...selected], approved, current);
      if (!result || !current()) return;
      setReceipt(result); setPage(1); setStep("results"); callbacks.current.onRetried(result);
    } catch (failure) { if (current()) setError(workspaceErrorMessage(failure, "The retry result could not be confirmed. Close this review and refresh the queue before trying again.")); }
    finally { pending.current = ""; if (current()) setBusy(""); }
  };

  return <dialog ref={dialog} aria-modal="true" aria-labelledby={`${id}-title`} aria-describedby={`${id}-description`} className="m-auto max-h-[calc(100dvh-2rem)] w-[min(48rem,calc(100vw-2rem))] max-w-none overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55" onCancel={(event) => { event.preventDefault(); if (canClose) finish(); }}>
    <div className="grid gap-5 p-5 sm:p-6">
      <header className="flex items-start justify-between gap-4"><div className="min-w-0"><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted">Upload retry · {review?.channelName || channelName || "Connected channel"}</p><h2 ref={heading} tabIndex={-1} id={`${id}-title`} className="m-0 text-xl font-semibold outline-none">{details.heading}</h2><p id={`${id}-description`} className="mt-2 mb-0 max-w-prose text-sm leading-relaxed text-muted">{details.description}</p></div><button className={`${secondary} shrink-0 px-3`} disabled={!canClose} aria-label="Close upload retry" type="button" onClick={() => finish()}><X aria-hidden="true" size={18} /></button></header>
      <ol aria-label="Upload retry steps" className="m-0 grid list-none grid-cols-3 gap-2 p-0">{steps.map((candidate, index) => <li key={candidate.key} aria-current={candidate.key === step ? "step" : undefined} className={`rounded-md border px-3 py-2 text-xs font-semibold ${candidate.key === step ? "border-brand bg-subtle-surface text-ink" : "border-line text-muted"}`}><span className="mr-1">{index + 1}.</span>{candidate.label}</li>)}</ol>
      {error ? <p role="alert" className="m-0 rounded-lg border border-line bg-subtle-surface p-4 text-sm leading-relaxed">{error}</p> : null}
      {busy === "load" ? <p role="status" className="m-0 flex items-center gap-2 text-sm"><LoaderCircle aria-hidden="true" size={18} className="animate-spin" />Checking the saved uploads…</p> : null}
      {review && step !== "results" ? <section className="grid gap-2 rounded-lg border border-line bg-subtle-surface p-4"><strong className="text-sm">{review.failureDetail}</strong><p className="m-0 text-sm leading-relaxed text-muted">{review.eligibleCount} ready for verification · {review.blockedCount} need attention. Each retry keeps its existing upload ID, channel and saved progress.</p></section> : null}
      {review ? <UploadRetryList review={step === "review" ? { ...review, items: review.items.filter((item) => selected.has(item.itemId)) } : review} selected={selected} onSelect={step === "choose" && !busy ? (itemId, checked) => { setSelected((current) => { const next = new Set(current); if (checked) next.add(itemId); else next.delete(itemId); return next; }); setApproved(false); } : undefined} page={page} onPage={setPage} receipt={receipt} /> : null}
      {review && step === "choose" ? <div className="flex flex-wrap items-center justify-between gap-3"><span aria-live="polite" className="text-sm font-semibold">{selected.size} selected</span><div className="flex flex-wrap gap-2"><button type="button" className={secondary} disabled={Boolean(busy) || review.eligibleCount === 0} onClick={() => { setSelected(new Set(review.items.filter((item) => item.eligible).map((item) => item.itemId))); setApproved(false); }}>Select all eligible</button><button type="button" className={secondary} disabled={Boolean(busy) || selected.size === 0} onClick={() => { setSelected(new Set()); setApproved(false); }}>Clear selection</button></div></div> : null}
      {step === "review" ? <section className="grid gap-3 rounded-lg border border-line bg-subtle-surface p-4"><p className="m-0 text-sm leading-relaxed">Retry {selected.size} selected {selected.size === 1 ? "upload" : "uploads"} for {review?.channelName || channelName || "this channel"}. Available, unchanged originals return to the queue. Missing or changed originals stay for attention.</p><p className="m-0 text-sm leading-relaxed text-muted">Saved YouTube sessions reconcile before transfer. Channel and Google Cloud limits still apply.</p><label className="flex items-start gap-3 text-sm font-semibold leading-relaxed"><input className="mt-1 h-5 w-5 shrink-0 accent-brand" type="checkbox" disabled={Boolean(busy) || attempted} checked={approved} onChange={(event) => setApproved(event.target.checked)} /><span>I approve retrying these selected existing uploads.</span></label></section> : null}
      {step === "results" && receipt ? <section role="status" className="grid gap-2 rounded-lg border border-line bg-subtle-surface p-4"><strong className="flex items-center gap-2 text-sm"><CheckCircle2 aria-hidden="true" size={18} />{receipt.queuedItemIds.length} queued · {receipt.blockedItems.length} need attention</strong><p className="m-0 text-sm leading-relaxed text-muted">{receipt.detail}</p></section> : null}
      <footer className="flex flex-wrap justify-between gap-3 border-t border-line pt-4"><div>{step === "review" ? <button className={secondary} type="button" disabled={Boolean(busy) || attempted} onClick={() => { setApproved(false); setPage(1); setStep("choose"); }}><ArrowLeft aria-hidden="true" size={17} />Back</button> : null}</div><div className="flex flex-wrap gap-3"><button className={secondary} type="button" disabled={!canClose} onClick={() => finish()}>{step === "results" || attempted ? "Close" : "Cancel"}</button>{step === "choose" ? <button className={primary} type="button" disabled={Boolean(busy) || !review || selected.size === 0} onClick={() => { setApproved(false); setPage(1); setStep("review"); }}><ArrowRight aria-hidden="true" size={17} />Next: Review</button> : step === "review" ? <button className={primary} type="button" disabled={Boolean(busy) || attempted || !approved || selected.size === 0} onClick={() => { void retry(); }}>{busy === "retry" ? <LoaderCircle aria-hidden="true" size={17} className="animate-spin" /> : <RotateCcw aria-hidden="true" size={17} />}{busy === "retry" ? "Verifying originals…" : `Retry ${selected.size} ${selected.size === 1 ? "upload" : "uploads"}`}</button> : <button className={primary} type="button" onClick={() => finish()}>Done</button>}</div></footer>
    </div>
  </dialog>;
}
