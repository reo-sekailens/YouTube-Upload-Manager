import { ArrowLeft, ArrowRight, CheckCircle2, Download, LoaderCircle, RefreshCw, ShieldCheck, Wrench, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { beginDeletionAuthorization, enableDeletionSudoMode, loadConnectionSettings } from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import { checkUploadCompletion, completionOutcomeLabels, completionStepLabels, exportUploadCompletion, loadUploadCompletion, repairUploadCompletion, type CompletionRepairStep, type CompletionReviewedJob, type CompletionStep, type UploadCompletionItem, type UploadCompletionPage } from "../lib/upload-completion";
import type { ConnectionSettings } from "../lib/types";
import "./UploadCompletionDialog.lazy.css";

export type UploadCompletionDialogProps = { expectedChannelId: string; itemIds?: string[]; uploadBatchId?: string; onClose: () => void; onChanged?: () => void };
const button = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const PAGE_SIZE = 25;
const managementSteps: CompletionRepairStep[] = ["playlist", "caption", "publication"];
function hasRetainedRepairSnapshot(step: CompletionStep) {
  if (step.key === "playlist" || step.key === "source_cleanup") return true;
  if (step.key === "thumbnail" || step.key === "caption") {
    const ids = step.repairJobIds;
    return Boolean(ids?.length && ids.length <= 400 && ids.every((id) => typeof id === "string" && id.length > 0) && new Set(ids).size === ids.length);
  }
  if (step.key === "publication") return Boolean(step.publicationSnapshot?.publishAt && step.publicationSnapshot.updatedAt);
  return false;
}
export function repairableCompletionSteps(item?: UploadCompletionItem): CompletionRepairStep[] {
  return item?.steps.filter((step) => step.repairable && hasRetainedRepairSnapshot(step)).map((step) => step.key as CompletionRepairStep) ?? [];
}
export function reviewedCompletionJobs(item: UploadCompletionItem, steps: CompletionRepairStep[]): CompletionReviewedJob[] {
  return steps.flatMap((key): CompletionReviewedJob[] => {
    const step = item.steps.find((candidate) => candidate.key === key);
    if (key === "thumbnail" || key === "caption") return [{ key, jobIds: [...(step?.repairJobIds ?? [])] }];
    if (key === "publication") return [{ key, publishAt: step?.publicationSnapshot?.publishAt, updatedAt: step?.publicationSnapshot?.updatedAt }];
    return [];
  });
}
export function completionRepairValid(item: UploadCompletionItem | undefined, steps: CompletionRepairStep[], confirmation: string, approved: boolean) {
  const eligible = repairableCompletionSteps(item);
  return Boolean(item?.videoId && approved && steps.length && new Set(steps).size === steps.length && steps.every((step) => eligible.includes(step)) && (!steps.includes("source_cleanup") || confirmation === item.fileName));
}
export function completionRepairDescription(item: UploadCompletionItem, key: CompletionRepairStep) {
  const step = item.steps.find((candidate) => candidate.key === key);
  if (key === "playlist") return `Check membership in ${item.playlistTitle || "the saved playlist"} before adding this same video.`;
  if (key === "source_cleanup") return "Recheck YouTube processing and original file identity before deleting the unchanged original.";
  if (key === "thumbnail") return "Reapply only the reviewed thumbnail job using its unchanged original image. Later attachments require a separate review.";
  if (key === "caption") return `Check only the ${step?.repairJobIds?.length ?? 0} reviewed caption job${step?.repairJobIds?.length === 1 ? "" : "s"} before any insertion. Existing or uncertain tracks are reconciled; no caption is replaced or deleted.`;
  return `Recheck the retained publication time, ${step?.publicationSnapshot?.publishAt ?? "unavailable"}, for this private video. A changed or elapsed time requires a new calendar review.`;
}

export function assertCompletionReportScope(itemIds?: string[], uploadBatchId?: string) {
  if (itemIds !== undefined && uploadBatchId !== undefined) throw new Error("Choose a saved batch or selected uploads, rather than combining report scopes.");
  if (uploadBatchId !== undefined && !uploadBatchId.trim()) throw new Error("Choose a valid saved upload batch before opening its report.");
}
export function completionReportScopeLabel(itemIds?: string[], uploadBatchId?: string, total?: number) {
  if (uploadBatchId !== undefined) return total === undefined ? "the selected saved upload batch" : `all ${total} upload${total === 1 ? "" : "s"} in the selected saved batch`;
  return itemIds !== undefined ? "the selected uploads" : "all uploads for this connected channel";
}
export async function loadCompletionForChannel(channelId: string, itemIds: string[] | undefined, offset: number, current: () => boolean, uploadBatchId?: string) {
  assertCompletionReportScope(itemIds, uploadBatchId);
  const settings = await loadConnectionSettings();
  assertWorkspaceChannel(settings, channelId);
  if (!current()) return undefined;
  const page = await loadUploadCompletion(channelId, itemIds, offset, PAGE_SIZE, uploadBatchId);
  if (!current()) return undefined;
  if (page.channelId !== channelId || page.uploadBatchId !== uploadBatchId || page.items.some((item) => item.channelId !== channelId || (itemIds && !itemIds.includes(item.itemId))))
    throw new Error("Completion results belong to another channel or selection. Reopen this checklist.");
  return { page, settings };
}
export async function exportCompletionForChannel(channelId: string, format: "json" | "csv", path: string, itemIds: string[] | undefined, current: () => boolean, uploadBatchId?: string) {
  assertCompletionReportScope(itemIds, uploadBatchId);
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const receipt = await exportUploadCompletion(channelId, format, path, itemIds, uploadBatchId);
  if (!current()) return undefined;
  if (receipt.uploadBatchId !== uploadBatchId || receipt.format !== format)
    throw new Error("The export receipt did not match the reviewed report scope. Reopen this checklist.");
  return receipt;
}

export async function repairCompletionForChannel(channelId: string, item: UploadCompletionItem | undefined, steps: CompletionRepairStep[], confirmation: string, approved: boolean, current: () => boolean) {
  if (!completionRepairValid(item, steps, confirmation, approved) || !item?.videoId || item.channelId !== channelId)
    throw new Error("Review and approve the eligible follow-up steps for this saved video first.");
  const selectedSteps = [...steps];
  const reviewedJobs = reviewedCompletionJobs(item, selectedSteps);
  const reviewedItemId = item.itemId;
  const reviewedVideoId = item.videoId;
  const settings = await loadConnectionSettings(); assertWorkspaceChannel(settings, channelId);
  if (!current()) return undefined;
  if (selectedSteps.some((key) => managementSteps.includes(key)) && (!settings.deletionAuthorized || !settings.deletionSudoActive))
    throw new Error("Grant management permission and enter its temporary mode before repairing the selected video follow-ups.");
  const result = await repairUploadCompletion(channelId, reviewedItemId, reviewedVideoId, selectedSteps, selectedSteps.includes("source_cleanup") ? confirmation : undefined, reviewedJobs.length ? reviewedJobs : undefined);
  if (!current()) return undefined;
  if (result.channelId !== channelId || result.itemId !== reviewedItemId || result.videoId !== reviewedVideoId)
    throw new Error("The repair receipt did not match the reviewed video. Refresh this checklist.");
  return result;
}

export function CompletionItemChecklist({ item, onRepair }: { item: UploadCompletionItem; onRepair?: () => void }) {
  return <article className="grid min-w-0 gap-4 rounded-lg border border-line bg-subtle-surface p-4">
    <div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><h3 className="m-0 break-words text-sm font-bold">{item.title}</h3><p className="mt-1 mb-0 break-words text-xs text-muted">{item.videoId ? `YouTube video: ${item.videoId}` : "Awaiting a confirmed YouTube video"}</p></div>{onRepair && repairableCompletionSteps(item).length ? <button className={button} onClick={onRepair} type="button"><Wrench aria-hidden="true" size={16} />Review repair</button> : null}</div>
    <dl className="m-0 grid min-w-0 gap-3 sm:grid-cols-2">{item.steps.map((step) => <div className="min-w-0" key={step.key}><dt className="flex flex-wrap items-baseline justify-between gap-2 text-xs font-semibold"><span>{completionStepLabels[step.key]}</span><span className={step.outcome === "succeeded" ? "text-success-ink" : step.outcome === "failed" || step.outcome === "unknown" ? "text-warning-ink" : "text-muted"}>{completionOutcomeLabels[step.outcome]}</span></dt><dd className="mt-1 ml-0 break-words text-xs leading-relaxed text-muted">{step.detail}{step.checkedAt ? <span className="mt-1 block">Checked {new Date(step.checkedAt).toLocaleString()}</span> : null}</dd></div>)}</dl>
  </article>;
}

export function CompletionPermissionControls({ permission, busy, onAction }: { permission?: ConnectionSettings; busy: boolean; onAction: (action: "grant" | "enable" | "refresh") => void }) {
  const ready = permission?.deletionAuthorized === true && permission.deletionSudoActive === true;
  return <section aria-label="Follow-up repair permission" className="grid gap-3 rounded-lg bg-subtle-surface p-4"><h3 className="m-0 flex items-center gap-2 text-sm font-semibold"><ShieldCheck aria-hidden="true" size={18} />Video-management permission</h3><p className="m-0 text-sm leading-relaxed text-muted">{ready ? "Temporary management mode is ready. Pending video follow-ups can continue for this channel." : "Grant Google’s management permission and activate its 15-minute mode before a playlist, caption or publication write."}</p><div className="flex flex-wrap gap-2">{!permission?.deletionAuthorized ? <button className={button} disabled={busy} onClick={() => onAction("grant")} type="button">Grant management permission</button> : !ready ? <button className={button} disabled={busy} onClick={() => onAction("enable")} type="button">Enter management mode</button> : null}<button className={button} disabled={busy} onClick={() => onAction("refresh")} type="button">Refresh permission</button></div></section>;
}

export function UploadCompletionDialog({ expectedChannelId, itemIds, uploadBatchId, onClose, onChanged }: UploadCompletionDialogProps) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const selectedIds = itemIds?.join("\n");
  const scopeKey = JSON.stringify([expectedChannelId, uploadBatchId ?? null, selectedIds ?? null]);
  const openedScope = useRef(scopeKey);
  const callbacks = useRef({ onClose, onChanged }); callbacks.current = { onClose, onChanged };
  const closed = useRef(false);
  const pending = useRef(false);
  const fence = useWorkspaceRequestFence(scopeKey);
  const [page, setPage] = useState<UploadCompletionPage>();
  const [offset, setOffset] = useState(0);
  const [step, setStep] = useState<"checklist" | "choices" | "review">("checklist");
  const [repairItem, setRepairItem] = useState<UploadCompletionItem>();
  const [steps, setSteps] = useState<CompletionRepairStep[]>([]);
  const [confirmation, setConfirmation] = useState("");
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [permission, setPermission] = useState<ConnectionSettings>();
  const [exportFormat, setExportFormat] = useState<"json" | "csv">("json");
  const [exportReview, setExportReview] = useState(false);

  const finish = useCallback(() => { if (closed.current) return; closed.current = true; fence.invalidate(); dialog.current?.close(); callbacks.current.onClose(); }, [fence]);
  useEffect(() => {
    const element = dialog.current;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    element?.showModal(); heading.current?.focus();
    return () => { element?.close(); if (previous?.isConnected) previous.focus(); };
  }, []);
  useEffect(() => { if (scopeKey !== openedScope.current) finish(); }, [scopeKey, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step, exportReview]);

  const run = useCallback(async (operation: (current: () => boolean) => Promise<void>) => {
    if (pending.current || closed.current) return;
    pending.current = true; setBusy(true); setError("");
    const request = fence.begin(); const current = () => !closed.current && request();
    try { await operation(current); } catch (failure) { if (current()) setError(workspaceErrorMessage(failure, "This completion action could not finish. Refresh the checklist to review retained receipts.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  }, [fence]);
  const load = useCallback(() => run(async (current) => {
    const result = await loadCompletionForChannel(expectedChannelId, selectedIds?.split("\n"), offset, current, uploadBatchId);
    if (result && current()) {
      setPage(result.page); setPermission(result.settings);
      setRepairItem(undefined); setSteps([]); setConfirmation(""); setApproved(false); setStep("checklist");
      heading.current?.focus(); dialog.current?.scrollTo({ top: 0 });
    }
  }), [expectedChannelId, selectedIds, uploadBatchId, offset, run]);
  useEffect(() => { let cancelled = false; queueMicrotask(() => { if (!cancelled) void load(); }); return () => { cancelled = true; }; }, [load]);

  const refreshProcessing = useCallback(() => run(async (current) => {
    const ids = page?.items.filter((item) => item.videoId && item.steps.some((outcome) => outcome.key === "processing" && (outcome.outcome === "waiting" || outcome.outcome === "unknown"))).map((item) => item.itemId) ?? [];
    if (!ids.length) return;
    assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId); if (!current()) return;
    await checkUploadCompletion(expectedChannelId, ids); if (!current()) return;
    const result = await loadCompletionForChannel(expectedChannelId, selectedIds?.split("\n"), offset, current, uploadBatchId);
    if (result && current()) { setPage(result.page); setPermission(result.settings); callbacks.current.onChanged?.(); }
  }), [page, run, expectedChannelId, selectedIds, uploadBatchId, offset]);
  useEffect(() => {
    if (step !== "checklist" || exportReview || !page?.items.some((item) => item.videoId && item.steps.some((outcome) => outcome.key === "processing" && ["waiting", "unknown"].includes(outcome.outcome)))) return;
    const timer = window.setTimeout(() => void refreshProcessing(), 30_000);
    return () => window.clearTimeout(timer);
  }, [page, step, exportReview, refreshProcessing]);

  const permissionAction = (action: "grant" | "enable" | "refresh") => run(async (current) => {
    let settings = await loadConnectionSettings(); assertWorkspaceChannel(settings, expectedChannelId); if (!current()) return;
    if (action === "grant") { const { authorizationUrl } = await beginDeletionAuthorization(); assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId); if (!current()) return; await openAndCopyGoogleAuthorization(authorizationUrl); if (current()) setNotice("Complete Google's approval, return here, then refresh permission."); }
    else if (action === "enable") { settings = await enableDeletionSudoMode(); assertWorkspaceChannel(settings, expectedChannelId); }
    if (current()) { setPermission(settings); callbacks.current.onChanged?.(); }
  });
  const repair = () => run(async (current) => {
    if (!completionRepairValid(repairItem, steps, confirmation, approved) || !repairItem?.videoId) return;
    const result = await repairCompletionForChannel(expectedChannelId, repairItem, steps, confirmation, approved, current);
    if (!result || !current()) return;
    setPage((previous) => previous ? { ...previous, items: previous.items.map((item) => item.itemId === result.itemId ? result : item) } : previous);
    setRepairItem(undefined); setStep("checklist"); setNotice("The selected follow-up steps were checked. Review their individual outcomes below."); callbacks.current.onChanged?.();
  });
  const exportReport = () => run(async (current) => {
    assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId); if (!current()) return;
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ title: "Save completion report", defaultPath: `youtube-completion.${exportFormat}`, filters: [{ name: exportFormat.toUpperCase(), extensions: [exportFormat] }] });
    if (!path || !current()) return;
    const receipt = await exportCompletionForChannel(expectedChannelId, exportFormat, path, selectedIds?.split("\n"), current, uploadBatchId);
    if (receipt && current()) { setExportReview(false); setNotice(`Saved completion outcomes for ${receipt.count} upload${receipt.count === 1 ? "" : "s"}${uploadBatchId ? " in the selected saved batch" : ""}.`); }
  });
  const permissionReady = permission?.deletionAuthorized === true && permission.deletionSudoActive === true;
  const needsManagement = steps.some((key) => managementSteps.includes(key));
  const repairValid = completionRepairValid(repairItem, steps, confirmation, approved) && (!needsManagement || permissionReady);
  const label = exportReview ? "Review completion report" : step === "checklist" ? "Upload completion checklist" : step === "choices" ? "Choose follow-up steps" : "Review the existing video";

  return <dialog aria-describedby={`${id}-description`} aria-labelledby={`${id}-heading`} aria-modal="true" className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-3xl overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55" onCancel={(event) => { event.preventDefault(); if (!pending.current) finish(); }} ref={dialog}>
    <header className="flex items-start justify-between gap-4 border-b border-line p-4 sm:p-6"><div className="min-w-0"><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted uppercase">Completion & repair</p><h2 className="m-0 text-lg font-bold tracking-tight focus:outline-none" id={`${id}-heading`} ref={heading} tabIndex={-1}>{label}</h2><p className="mt-2 mb-0 max-w-xl text-sm leading-relaxed text-muted" id={`${id}-description`}>Keep the confirmed upload receipt while checking or repairing its requested follow-up steps.</p></div><button aria-label="Close completion checklist" className={`${button} shrink-0 px-2`} disabled={busy} onClick={finish} type="button"><X aria-hidden="true" size={20} /></button></header>
    <ol aria-label="Completion repair steps" className="m-0 flex list-none gap-3 border-b border-line bg-subtle-surface px-4 py-3 text-xs sm:px-6">{["Checklist", "Repair choices", "Review"].map((text, index) => <li aria-current={(exportReview ? index === 2 : index === ["checklist", "choices", "review"].indexOf(step)) ? "step" : undefined} className="flex min-w-0 flex-1 items-center gap-2" key={text}><span aria-hidden="true" className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-line bg-surface">{index + 1}</span><span>{text}</span></li>)}</ol>
    <div aria-busy={busy} className="grid min-w-0 gap-4 p-4 sm:p-6">
      {error ? <div className="rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">{error}<button className={`${button} mt-3`} disabled={busy} onClick={() => void load()} type="button">Refresh checklist</button></div> : null}
      {notice ? <p className="m-0 text-sm text-muted" role="status">{notice}</p> : null}
      {busy ? <p className="m-0 flex items-center gap-2 text-sm text-muted" role="status"><LoaderCircle aria-hidden="true" className="animate-spin" size={16} />Checking retained completion receipts…</p> : null}
      {exportReview ? <section className="grid gap-4"><p className="m-0 text-sm leading-relaxed">Export {completionReportScopeLabel(itemIds, uploadBatchId, page?.total)} with their separate completion outcomes. The report excludes credentials, resumable sessions, filenames and source locations.</p><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-format`}>Report format<select className="min-h-11 rounded-md border border-line bg-control px-3 text-ink" disabled={busy} id={`${id}-format`} onChange={(event) => setExportFormat(event.target.value as "json" | "csv")} value={exportFormat}><option value="json">JSON</option><option value="csv">CSV</option></select></label><p className="m-0 text-xs leading-relaxed text-muted">Reports contain titles and YouTube IDs. Choose a new filename; an existing report is never overwritten.</p></section> : step === "checklist" ? <>
        <div className="flex flex-wrap items-center justify-between gap-3"><p className="m-0 text-sm text-muted">{page ? `${page.total} upload${page.total === 1 ? "" : "s"}${uploadBatchId ? " in the selected saved batch" : ""} · ${page.pendingProcessing} awaiting processing evidence` : uploadBatchId ? "Loading this saved upload batch…" : "Loading this channel’s saved uploads…"}</p><div className="flex flex-wrap gap-2"><button className={button} disabled={busy || !page?.items.some((item) => item.videoId && item.steps.some((s) => s.key === "processing" && ["unknown", "waiting"].includes(s.outcome)))} onClick={() => void refreshProcessing()} type="button"><RefreshCw aria-hidden="true" size={16} />Check processing</button><button className={button} disabled={busy || !page?.total} onClick={() => { setExportReview(true); setError(""); }} type="button"><Download aria-hidden="true" size={16} />Export report</button></div></div>
        {page?.items.some((item) => item.steps.some((outcome) => managementSteps.includes(outcome.key as CompletionRepairStep) && ["waiting", "failed", "unknown"].includes(outcome.outcome))) ? <CompletionPermissionControls permission={permission} busy={busy} onAction={(action) => void permissionAction(action)} /> : null}
        {page?.items.map((item) => <CompletionItemChecklist item={item} key={item.itemId} onRepair={busy ? undefined : () => { setRepairItem(item); setSteps(repairableCompletionSteps(item)); setConfirmation(""); setApproved(false); setError(""); setStep("choices"); }} />)}
        {page && !page.items.length ? <div className="rounded-lg bg-subtle-surface p-6"><CheckCircle2 aria-hidden="true" className="text-muted" size={24} /><p className="mb-0 text-sm text-muted">No saved uploads are available in this selection.</p></div> : null}
        {page && page.total > PAGE_SIZE ? <nav aria-label="Completion pages" className="flex items-center justify-between gap-3"><button className={button} disabled={busy || !offset} onClick={() => setOffset((value) => Math.max(0, value - PAGE_SIZE))} type="button">Previous page</button><span className="text-xs text-muted">{offset + 1}–{Math.min(offset + PAGE_SIZE, page.total)} of {page.total}</span><button className={button} disabled={busy || offset + PAGE_SIZE >= page.total} onClick={() => setOffset((value) => value + PAGE_SIZE)} type="button">Next page</button></nav> : null}
        <p className="m-0 text-xs leading-relaxed text-muted">Only pending videos on this page are checked, at most once every 30 seconds while this checklist is open. Terminal processing results stop polling. Processing success does not establish Studio copyright checks.</p>
      </> : repairItem ? <>
        <div className="rounded-lg bg-subtle-surface p-4"><p className="m-0 break-words text-sm font-bold">{repairItem.title}</p><p className="mt-2 mb-0 break-words text-xs text-muted">Channel {expectedChannelId} · Existing YouTube video {repairItem.videoId}</p></div>
        {step === "choices" ? <fieldset className="m-0 grid gap-3 border-0 p-0" disabled={busy}><legend className="mb-3 text-sm font-semibold">Follow-up steps to repair</legend>{repairableCompletionSteps(repairItem).map((key) => <label className="flex items-start gap-3 rounded-lg border border-line p-4 text-sm" key={key}><input checked={steps.includes(key)} className="mt-1 accent-brand" onChange={(event) => { setSteps((previous) => event.target.checked ? [...previous, key] : previous.filter((value) => value !== key)); setApproved(false); }} type="checkbox" /><span className="min-w-0"><strong className="block">{completionStepLabels[key]}</strong><span className="mt-1 block leading-relaxed text-muted">{completionRepairDescription(repairItem, key)}</span></span></label>)}</fieldset> : <><dl className="m-0 grid gap-3 rounded-lg border border-line p-4 text-sm"><div><dt className="text-xs text-muted">Approved repairs</dt><dd className="mt-1 ml-0 font-semibold">{steps.map((key) => completionStepLabels[key]).join(" and ")}</dd></div>{steps.includes("playlist") ? <div><dt className="text-xs text-muted">Saved playlist</dt><dd className="mt-1 ml-0 break-words">{repairItem.playlistTitle || repairItem.playlistId}</dd></div> : null}{steps.filter((key) => key === "thumbnail" || key === "caption" || key === "publication").map((key) => <div key={key}><dt className="text-xs text-muted">{completionStepLabels[key]}</dt><dd className="mt-1 ml-0 break-words leading-relaxed">{completionRepairDescription(repairItem, key)}</dd></div>)}</dl>
          {needsManagement ? <CompletionPermissionControls permission={permission} busy={busy} onAction={(action) => void permissionAction(action)} /> : null}
          {steps.includes("source_cleanup") ? <label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-confirm`}>Type the original filename: <span className="break-words font-normal">{repairItem.fileName}</span><input autoComplete="off" className="min-h-11 w-full rounded-md border border-line bg-control px-3 py-2 text-base text-ink" disabled={busy} id={`${id}-confirm`} onChange={(event) => setConfirmation(event.target.value)} value={confirmation} /></label> : null}
          <label className="flex items-start gap-3 text-sm leading-relaxed"><input checked={approved} className="mt-1 accent-brand" disabled={busy} onChange={(event) => setApproved(event.target.checked)} type="checkbox" /><span>I approve these follow-up steps for this existing video{steps.includes("source_cleanup") ? ", including deletion of its unchanged original after verified processing" : ""}.</span></label>
        </>}
      </> : null}
    </div>
    <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-line p-4 sm:px-6"><button className={button} disabled={busy} onClick={() => { if (exportReview) setExportReview(false); else if (step === "review") { setStep("choices"); setApproved(false); } else if (step === "choices") setStep("checklist"); else finish(); setError(""); }} type="button">{step === "checklist" && !exportReview ? "Close" : <><ArrowLeft aria-hidden="true" size={16} />Back</>}</button>{exportReview ? <button className={primary} disabled={busy} onClick={() => void exportReport()} type="button"><Download aria-hidden="true" size={16} />Choose where to save</button> : step === "choices" ? <button className={primary} disabled={busy || !steps.length} onClick={() => { setStep("review"); setApproved(false); }} type="button">Next: Review<ArrowRight aria-hidden="true" size={16} /></button> : step === "review" ? <button className={primary} disabled={busy || !repairValid} onClick={() => void repair()} type="button"><Wrench aria-hidden="true" size={16} />Repair selected steps</button> : null}</footer>
  </dialog>;
}
