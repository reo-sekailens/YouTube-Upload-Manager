import { lazy, Suspense, useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { FileCheck2, FolderSearch, ListChecks, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import type { BatchImportReceipt, ConnectionSettings, UploadItem } from "../lib/types";
import { isTauri, loadConnectionSettings, resumeQueuedUploads } from "../lib/local";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import { loadUploadCompletionBatches, type UploadCompletionBatch as CompletionBatch, type UploadCompletionBatchPage as CompletionBatchPage } from "../lib/upload-completion";
import "./BatchWorkflowTools.lazy.css";

const SourceRelinkDialog = lazy(() => import("./SourceRelinkDialog").then((m) => ({ default: m.SourceRelinkDialog })));
const UploadCompletionDialog = lazy(() => import("./UploadCompletionDialog").then((m) => ({ default: m.UploadCompletionDialog })));
const UploadPlanDialog = lazy(() => import("./UploadPlanDialog").then((m) => ({ default: m.UploadPlanDialog })));
const QuotaPlannerDialog = lazy(() => import("./QuotaPlannerDialog").then((m) => ({ default: m.QuotaPlannerDialog })));
const UploadControlsDialog = lazy(() => import("./UploadControlsDialog").then((m) => ({ default: m.UploadControlsDialog })));
const VideoAssetsDialog = lazy(() => import("./VideoAssetsDialog").then((m) => ({ default: m.VideoAssetsDialog })));
const PublicationCalendarDialog = lazy(() => import("./PublicationCalendarDialog").then((m) => ({ default: m.PublicationCalendarDialog })));
const DesktopPresenceDialog = lazy(() => import("./DesktopPresenceDialog").then((m) => ({ default: m.DesktopPresenceDialog })));
const ChannelProfilesDialog = lazy(() => import("./ChannelProfilesDialog").then((m) => ({ default: m.ChannelProfilesDialog })));
const PerceptualReviewDialog = lazy(() => import("./PerceptualReviewDialog").then((m) => ({ default: m.PerceptualReviewDialog })));
const button = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";

type Props = { items: UploadItem[]; channelId?: string; channelName?: string; busy: boolean; onRefresh: () => Promise<void>; onNotice: (message: string) => void; onConnectionChanged?: (settings?: ConnectionSettings) => void | Promise<void> };
type Action = "plan" | "completionScope" | "completion" | "selection" | "relink" | "quota" | "controls" | "assetSelection" | "assets" | "publication" | "desktop" | "profiles" | "perceptual";
type OpenWorkflow = { channelId?: string; action: Action; generation: number; scopeRevision: number };
type SelectionOptions = { label: string; title: string; description: string; continueLabel: string; limit: number };
const originalSelection: SelectionOptions = { label: "Locate originals", title: "Select uploads to repair", description: "Verification checks that the replacements contain the same original bytes before any location is changed.", continueLabel: "Choose replacement location", limit: 100 };
const assetSelection: SelectionOptions = { label: "Video attachments", title: "Select videos for attachments", description: "Match original thumbnail and caption files to these saved uploads. Attachments wait for each upload’s confirmed YouTube video.", continueLabel: "Choose thumbnails and captions", limit: 100 };
type CompletionScope = { uploadBatchId?: string };
const completionBatchPageSize = 25;

export async function loadCompletionScopesForChannel(expected: string, offset: number, current: () => boolean): Promise<CompletionBatchPage | undefined> {
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return undefined;
  const page = await loadUploadCompletionBatches(expected, offset, completionBatchPageSize);
  if (!current()) return undefined;
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return undefined;
  if (page.channelId !== expected || page.offset !== offset || page.limit !== completionBatchPageSize || !Number.isSafeInteger(page.totalBatches) || page.totalBatches < page.batches.length || page.batches.length > completionBatchPageSize || new Set(page.batches.map((batch) => batch.batchId)).size !== page.batches.length || page.batches.some((batch) => !/^[A-Za-z0-9_-]{1,128}$/.test(batch.batchId) || !Number.isSafeInteger(batch.itemCount) || batch.itemCount < 1 || !Number.isFinite(new Date(batch.createdAt).getTime()))) throw new Error("Saved batch scopes did not match this channel. Refresh them before opening a report.");
  return page;
}

export function CompletionReportScopeReview({ batch, channelId, channelName }: { batch?: CompletionBatch; channelId: string; channelName?: string }) {
  return <section aria-label="Reviewed completion report scope" className="grid min-w-0 gap-3 rounded-lg border border-line bg-subtle-surface p-4">
    <h3 className="m-0 text-base font-semibold">{batch ? "One saved upload batch" : "All uploads in this channel"}</h3>
    <p className="m-0 break-words text-sm">{channelName || "Selected YouTube channel"}<span className="mt-1 block break-all text-xs text-muted">{channelId}</span></p>
    {batch ? <><p className="m-0 break-words text-sm font-semibold">{batch.title || "Saved upload batch"}</p><p className="m-0 text-sm text-muted">{batch.itemCount} retained uploads · {new Date(batch.createdAt).toLocaleString()}</p><p className="m-0 break-all text-xs text-muted">Batch {batch.batchId}</p><p className="m-0 text-sm leading-relaxed">The report reads every retained upload in this saved batch, including cancelled uploads. Each follow-up action still requires its own review.</p></> : <p className="m-0 text-sm leading-relaxed">The report includes this channel’s saved uploads, including cancelled uploads and older uploads without a saved batch. Other channels stay separate.</p>}
    <p className="m-0 text-xs leading-relaxed text-muted">Opening a report reads saved evidence. It does not retry uploads or apply follow-up changes.</p>
  </section>;
}

export function CompletionReportScopeDialog({ expectedChannelId, channelName, onClose, onSelected }: { expectedChannelId: string; channelName?: string; onClose: () => void; onSelected: (scope: CompletionScope) => void }) {
  const id = useId(); const dialog = useRef<HTMLDialogElement>(null); const heading = useRef<HTMLHeadingElement>(null);
  const openedChannel = useRef(expectedChannelId); const callbacks = useRef({ onClose, onSelected }); callbacks.current = { onClose, onSelected };
  const closed = useRef(false); const pending = useRef(false); const fence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState(0); const [page, setPage] = useState<CompletionBatchPage>(); const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<CompletionBatch>(); const [busy, setBusy] = useState(true); const [error, setError] = useState("");
  const finish = useCallback(() => { if (closed.current) return; closed.current = true; fence.invalidate(); dialog.current?.close(); callbacks.current.onClose(); }, [fence]);
  const load = useCallback(async () => {
    if (pending.current || closed.current || openedChannel.current !== expectedChannelId) return;
    pending.current = true; setBusy(true); setError(""); const valid = fence.begin(); const current = () => valid() && !closed.current;
    try { const loaded = await loadCompletionScopesForChannel(expectedChannelId, offset, current); if (loaded) setPage(loaded); }
    catch (problem) { if (current()) { setPage(undefined); setError(workspaceErrorMessage(problem, "Saved report scopes could not be loaded. Refresh this channel before continuing.")); } }
    finally { pending.current = false; if (current()) setBusy(false); }
  }, [expectedChannelId, offset, fence]);
  useEffect(() => { const element = dialog.current; const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined; element?.showModal(); heading.current?.focus(); return () => { element?.close(); fence.invalidate(); if (previous?.isConnected) previous.focus(); }; }, [fence]);
  useEffect(() => { let cancelled = false; queueMicrotask(() => { if (!cancelled) void load(); }); return () => { cancelled = true; }; }, [load]);
  useEffect(() => { if (openedChannel.current !== expectedChannelId) finish(); }, [expectedChannelId, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step]);
  const loadedPage = page?.offset === offset ? page : undefined;
  return <dialog ref={dialog} aria-modal="true" aria-labelledby={`${id}-heading`} aria-describedby={`${id}-description`} className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-3xl overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55" onCancel={(event) => { event.preventDefault(); finish(); }}>
    <header className="flex items-start justify-between gap-4 border-b border-line p-4 sm:p-6"><div className="min-w-0"><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted">Upload completion</p><h2 ref={heading} tabIndex={-1} id={`${id}-heading`} className="m-0 text-lg font-bold focus:outline-none">{step ? "Review completion report scope" : "Choose completion report scope"}</h2><p id={`${id}-description`} className="mt-2 mb-0 max-w-xl text-sm leading-relaxed text-muted">Open a report for the selected channel or one complete saved upload batch.</p></div><button className={`${button} shrink-0 px-2`} type="button" aria-label="Close completion report scope" onClick={finish}><X size={20} aria-hidden="true" /></button></header>
    <ol aria-label="Completion report scope steps" className="m-0 flex list-none gap-4 border-b border-line bg-subtle-surface px-4 py-3 text-sm sm:px-6"><li aria-current={!step ? "step" : undefined}>1. Choose scope</li><li aria-current={step ? "step" : undefined}>2. Review scope</li></ol>
    <div className="grid min-w-0 gap-4 p-4 sm:p-6" aria-busy={busy}>
      {busy ? <p role="status" className="m-0 text-sm text-muted">Reading saved batches for this channel…</p> : null}
      {error ? <div role="alert" className="rounded-lg bg-warning-surface p-4 text-sm leading-relaxed text-warning-ink">{error}<button className={`${button} mt-3`} disabled={busy} type="button" onClick={() => { setStep(0); setSelected(undefined); void load(); }}>Refresh report scopes</button></div> : null}
      {step ? <CompletionReportScopeReview batch={selected} channelId={expectedChannelId} channelName={channelName} /> : <><label className="flex items-start gap-3 rounded-lg border border-line bg-subtle-surface p-4 text-sm"><input className="mt-1 h-5 w-5 shrink-0 accent-brand" type="radio" name={`${id}-scope`} checked={!selected} disabled={busy || !loadedPage} onChange={() => setSelected(undefined)} /><span className="min-w-0"><strong className="block">All uploads in this channel</strong><span className="mt-1 block leading-relaxed text-muted">Includes cancelled uploads and older uploads without a saved batch.</span></span></label>
        <h3 className="m-0 text-sm font-semibold">Saved upload batches</h3>
        {loadedPage?.batches.length ? <ul className="m-0 grid list-none gap-2 p-0">{loadedPage.batches.map((batch) => <li key={batch.batchId}><label className="flex items-start gap-3 rounded-lg border border-line p-4 text-sm"><input className="mt-1 h-5 w-5 shrink-0 accent-brand" type="radio" name={`${id}-scope`} checked={selected?.batchId === batch.batchId} disabled={busy} onChange={() => setSelected(batch)} /><span className="min-w-0"><strong className="block break-words">{batch.title || "Saved upload batch"}</strong><span className="mt-1 block text-muted">{batch.itemCount} retained uploads · {new Date(batch.createdAt).toLocaleString()}</span><span className="mt-1 block break-all text-xs text-muted">Batch {batch.batchId}</span></span></label></li>)}</ul> : loadedPage ? <p className="m-0 text-sm leading-relaxed text-muted">No saved batches on this page. Older uploads remain available in the all-channel report.</p> : null}
        {loadedPage && loadedPage.totalBatches > completionBatchPageSize ? <div className="flex flex-wrap items-center justify-between gap-2"><button className={button} type="button" disabled={busy || offset === 0} onClick={() => setOffset((old) => Math.max(0, old - completionBatchPageSize))}>Previous batches</button><span className="text-sm text-muted">Page {Math.floor(offset / completionBatchPageSize) + 1} of {Math.ceil(loadedPage.totalBatches / completionBatchPageSize)}</span><button className={button} type="button" disabled={busy || offset + completionBatchPageSize >= loadedPage.totalBatches} onClick={() => setOffset((old) => old + completionBatchPageSize)}>Next batches</button></div> : null}
        {selected ? <p className="m-0 break-words text-sm font-semibold">Selected batch: {selected.title || selected.batchId}</p> : null}</>}
    </div>
    <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-line p-4 sm:px-6"><button className={button} type="button" onClick={() => step ? setStep(0) : finish()}>{step ? "Back" : "Cancel"}</button><button className={primary} type="button" disabled={busy || !loadedPage || Boolean(error)} onClick={() => step ? callbacks.current.onSelected(selected ? { uploadBatchId: selected.batchId } : {}) : setStep(1)}>{step ? "Open completion report" : "Next: Review report scope"}</button></footer>
  </dialog>;
}

/** Native scope checks remain authoritative; late completions cannot update another visible workspace. */
export async function executeReviewedUploadPlan(planId: string, expected: string, current: () => boolean, refresh: () => Promise<void>, notice: (message: string) => void) {
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return;
  const receipt = await invoke<BatchImportReceipt>("import_upload_plan", { planId, expectedChannelId: expected });
  if (!current()) return;
  await refresh();
  if (current()) notice(`${receipt.importedCount} reviewed originals saved; ${receipt.queuedCount} queued, ${receipt.duplicateCount} need duplicate review, ${receipt.failedCount} need attention.`);
}

export async function refreshReviewedWorkflow(current: () => boolean, refresh: () => Promise<void>, notice: (message: string) => void, message?: string) {
  if (!current()) return;
  try { await refresh(); if (current() && message) notice(message); }
  catch (error) { if (current()) notice(workspaceErrorMessage(error, "Refresh the queue to see the saved workflow result.")); }
}

export function reviewedProfileTransitionCurrent(previous: string | undefined, approvedNext: string | undefined, openedRevision: number, current: string | undefined, revision: number) {
  return (revision === openedRevision && current === previous) || (revision === openedRevision + 1 && current === approvedNext);
}

export function OriginalSelectionDialog({ items, onClose, onSelected, options = originalSelection }: { items: UploadItem[]; onClose: () => void; onSelected: (ids: string[]) => void; options?: SelectionOptions }) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [review, setReview] = useState(false);
  const availableIds = new Set(items.map((item) => item.id));
  const selectedIds = [...selected].filter((itemId) => availableIds.has(itemId));
  const matches = items.filter((item) => item.title.toLowerCase().includes(query.toLowerCase()) || item.fileName.toLowerCase().includes(query.toLowerCase()));
  const visible = review ? items.filter((item) => selected.has(item.id)) : matches;
  const pages = Math.max(1, Math.ceil(visible.length / 25));
  const rows = visible.slice((Math.min(page, pages) - 1) * 25, Math.min(page, pages) * 25);
  useEffect(() => { const element = dialog.current; const previous = document.activeElement; element?.showModal(); return () => { element?.close(); if (previous instanceof HTMLElement && previous.isConnected) previous.focus(); }; }, []);
  return <dialog ref={dialog} aria-modal="true" aria-labelledby={id} onCancel={(event) => { event.preventDefault(); onClose(); }} className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-2xl overflow-y-auto rounded-xl border border-line bg-surface p-5 text-ink shadow-2xl backdrop:bg-black/55">
    <header className="flex items-start justify-between gap-4"><div><p className="mt-0 mb-2 text-xs font-semibold uppercase text-muted">{options.label}</p><h2 id={id} className="m-0 text-lg font-bold">{review ? "Review selected uploads" : options.title}</h2></div><button className={button} aria-label="Close upload selection" type="button" onClick={onClose}><X aria-hidden="true" size={18} /></button></header>
    <ol aria-label="Upload selection steps" className="my-4 flex list-none gap-4 p-0 text-sm"><li aria-current={!review ? "step" : undefined}>1. Select uploads</li><li aria-current={review ? "step" : undefined}>2. Review selection</li></ol>
    <p className="text-sm leading-relaxed text-muted">Choose up to {options.limit} saved uploads. {options.description}</p>
    {!review ? <label className="grid gap-2 text-sm font-semibold">Find an upload<input className="min-h-11 rounded-md border border-line bg-control px-3 text-ink" value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} /></label> : null}
    <ul className="my-4 grid list-none gap-2 p-0">{rows.map((item) => <li key={item.id} className="rounded-lg border border-line bg-subtle-surface p-3"><label className="flex items-start gap-3 text-sm"><input className="mt-1 accent-brand" type="checkbox" checked={selected.has(item.id)} disabled={review || (!selected.has(item.id) && selectedIds.length >= options.limit)} onChange={(event) => setSelected((old) => { const next = new Set([...old].filter((itemId) => availableIds.has(itemId))); if (event.target.checked && next.size < options.limit) next.add(item.id); else next.delete(item.id); return next; })} /><span className="min-w-0"><strong className="block break-words">{item.title}</strong><span className="mt-1 block break-all text-xs text-muted">{item.fileName} · {item.status.replaceAll("_", " ")}</span></span></label></li>)}</ul>
    {!rows.length ? <p className="text-sm text-muted">No saved uploads match this selection.</p> : null}
    {pages > 1 ? <div className="flex flex-wrap items-center justify-between gap-2"><button className={button} disabled={page <= 1} onClick={() => setPage((old) => old - 1)}>Previous page</button><span className="text-sm text-muted">Page {Math.min(page, pages)} of {pages}</span><button className={button} disabled={page >= pages} onClick={() => setPage((old) => old + 1)}>Next page</button></div> : null}
    <p className="text-sm font-semibold">{selectedIds.length} selected</p>
    <footer className="flex flex-wrap justify-end gap-2 border-t border-line pt-4">{review ? <button className={button} type="button" onClick={() => { setReview(false); setPage(1); }}>Back</button> : null}<button className={button} type="button" onClick={onClose}>Cancel</button><button className={primary} disabled={!selectedIds.length || selectedIds.length > options.limit} type="button" onClick={() => review ? onSelected(selectedIds) : (setReview(true), setPage(1))}>{review ? options.continueLabel : "Next: Review selection"}</button></footer>
  </dialog>;
}

export function BatchWorkflowTools({ items, channelId, channelName, busy, onRefresh, onNotice, onConnectionChanged }: Props) {
  const [open, setOpen] = useState<OpenWorkflow>();
  const [selection, setSelection] = useState<string[]>([]);
  const [completionScope, setCompletionScope] = useState<CompletionScope>({});
  const currentChannel = useRef(channelId); currentChannel.current = channelId;
  const scope = useRef({ channelId, revision: 0 });
  if (scope.current.channelId !== channelId) scope.current = { channelId, revision: scope.current.revision + 1 };
  const currentOpen = useRef(open); currentOpen.current = open;
  const generation = useRef(0); const mounted = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; generation.current += 1; }; }, []);
  const selected = useMemo(() => { const ids = new Set(selection); return items.filter((item) => ids.has(item.id)); }, [items, selection]);
  const repairable = items.filter((item) => item.sourceDeleteStatus !== "deleted" && !["importing", "uploading", "dispatching"].includes(item.status));
  const assetItems = items.filter((item) => item.status !== "importing");
  useEffect(() => { setOpen(undefined); setSelection([]); setCompletionScope({}); }, [channelId]);
  const active = open?.channelId === channelId ? open : undefined;
  const isCurrent = (workflow: OpenWorkflow) => mounted.current && scope.current.revision === workflow.scopeRevision && currentChannel.current === workflow.channelId && currentOpen.current?.generation === workflow.generation && generation.current === workflow.generation;
  // Dialogs report a confirmed save before closing. Keep that one refresh valid
  // after close, while a channel change or another workflow still revokes it.
  const isSavedCurrent = (workflow: OpenWorkflow) => mounted.current && scope.current.revision === workflow.scopeRevision && currentChannel.current === workflow.channelId && ((currentOpen.current?.generation === workflow.generation && generation.current === workflow.generation) || (!currentOpen.current && generation.current === workflow.generation + 1));
  const close = () => { generation.current += 1; currentOpen.current = undefined; setOpen(undefined); };
  const closeFor = (workflow: OpenWorkflow) => { if (isCurrent(workflow)) close(); };
  const start = (action: Action) => {
    if (busy || !isTauri || (!channelId && !["profiles", "desktop"].includes(action))) return;
    const next = { channelId, action, generation: ++generation.current, scopeRevision: scope.current.revision }; currentOpen.current = next; setOpen(next);
  };
  const changed = (workflow: OpenWorkflow, message?: string) => { if (isCurrent(workflow)) void refreshReviewedWorkflow(() => isSavedCurrent(workflow), onRefresh, onNotice, message); };
  const profilesChanged = (workflow: OpenWorkflow, settings?: ConnectionSettings) => {
    if (!isCurrent(workflow)) return;
    close(); const requestGeneration = generation.current;
    const expectedNext = settings ? (settings.connected ? settings.activeChannelId : undefined) : workflow.channelId;
    const current = () => mounted.current && generation.current === requestGeneration && reviewedProfileTransitionCurrent(workflow.channelId, expectedNext, workflow.scopeRevision, currentChannel.current, scope.current.revision);
    void (async () => {
      if (onConnectionChanged) await onConnectionChanged(settings);
      if (!current()) return;
      await onRefresh();
      if (current()) onNotice("Saved channel profiles refreshed. Each channel keeps its own queue and approvals.");
    })().catch((error) => { if (current()) onNotice(workspaceErrorMessage(error, "The profile change was saved. Refresh the account and queue to see its current state.")); });
  };
  const unavailable = busy || !isTauri;
  const scopedUnavailable = unavailable || !channelId;
  return <section aria-label="Batch planning and recovery" className="my-5 grid gap-3 rounded-lg border border-line bg-subtle-surface p-4">
    <div><h3 className="m-0 text-sm font-semibold">Batch tools</h3><p className="mt-1 mb-0 max-w-2xl text-sm leading-relaxed text-muted">Review upload choices, follow-up steps and device settings in guided dialogs.</p></div>
    <div className="grid gap-4 sm:grid-cols-2">
      <div className="grid content-start gap-2"><h4 className="m-0 text-xs font-semibold uppercase tracking-wide text-muted">Plan and transfer</h4><div className="flex flex-wrap gap-2"><button type="button" className={button} disabled={scopedUnavailable} onClick={() => start("plan")}><FileCheck2 aria-hidden="true" size={18} />Preview upload batch</button><button type="button" className={button} disabled={scopedUnavailable} onClick={() => start("quota")}>Estimate API quota</button><button type="button" className={button} disabled={scopedUnavailable} onClick={() => start("controls")}>Upload speed, hours and priority</button><button type="button" className={button} disabled={scopedUnavailable} onClick={() => start("publication")}>Publication calendar</button></div></div>
      <div className="grid content-start gap-2"><h4 className="m-0 text-xs font-semibold uppercase tracking-wide text-muted">Saved uploads</h4><div className="flex flex-wrap gap-2"><button type="button" className={button} disabled={scopedUnavailable || !repairable.length} onClick={() => start("selection")}><FolderSearch aria-hidden="true" size={18} />Locate moved originals</button><button type="button" className={button} disabled={scopedUnavailable} onClick={() => { setCompletionScope({}); start("completionScope"); }}><ListChecks aria-hidden="true" size={18} />Upload completion</button><button type="button" className={button} disabled={scopedUnavailable || !assetItems.length} onClick={() => start("assetSelection")}>Add thumbnails and captions</button><button type="button" className={button} disabled={scopedUnavailable || !repairable.length} onClick={() => start("perceptual")}>Compare original videos</button></div></div>
    </div>
    <div className="flex flex-wrap items-center gap-2 border-t border-line pt-3"><span className="mr-1 text-xs font-semibold uppercase tracking-wide text-muted">This device</span><button type="button" className={button} disabled={unavailable} onClick={() => start("profiles")}>Saved channel profiles</button><button type="button" className={button} disabled={unavailable} onClick={() => start("desktop")}>Desktop activity and startup</button></div>
    {!channelId ? <p className="m-0 text-sm leading-relaxed text-muted">Select or connect a YouTube channel to use batch tools. Saved profiles and desktop settings are available on this device.</p> : null}
    <Suspense fallback={<p role="status" className="m-0 text-sm text-muted">Loading reviewed workflow…</p>}>
      {active?.action === "selection" ? <OriginalSelectionDialog items={repairable} onClose={() => closeFor(active)} onSelected={(ids) => { if (!isCurrent(active)) return; setSelection(ids); start("relink"); }} /> : null}
      {active?.action === "assetSelection" ? <OriginalSelectionDialog items={assetItems} options={assetSelection} onClose={() => closeFor(active)} onSelected={(ids) => { if (!isCurrent(active)) return; setSelection(ids); start("assets"); }} /> : null}
      {active?.action === "relink" && active.channelId ? <SourceRelinkDialog items={selected} expectedChannelId={active.channelId} channelName={channelName} onClose={() => closeFor(active)} onSaved={(receipt) => { if (!isCurrent(active) || receipt.channelId !== active.channelId) return; void (async () => { await onRefresh(); if (!isSavedCurrent(active)) return; if (receipt.resumedItemIds.length) { assertWorkspaceChannel(await loadConnectionSettings(), receipt.channelId); if (!isSavedCurrent(active)) return; await resumeQueuedUploads(); if (!isSavedCurrent(active)) return; await onRefresh(); } if (isSavedCurrent(active)) onNotice(receipt.detail); })().catch((error) => { if (isSavedCurrent(active)) onNotice(workspaceErrorMessage(error, "Original locations were saved; refresh the queue to check resumed work.")); }); }} /> : null}
      {active?.action === "completionScope" && active.channelId ? <CompletionReportScopeDialog expectedChannelId={active.channelId} channelName={channelName} onClose={() => closeFor(active)} onSelected={(reportScope) => { if (!isCurrent(active)) return; setCompletionScope(reportScope); start("completion"); }} /> : null}
      {active?.action === "completion" && active.channelId ? <UploadCompletionDialog expectedChannelId={active.channelId} uploadBatchId={completionScope.uploadBatchId} onClose={() => closeFor(active)} onChanged={() => changed(active)} /> : null}
      {active?.action === "plan" && active.channelId ? <UploadPlanDialog expectedChannelId={active.channelId} activeChannel={channelName} onClose={() => closeFor(active)} onExecute={(planId, expected) => executeReviewedUploadPlan(planId, expected, () => isCurrent(active), onRefresh, onNotice)} /> : null}
      {active?.action === "quota" && active.channelId ? <QuotaPlannerDialog expectedChannelId={active.channelId} activeChannel={channelName} onClose={() => closeFor(active)} /> : null}
      {active?.action === "controls" && active.channelId ? <UploadControlsDialog expectedChannelId={active.channelId} onClose={() => closeFor(active)} onSaved={() => changed(active, "Reviewed upload speed, hours and priorities saved on this device.")} /> : null}
      {active?.action === "assets" && active.channelId ? <VideoAssetsDialog expectedChannelId={active.channelId} activeChannel={channelName} itemIds={selection} onClose={() => closeFor(active)} onSaved={() => changed(active, "Reviewed thumbnail and caption jobs saved for the selected uploads.")} /> : null}
      {active?.action === "publication" && active.channelId ? <PublicationCalendarDialog expectedChannelId={active.channelId} onClose={() => closeFor(active)} onSaved={() => changed(active, "Reviewed publication times saved for this channel.")} /> : null}
      {active?.action === "perceptual" && active.channelId ? <PerceptualReviewDialog expectedChannelId={active.channelId} channelName={channelName} items={repairable} onClose={() => closeFor(active)} /> : null}
      {active?.action === "desktop" ? <DesktopPresenceDialog expectedChannelId={active.channelId} onClose={() => closeFor(active)} onSaved={() => changed(active, "Desktop choices saved on this device.")} /> : null}
      {active?.action === "profiles" ? <ChannelProfilesDialog expectedChannelId={active.channelId} activeChannel={channelName} onClose={() => closeFor(active)} onChanged={(settings) => profilesChanged(active, settings)} /> : null}
    </Suspense>
  </section>;
}
