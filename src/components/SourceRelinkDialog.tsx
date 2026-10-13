import { ArrowLeft, ArrowRight, CheckCircle2, FileSearch, FolderOpen, LoaderCircle, ShieldCheck, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { cancelSourceRelink, commitSourceRelink, loadSourceRelinkReview, prepareSourceRelink, verifySourceRelink, type SourceRelinkMode, type SourceRelinkReceipt, type SourceRelinkReview } from "../lib/source-relink";
import { loadConnectionSettings } from "../lib/local";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import "./SourceRelinkDialog.lazy.css";

export type SourceRelinkDialogProps = {
  items: Array<{ id: string; title: string; fileName: string }>;
  expectedChannelId: string;
  channelName?: string;
  onSaved: (receipt: SourceRelinkReceipt) => void;
  onClose: () => void;
};

const secondary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const steps = [
  { key: "location", label: "Select location", heading: "Locate the original videos", description: "Repair an existing upload after a folder or drive location changes." },
  { key: "verify", label: "Verify files", heading: "Verify the same original bytes", description: "Every replacement must match the full digest already saved for its upload." },
  { key: "review", label: "Review", heading: "Review the repaired locations", description: "Only the selected, verified originals will be updated." },
] as const;
type Step = typeof steps[number]["key"];

/** A late native review still owns local resources; release it without showing it. */
export async function prepareRelinkForChannel(itemIds: string[], channelId: string, path: string, mode: SourceRelinkMode, current: () => boolean): Promise<SourceRelinkReview | undefined> {
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const review = await prepareSourceRelink(itemIds, channelId, path, mode);
  if (!current() || review.channelId !== channelId || review.files.length !== itemIds.length || new Set(review.files.map((file) => file.itemId)).size !== itemIds.length || review.files.some((file) => !itemIds.includes(file.itemId))) {
    await cancelSourceRelink(review.reviewId, channelId);
    if (current()) throw new Error("The connected channel changed. Choose the original location again.");
    return undefined;
  }
  return review;
}

export async function saveRelinkForChannel(review: SourceRelinkReview, channelId: string, itemIds: string[], resumeUploads: boolean, approved: boolean, current: () => boolean): Promise<SourceRelinkReceipt | undefined> {
  if (!approved || review.state !== "verified" || review.channelId !== channelId || itemIds.length === 0 || itemIds.some((id) => !review.files.some((file) => file.itemId === id && file.status === "verified")))
    throw new Error("Review and approve at least one verified original before saving.");
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const receipt = await commitSourceRelink(review.reviewId, channelId, itemIds, resumeUploads, approved);
  if (!current()) return undefined;
  if (receipt.channelId !== channelId || receipt.reviewId !== review.reviewId || receipt.relinkedItemIds.length !== itemIds.length || new Set(receipt.relinkedItemIds).size !== itemIds.length || receipt.relinkedItemIds.some((id) => !itemIds.includes(id)) || [...receipt.resumedItemIds, ...receipt.attentionItemIds].some((id) => !receipt.relinkedItemIds.includes(id)))
    throw new Error("The saved result did not match this review. Refresh the queue before continuing.");
  return receipt;
}

export async function verifyRelinkForChannel(review: SourceRelinkReview, channelId: string, current: () => boolean): Promise<SourceRelinkReview | undefined> {
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const verified = await verifySourceRelink(review.reviewId, channelId);
  if (!current()) return undefined;
  if (verified.channelId !== channelId || verified.reviewId !== review.reviewId || verified.files.length !== review.files.length || new Set(verified.files.map((file) => file.itemId)).size !== review.files.length || verified.files.some((file) => !review.files.some((original) => original.itemId === file.itemId)))
    throw new Error("The verification result did not match this channel. Choose the location again.");
  return verified;
}

export function SourceRelinkFileList({ review, selected, onSelect, page, onPage }: { review: SourceRelinkReview; selected?: Set<string>; onSelect?: (id: string, checked: boolean) => void; page: number; onPage: (page: number) => void }) {
  const pages = Math.max(1, Math.ceil(review.files.length / 10));
  const visiblePage = Math.min(page, pages);
  return <section aria-label="Original file verification" className="grid min-w-0 gap-3">
    <div className="flex flex-wrap items-baseline justify-between gap-2"><strong className="text-sm">{review.verifiedCount} verified · {review.blockedCount} need attention</strong><span className="text-xs text-muted">{review.files.length} selected uploads</span></div>
    <ul className="m-0 grid list-none gap-3 p-0">
      {review.files.slice((visiblePage - 1) * 10, visiblePage * 10).map((file) => <li key={file.itemId} className="min-w-0 rounded-lg border border-line bg-subtle-surface p-4">
        <div className="flex items-start gap-3">
          {onSelect ? <input aria-label={`Repair ${file.fileName}`} type="checkbox" className="mt-1 h-5 w-5 shrink-0 accent-brand" checked={selected?.has(file.itemId) || false} disabled={file.status !== "verified"} onChange={(event) => onSelect(file.itemId, event.target.checked)} /> : null}
          <div className="grid min-w-0 flex-1 gap-2">
            <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1"><strong className="break-words text-sm">{file.title || file.fileName}</strong><span className="inline-flex items-center gap-1 text-xs font-semibold">{file.status === "verified" ? <CheckCircle2 aria-hidden="true" size={15} /> : null}{file.status === "verified" ? "Verified match" : file.status === "blocked" ? "Needs attention" : "Not checked yet"}</span></div>
            <dl className="m-0 grid gap-2 text-xs"><div><dt className="font-semibold text-muted">Previous location</dt><dd className="mt-1 ml-0 break-all">{file.previousLocation}</dd></div><div><dt className="font-semibold text-muted">Replacement location</dt><dd className="mt-1 ml-0 break-all">{file.replacementLocation}</dd></div></dl>
            <p className="m-0 text-sm leading-relaxed text-muted">{file.detail}</p>
          </div>
        </div>
      </li>)}
    </ul>
    {pages > 1 ? <nav aria-label="Verification pages" className="flex items-center justify-between gap-3"><button className={secondary} type="button" disabled={visiblePage === 1} onClick={() => onPage(visiblePage - 1)}>Previous</button><span className="text-xs text-muted">Page {visiblePage} of {pages}</span><button className={secondary} type="button" disabled={visiblePage === pages} onClick={() => onPage(visiblePage + 1)}>Next page</button></nav> : null}
  </section>;
}

export function SourceRelinkDialog({ items, expectedChannelId, channelName, onSaved, onClose }: SourceRelinkDialogProps) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const channelAtOpen = useRef(expectedChannelId);
  const currentChannel = useRef(expectedChannelId);
  currentChannel.current = expectedChannelId;
  const initialItems = useRef(items);
  const callbacks = useRef({ onSaved, onClose });
  callbacks.current = { onSaved, onClose };
  const reviewRef = useRef<SourceRelinkReview | undefined>(undefined);
  const closed = useRef(false);
  const pending = useRef("");
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState<Step>("location");
  const [mode, setMode] = useState<SourceRelinkMode>(items.length > 1 ? "folder" : "file");
  const [path, setPath] = useState("");
  const [review, setReview] = useState<SourceRelinkReview>();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [resume, setResume] = useState(true);
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [page, setPage] = useState(1);
  const details = steps.find((candidate) => candidate.key === step)!;
  const canClose = busy !== "save";

  const release = useCallback(() => {
    const owned = reviewRef.current;
    reviewRef.current = undefined;
    if (owned && owned.state !== "committed") void cancelSourceRelink(owned.reviewId, channelAtOpen.current).catch(() => undefined);
  }, []);

  const finish = useCallback(() => {
    if (closed.current || pending.current === "save") return;
    closed.current = true;
    fence.invalidate();
    release();
    dialog.current?.close();
    callbacks.current.onClose();
  }, [fence, release]);

  useEffect(() => {
    const element = dialog.current;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    element?.showModal();
    heading.current?.focus();
    return () => { element?.close(); release(); if (previous?.isConnected) previous.focus(); };
  }, [release]);
  useEffect(() => { if (channelAtOpen.current !== expectedChannelId) finish(); }, [expectedChannelId, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step]);

  useEffect(() => {
    if (busy !== "verify" || !review) return;
    const reviewId = review.reviewId;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const updated = await loadSourceRelinkReview(reviewId, expectedChannelId);
        if (active && !closed.current && pending.current === "verify" && updated.channelId === expectedChannelId && updated.reviewId === reviewId) setReview(updated);
      } catch { /* The verification command returns the authoritative failure. */ }
      if (active) timer = setTimeout(() => { void poll(); }, 750);
    };
    timer = setTimeout(() => { void poll(); }, 750);
    return () => { active = false; clearTimeout(timer); };
  }, [busy, review?.reviewId, expectedChannelId]);

  const choose = async () => {
    if (pending.current || closed.current) return;
    pending.current = "choose"; setBusy("choose"); setError("");
    const current = fence.begin();
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selectedPath = await open({ directory: mode === "folder", multiple: false });
      if (current() && !closed.current && typeof selectedPath === "string") setPath(selectedPath);
    } catch (failure) { if (current() && !closed.current) setError(workspaceErrorMessage(failure, "The location could not be chosen.")); }
    finally { pending.current = ""; if (current() && !closed.current) setBusy(""); }
  };

  const prepare = async () => {
    if (pending.current || closed.current) return;
    pending.current = "prepare"; setBusy("prepare"); setError("");
    const owned = fence.begin(); const current = () => owned() && !closed.current;
    try {
      release();
      const prepared = await prepareRelinkForChannel(initialItems.current.map((item) => item.id), expectedChannelId, path, mode, current);
      if (prepared && current()) { reviewRef.current = prepared; setReview(prepared); setApproved(false); setSelected(new Set()); setPage(1); setStep("verify"); }
    } catch (failure) { if (current()) setError(workspaceErrorMessage(failure, "The location could not be reviewed.")); }
    finally { pending.current = ""; if (current()) setBusy(""); }
  };

  const verify = async () => {
    if (pending.current || closed.current || !review) return;
    pending.current = "verify"; setBusy("verify"); setError("");
    const owned = fence.begin(); const current = () => owned() && !closed.current;
    try {
      const verified = await verifyRelinkForChannel(review, expectedChannelId, current);
      if (!verified || !current()) return;
      reviewRef.current = verified; setReview(verified); setSelected(new Set(verified.files.filter((file) => file.status === "verified").map((file) => file.itemId))); setApproved(false);
    } catch (failure) { if (current()) setError(workspaceErrorMessage(failure, "The originals could not be verified.")); }
    finally {
      pending.current = "";
      if (current()) setBusy("");
      else if (!closed.current && channelAtOpen.current !== currentChannel.current) { setBusy(""); finish(); }
    }
  };

  const save = async () => {
    if (pending.current || closed.current || !review) return;
    pending.current = "save"; setBusy("save"); setError("");
    const owned = fence.begin(); const current = () => owned() && !closed.current;
    try {
      const receipt = await saveRelinkForChannel(review, expectedChannelId, [...selected], resume, approved, current);
      if (!receipt || !current()) return;
      reviewRef.current = { ...review, state: "committed" };
      pending.current = "";
      callbacks.current.onSaved(receipt);
      finish();
    } catch (failure) { if (current()) setError(workspaceErrorMessage(failure, "Locations were not saved. Review the result before retrying.")); }
    finally {
      pending.current = "";
      if (current()) setBusy("");
      else if (!closed.current && channelAtOpen.current !== currentChannel.current) { setBusy(""); finish(); }
    }
  };

  const back = () => {
    if (pending.current) return;
    setApproved(false); setError(""); setPage(1);
    if (step === "review") setStep("verify");
    else { release(); setReview(undefined); setStep("location"); }
  };

  return <dialog ref={dialog} aria-modal="true" aria-labelledby={`${id}-title`} aria-describedby={`${id}-description`} className="m-auto max-h-[calc(100dvh-2rem)] w-[min(52rem,calc(100vw-2rem))] max-w-none overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55" onCancel={(event) => { event.preventDefault(); if (canClose) finish(); }}>
    <div className="grid gap-5 p-5 sm:p-6">
      <header className="flex items-start justify-between gap-4"><div className="min-w-0"><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted">Locate originals · {channelName || "Connected channel"}</p><h2 ref={heading} tabIndex={-1} id={`${id}-title`} className="m-0 text-xl font-semibold outline-none">{details.heading}</h2><p id={`${id}-description`} className="mt-2 mb-0 max-w-prose text-sm leading-relaxed text-muted">{details.description}</p></div><button type="button" aria-label="Close original location review" disabled={!canClose} className={`${secondary} shrink-0 px-3`} onClick={finish}><X aria-hidden="true" size={18} /></button></header>
      <ol aria-label="Locate originals steps" className="m-0 grid list-none grid-cols-3 gap-2 p-0">{steps.map((candidate, index) => <li key={candidate.key} aria-current={candidate.key === step ? "step" : undefined} className={`rounded-md border px-3 py-2 text-xs font-semibold ${candidate.key === step ? "border-brand bg-subtle-surface text-ink" : "border-line text-muted"}`}><span className="mr-1">{index + 1}.</span>{candidate.label}</li>)}</ol>
      {error ? <p role="alert" className="m-0 rounded-lg border border-line bg-subtle-surface p-4 text-sm leading-relaxed">{error}</p> : null}
      {step === "location" ? <div className="grid gap-4">
        <p className="m-0 text-sm">{initialItems.current.length} existing {initialItems.current.length === 1 ? "upload" : "uploads"} selected. Upload IDs and saved progress stay the same.</p>
        {initialItems.current.length === 1 ? <label className="grid gap-2 text-sm font-semibold">Replacement location<select className="min-h-11 rounded-md border border-line bg-control px-3 text-sm text-ink" value={mode} disabled={Boolean(busy)} onChange={(event) => { setMode(event.target.value as SourceRelinkMode); setPath(""); }}><option value="file">One original file</option><option value="folder">Moved folder</option></select></label> : null}
        <div className="grid gap-3 rounded-lg border border-line bg-subtle-surface p-4"><strong className="text-sm">{mode === "folder" ? "Replacement folder" : "Replacement original"}</strong>{path ? <p className="m-0 break-all text-sm">{path}</p> : <p className="m-0 text-sm text-muted">Choose the location containing the same original bytes.</p>}<button className={`${secondary} justify-self-start`} disabled={Boolean(busy)} type="button" onClick={() => { void choose(); }}><FolderOpen aria-hidden="true" size={18} />{mode === "folder" ? "Choose folder" : "Choose original file"}</button></div>
        {mode === "folder" ? <p className="m-0 text-sm leading-relaxed text-muted">The shared previous folder is mapped to this location, keeping each video's relative path. Verify the proposed paths on the next step.</p> : null}
        <p className="m-0 flex items-start gap-2 text-sm leading-relaxed text-muted"><ShieldCheck aria-hidden="true" size={18} className="mt-0.5 shrink-0" /><span>Only a trusted full content match can repair a location. No video copies are made. Watched-folder roots and their approvals are unchanged.</span></p>
      </div> : null}
      {review && step !== "location" ? <SourceRelinkFileList review={review} page={page} onPage={setPage} selected={selected} onSelect={step === "review" && !busy ? (itemId, checked) => { setSelected((current) => { const next = new Set(current); if (checked) next.add(itemId); else next.delete(itemId); return next; }); setApproved(false); } : undefined} /> : null}
      {step === "verify" && review ? <div className="grid gap-3"><p className="m-0 text-sm leading-relaxed text-muted">Files without a saved digest, unavailable replacements, and different bytes remain blocked. Cancel keeps all saved locations and progress.</p>{busy === "verify" ? <p role="status" className="m-0 flex items-center gap-2 text-sm font-semibold"><LoaderCircle aria-hidden="true" size={18} className="animate-spin" />Checking full file contents… {review.verifiedCount + review.blockedCount} of {review.files.length} checked</p> : <button type="button" className={`${secondary} justify-self-start`} onClick={() => { void verify(); }} disabled={Boolean(busy)}><FileSearch aria-hidden="true" size={18} />{review.state === "verified" ? "Verify again" : "Verify originals"}</button>}</div> : null}
      {step === "review" ? <div className="grid gap-4 rounded-lg border border-line bg-subtle-surface p-4"><p className="m-0 text-sm leading-relaxed">{selected.size} verified {selected.size === 1 ? "location" : "locations"} selected for {review?.channelName || channelName || "this channel"}. Choices, confirmed bytes and YouTube video receipts are preserved.</p><label className="flex items-start gap-3 text-sm leading-relaxed"><input className="mt-1 h-5 w-5 shrink-0 accent-brand" type="checkbox" checked={resume} disabled={Boolean(busy)} onChange={(event) => { setResume(event.target.checked); setApproved(false); }} /><span>Resume eligible uploads after saving. Saved YouTube sessions reconcile before transfer; uncertain progress without a session stays for attention. Drafts, cancelled items and completed videos keep their existing state.</span></label><label className="flex items-start gap-3 text-sm font-semibold leading-relaxed"><input className="mt-1 h-5 w-5 shrink-0 accent-brand" type="checkbox" checked={approved} disabled={Boolean(busy)} onChange={(event) => setApproved(event.target.checked)} /><span>I approve these verified locations for the selected existing uploads.</span></label><p className="m-0 text-xs leading-relaxed text-muted">File contents are checked again when saving. An expired review, changed file or changed upload requires a fresh review.</p></div> : null}
      <footer className="flex flex-wrap justify-between gap-3 border-t border-line pt-4"><div>{step !== "location" ? <button type="button" className={secondary} disabled={Boolean(busy)} onClick={back}><ArrowLeft aria-hidden="true" size={17} />Back</button> : null}</div><div className="flex flex-wrap gap-3"><button type="button" className={secondary} disabled={!canClose} onClick={finish}>Cancel</button>{step === "location" ? <button type="button" className={primary} disabled={!path || Boolean(busy) || initialItems.current.length < 1 || initialItems.current.length > 100} onClick={() => { void prepare(); }}>{busy === "prepare" ? <LoaderCircle aria-hidden="true" size={17} className="animate-spin" /> : <ArrowRight aria-hidden="true" size={17} />}Next: Verify files</button> : step === "verify" ? <button type="button" className={primary} disabled={Boolean(busy) || review?.state !== "verified" || review.verifiedCount === 0} onClick={() => { setPage(1); setStep("review"); }}><ArrowRight aria-hidden="true" size={17} />Next: Review</button> : <button type="button" className={primary} disabled={Boolean(busy) || !approved || selected.size === 0} onClick={() => { void save(); }}>{busy === "save" ? <LoaderCircle aria-hidden="true" size={17} className="animate-spin" /> : <ShieldCheck aria-hidden="true" size={17} />}{busy === "save" ? "Rechecking and saving…" : "Save repaired locations"}</button>}</div></footer>
    </div>
  </dialog>;
}
