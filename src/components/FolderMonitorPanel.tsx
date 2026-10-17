import { FolderOpen, ListFilter, Pause, Play, Plus, Settings2, Trash2 } from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "../lib/local";
import {
  disableFolderMonitor, enableFolderMonitor, loadFolderMonitors, loadFolderMonitorOverview,
  requeueCancelledFolderMonitorFiles, reconcileFolderMonitorUploads,
  deleteFolderMonitorUploadedSource, deleteFolderMonitorUploadedSources, removeFolderMonitor, scanFolderMonitorNow,
} from "../lib/folder-monitor";
import type { FolderMonitorSettings, FolderMonitorFileActivity, FolderMonitorLogEntry, FolderMonitorLocalDeleteResult } from "../lib/types";
import { subscribeLocalStateChanges } from "../lib/state-events";
import { createFolderMonitorRefreshQueue } from "../lib/folder-monitor-refresh";
import { useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import { FolderMonitorSetupDialog } from "./FolderMonitorSetupDialog";
import "./FolderMonitorPanel.lazy.css";

const FolderRulesDialog = lazy(() => import("./FolderRulesDialog").then((module) => ({ default: module.FolderRulesDialog })));

type FolderMonitorPanelProps = {
  activeChannel?: string;
  activeChannelId?: string;
  onNotice: (message: string) => void;
  onQueueRefresh: () => Promise<void>;
};

const buttonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primaryButtonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";

export function folderMonitorStatusLabel(settings: FolderMonitorSettings) {
  if (!settings.enabled) return "Paused";
  if (settings.status === "watching") return "Watching";
  if (settings.status === "scanning") return "Scanning";
  if (settings.status === "paused") return "Paused";
  if (settings.status === "error") return "Needs attention";
  return settings.status.replaceAll("_", " ");
}

export function folderObservationLabel(state: string) {
  const labels: Record<string,string> = { excluded: "Excluded by filename rules", retained_excluded: "Previously excluded original retained", waiting_ready_marker: "Waiting for producer-ready marker", invalid_ready_marker: "Invalid producer-ready marker", observed: "Changing or waiting for another unchanged scan" };
  return labels[state] || state.replaceAll("_", " ");
}

function folderName(path?: string) { return path?.split(/[\\/]/).filter(Boolean).at(-1) || "Watched folder"; }
function formatScanTime(value?: string) {
  if (!value) return "Not scanned yet";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
function formatBytes(value: number) {
  if (value < 1024) return value + " B";
  if (value < 1024 ** 2) return (value / 1024).toFixed(1) + " KB";
  if (value < 1024 ** 3) return (value / 1024 ** 2).toFixed(1) + " MB";
  return (value / 1024 ** 3).toFixed(1) + " GB";
}
function fileProgress(file: FolderMonitorFileActivity) {
  if (!file.totalBytes || file.totalBytes <= 0 || file.confirmedBytes === undefined) return undefined;
  return Math.min(100, Math.round((file.confirmedBytes / file.totalBytes) * 100));
}

export function FolderMonitorCards({ monitors, selectedId, busy, onSelect, onToggle, onEdit, onRemove }: {
  monitors: FolderMonitorSettings[]; selectedId?: string; busy: boolean;
  onSelect: (id: string) => void; onToggle: (monitor: FolderMonitorSettings) => void;
  onEdit: (monitor: FolderMonitorSettings) => void; onRemove: (monitor: FolderMonitorSettings) => void;
}) {
  return <ul aria-label="Watched folders" className="mt-5 mb-0 grid list-none gap-3 p-0 lg:grid-cols-2">{monitors.map((monitor) => {
    const selected = monitor.id === selectedId;
    const status = folderMonitorStatusLabel(monitor);
    return <li className={"min-w-0 rounded-lg border " + (selected ? "border-brand bg-active-surface" : "border-line bg-subtle-surface")} key={monitor.id}>
      <button aria-label={"View activity for " + monitor.folderPath} aria-pressed={selected} className="grid min-h-24 w-full gap-3 rounded-t-lg border-0 bg-transparent p-4 text-left text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring" disabled={busy} onClick={() => monitor.id && onSelect(monitor.id)} type="button">
        <span className="flex items-start justify-between gap-3"><span className="flex min-w-0 items-start gap-2"><FolderOpen aria-hidden="true" className="mt-0.5 shrink-0 text-muted" size={18} /><span className="min-w-0"><strong className="block break-words text-base">{folderName(monitor.folderPath)}</strong><span className="mt-1 block break-all text-xs leading-relaxed text-muted">{monitor.folderPath}</span></span></span><span className={"shrink-0 rounded-full px-2 py-1 text-xs font-semibold " + (monitor.enabled && monitor.status === "watching" ? "bg-subtle-surface text-success" : monitor.status === "error" ? "bg-warning-surface text-warning-ink" : "bg-control text-muted")}>{status}</span></span>
        <span className="block text-sm leading-relaxed text-muted"><span className="capitalize">{monitor.visibility}</span> · {monitor.madeForKids ? "Made for kids" : "Not made for kids"} · {monitor.playlistTitle || "No playlist"}</span>
        <span className="text-xs font-semibold text-active-ink">{selected ? "Viewing this folder’s activity" : "View folder activity"}</span>
      </button>
      <div className="flex flex-wrap gap-2 border-t border-line px-4 py-3"><button aria-label={(monitor.enabled ? "Pause " : "Resume ") + folderName(monitor.folderPath)} className={buttonClass} disabled={busy} onClick={() => onToggle(monitor)} type="button">{monitor.enabled ? <Pause aria-hidden="true" size={16} /> : <Play aria-hidden="true" size={16} />}{monitor.enabled ? "Pause" : "Resume"}</button><button aria-label={"Edit " + folderName(monitor.folderPath)} className={buttonClass} disabled={busy} onClick={() => onEdit(monitor)} type="button"><Settings2 aria-hidden="true" size={16} />Edit</button><button aria-label={"Remove " + folderName(monitor.folderPath)} className={buttonClass} disabled={busy} onClick={() => onRemove(monitor)} type="button"><Trash2 aria-hidden="true" size={16} />Remove</button></div>
    </li>;
  })}</ul>;
}

export function FolderMonitorConfirmationDialog({ monitor, onConfirm, onClose }: { monitor: FolderMonitorSettings; onConfirm: () => Promise<void>; onClose: () => void }) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const [review, setReview] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const pending = useRef(false);
  const closed = useRef(false);
  const callbacks = useRef({ onConfirm, onClose }); callbacks.current = { onConfirm, onClose };
  const finish = () => { if (closed.current) return; closed.current = true; dialog.current?.close(); callbacks.current.onClose(); };
  useEffect(() => { closed.current = false; const element = dialog.current; const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined; element?.showModal(); heading.current?.focus(); return () => { closed.current = true; element?.close(); if (previous?.isConnected) previous.focus(); }; }, []);
  useEffect(() => { heading.current?.focus(); }, [review]);
  const confirm = async () => { if (pending.current || closed.current) return; pending.current = true; setBusy(true); setError(""); try { await callbacks.current.onConfirm(); if (!closed.current) finish(); } catch (failure) { if (!closed.current) setError(workspaceErrorMessage(failure, "This folder’s action could not complete.")); } finally { pending.current = false; if (!closed.current) setBusy(false); } };
  return <dialog aria-describedby={id + "-description"} aria-labelledby={id + "-heading"} aria-modal="true" className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-lg overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55" onCancel={(event) => { event.preventDefault(); if (!pending.current) finish(); }} ref={dialog}>
    <header className="border-b border-line p-5"><h2 className="m-0 text-lg font-bold focus:outline-none" id={id + "-heading"} ref={heading} tabIndex={-1}>{review ? "Review folder removal" : "Confirm this folder"}</h2><p className="mt-2 mb-0 text-sm leading-relaxed text-muted" id={id + "-description"}>Remove only this folder from monitoring.</p></header>
    <ol aria-label="Folder action steps" className="m-0 flex list-none gap-4 border-b border-line bg-subtle-surface px-5 py-3 text-sm"><li aria-current={!review ? "step" : undefined} className={!review ? "font-semibold text-active-ink" : "text-muted"}>1. Folder</li><li aria-current={review ? "step" : undefined} className={review ? "font-semibold text-active-ink" : "text-muted"}>2. Review</li></ol>
    <div className="grid gap-4 p-5"><div className="rounded-lg border border-line bg-subtle-surface p-4"><strong className="block break-all text-sm">{monitor.folderPath}</strong><p className="mt-2 mb-0 text-sm text-muted">{monitor.channelName || "Connected channel"} · {monitor.visibility} · {monitor.madeForKids ? "Made for kids" : "Not made for kids"}</p></div>{review ? <p className="m-0 text-sm leading-relaxed text-muted">New discoveries will stop for this folder. Original files, existing upload queue items, completed videos and saved upload receipts are retained. Your other folders keep monitoring.</p> : null}{error ? <p className="m-0 text-sm text-danger" role="alert">{error}</p> : null}</div>
    <footer className="flex flex-wrap gap-2 border-t border-line bg-subtle-surface p-5">{review ? <button className={buttonClass} disabled={busy} onClick={() => setReview(false)} type="button">Back</button> : null}<span aria-hidden="true" className="flex-1" /><button className={buttonClass} disabled={busy} onClick={finish} type="button">Cancel</button><button className={primaryButtonClass} disabled={busy} onClick={() => review ? void confirm() : setReview(true)} type="button">{busy ? "Working…" : review ? "Remove folder" : "Next: Review"}</button></footer>
  </dialog>;
}

export function FolderMonitorPanel({ activeChannel, activeChannelId, onNotice, onQueueRefresh }: FolderMonitorPanelProps) {
  const [catalog, setCatalog] = useState<{ channelId?: string; monitors: FolderMonitorSettings[] }>({ monitors: [] });
  const [selectedId, setSelectedId] = useState<string>();
  const [detail, setDetail] = useState<{ channelId?: string; monitorId?: string; settings: FolderMonitorSettings; files: FolderMonitorFileActivity[]; logs: FolderMonitorLogEntry[] }>();
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState("");
  const requestRefresh = useRef<() => void>(() => undefined);
  const [setup, setSetup] = useState<{ channelId: string; monitor?: FolderMonitorSettings }>();
  const [confirmation, setConfirmation] = useState<{ channelId: string; monitor: FolderMonitorSettings }>();
  const [rulesReview, setRulesReview] = useState<{ channelId: string; monitor: FolderMonitorSettings }>();
  const [deletionQueue, setDeletionQueue] = useState<FolderMonitorFileActivity[]>([]);
  const [deletionConfirmation, setDeletionConfirmation] = useState("");
  const [deletionSubmitting, setDeletionSubmitting] = useState(false);
  const [deletionProgress, setDeletionProgress] = useState({ completed: 0, total: 0 });
  const [deletionResults, setDeletionResults] = useState<FolderMonitorLocalDeleteResult[]>([]);
  const readFence = useWorkspaceRequestFence(activeChannelId);
  const mutationFence = useWorkspaceRequestFence(activeChannelId);
  const pending = useRef(false);
  const deletionScope = useRef<{ monitorId: string; channelId: string; itemIds: Set<string> } | undefined>(undefined);
  const scopeRef = useRef(activeChannelId); scopeRef.current = activeChannelId;
  const selectedRef = useRef(selectedId); selectedRef.current = selectedId;
  const monitors = catalog.channelId === activeChannelId ? catalog.monitors : [];
  const selected = monitors.find((monitor) => monitor.id === selectedId);
  const settings = detail && detail.channelId === activeChannelId && detail.monitorId === selectedId ? detail.settings : undefined;
  const files = settings ? detail!.files : [];
  const logs = settings ? detail!.logs : [];
  const blocked = busy || deletionSubmitting || (loading && catalog.channelId !== activeChannelId);
  const loadedScope = useRef({ catalog, detail }); loadedScope.current = { catalog, detail };
  const visibleSetup = setup?.channelId === activeChannelId ? setup : undefined;
  const visibleConfirmation = confirmation?.channelId === activeChannelId ? confirmation : undefined;
  const visibleRules = rulesReview?.channelId === activeChannelId ? rulesReview : undefined;

  useEffect(() => { setSetup(undefined); setConfirmation(undefined); setRulesReview(undefined); setDeletionQueue([]); setDeletionResults([]); setDeletionConfirmation(""); setDeletionSubmitting(false); setBusy(false); deletionScope.current = undefined; }, [activeChannelId]);
  useEffect(() => { setDeletionQueue([]); setDeletionResults([]); setDeletionConfirmation(""); deletionScope.current = undefined; }, [selectedId]);

  useEffect(() => {
    if (!activeChannelId) { requestRefresh.current = () => undefined; setLoading(false); setLoadError(""); return; }
    let active = true;
    const previous = loadedScope.current;
    const hasDetail = previous.detail?.channelId === activeChannelId && previous.detail.monitorId === selectedId;
    setLoading(previous.catalog.channelId !== activeChannelId || Boolean(selectedId && !hasDetail));
    setLoadError("");
    const current = readFence.begin();
    const refresh = createFolderMonitorRefreshQueue(async () => {
      let changingSelection = false;
      try {
        const loaded = await loadFolderMonitors(activeChannelId);
        if (!active || !current()) return;
        if (loaded.monitors.some((monitor) => monitor.channelId !== activeChannelId || !monitor.id)) throw new Error("The watched folder list did not match the connected channel.");
        setCatalog({ channelId: activeChannelId, monitors: loaded.monitors });
        const nextId = loaded.monitors.some((monitor) => monitor.id === selectedId) ? selectedId : loaded.monitors[0]?.id;
        if (nextId !== selectedId) { changingSelection = true; setSelectedId(nextId); return; }
        if (nextId) {
          const overview = await loadFolderMonitorOverview(nextId, activeChannelId);
          if (!active || !current()) return;
          if (overview.settings.id !== nextId || overview.settings.channelId !== activeChannelId) throw new Error("The folder activity did not match the selected folder.");
          setDetail({ channelId: activeChannelId, monitorId: nextId, ...overview });
        } else setDetail(undefined);
        setLoadError("");
      } catch (failure) { if (active && current()) setLoadError(workspaceErrorMessage(failure, "Folder monitoring status could not be loaded from this device.")); }
      finally { if (active && current() && !changingSelection) setLoading(false); }
    }, (failure) => { if (active && current()) setLoadError(workspaceErrorMessage(failure, "Folder monitoring status could not be loaded from this device.")); });
    const request = () => {
      const known = loadedScope.current;
      if (known.catalog.channelId !== activeChannelId || (selectedId && (known.detail?.channelId !== activeChannelId || known.detail.monitorId !== selectedId))) setLoading(true);
      refresh.request();
    };
    requestRefresh.current = request;
    refresh.start();
    return () => { active = false; refresh.dispose(); if (requestRefresh.current === request) requestRefresh.current = () => undefined; };
  }, [activeChannelId, selectedId, readFence]);

  useEffect(() => {
    if (!isTauri || !activeChannelId) return;
    let active = true;
    let stop: (() => void) | undefined;
    void subscribeLocalStateChanges((batch) => {
      if (!active || (!batch.resetRequired && !batch.changes.some((change) => change.channelId === activeChannelId && ["folder_monitor", "monitor", "upload"].includes(change.surface)))) return;
      requestRefresh.current();
    }).then((unsubscribe) => { if (active) stop = unsubscribe; else unsubscribe(); }).catch(() => undefined);
    return () => { active = false; stop?.(); };
  }, [activeChannelId]);

  useEffect(() => {
    if (!isTauri) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<{ monitorId?: string; channelId?: string; completed: number; total: number; result: FolderMonitorLocalDeleteResult }>("folder-monitor-local-delete-progress", (event) => {
      const scope = deletionScope.current;
      const payload = event.payload;
      if (!active || !scope || scope.channelId !== scopeRef.current || payload.channelId !== scope.channelId || payload.monitorId !== scope.monitorId || !scope.itemIds.has(payload.result.itemId)) return;
      setDeletionProgress({ completed: payload.completed, total: payload.total });
      setDeletionResults((previous) => previous.some((result) => result.itemId === payload.result.itemId) ? previous : [...previous, payload.result]);
    }).then((stop) => { if (active) unlisten = stop; else stop(); }).catch(() => undefined);
    return () => { active = false; unlisten?.(); };
  }, []);

  const perform = useCallback(async (monitor: FolderMonitorSettings, operation: () => Promise<unknown>, notice: string, refreshQueue = false) => {
    if (!isTauri || !monitor.id || !activeChannelId || monitor.channelId !== activeChannelId || pending.current) throw new Error("This folder is unavailable or another folder action is still running.");
    pending.current = true;
    setBusy(true);
    const current = mutationFence.begin();
    try {
      await operation();
      if (!current()) return;
      requestRefresh.current();
      onNotice(notice);
      if (refreshQueue) {
        try { await onQueueRefresh(); }
        catch { if (current()) setLoadError("The folder action completed, but the upload queue could not be refreshed. Retry loading folders."); }
      }
    } catch (failure) {
      if (current()) throw failure;
    } finally { pending.current = false; if (current()) setBusy(false); }
  }, [activeChannelId, mutationFence, onNotice, onQueueRefresh]);
  const report = (action: Promise<unknown>) => void action.catch((failure) => onNotice(workspaceErrorMessage(failure, "This folder’s action could not complete.")));
  const toggle = (monitor: FolderMonitorSettings) => report(perform(monitor, () => monitor.enabled ? disableFolderMonitor(monitor.id, activeChannelId) : enableFolderMonitor(monitor.folderPath!, monitor.visibility, monitor.madeForKids, Boolean(monitor.deleteSourceAfterUpload), monitor.playlistId, monitor.playlistTitle, monitor.id, activeChannelId), monitor.enabled ? "This folder is paused. Its existing upload queue and originals are retained; other folders keep monitoring." : "This folder is monitoring again. Other folders were not changed."));
  const scanNow = () => { if (selected) report(perform(selected, () => scanFolderMonitorNow(selected.id, activeChannelId), "This folder’s scan finished.", true)); };

  const uploadingFiles = useMemo(() => files.filter((file) => ["importing", "dispatching", "uploading"].includes(file.uploadStatus ?? file.observationState)), [files]);
  const queuedFiles = useMemo(() => files.filter((file) => ["draft", "queued"].includes(file.uploadStatus ?? file.observationState)), [files]);
  const reconciliationFiles = useMemo(() => files.filter((file) => file.uploadStatus === "needs_reconciliation"), [files]);
  const cancelledFiles = useMemo(() => files.filter((file) => file.uploadStatus === "cancelled" && file.itemId && !["duplicate", "duplicate_title", "hash_failed", "rejected"].includes(file.observationState)), [files]);
  const uploadedFiles = useMemo(() => files.filter((file) => file.itemId && file.videoId && file.liveConfirmed && file.localSourceAvailable && file.sourceDeleteStatus !== "deleted" && file.uploadStatus === "uploaded"), [files]);
  const recentFiles = useMemo(() => files.slice(0, 24), [files]);

  const openDeletionReview = (chosen: FolderMonitorFileActivity[]) => { if (!isTauri || blocked || !selected || chosen.length === 0) return; setDeletionQueue(chosen); setDeletionConfirmation(""); setDeletionProgress({ completed: 0, total: chosen.length }); setDeletionResults([]); };
  const deleteLocalSource = async () => {
    const target = deletionQueue[0];
    if (!isTauri || !selected?.id || !activeChannelId || !target?.itemId || deletionSubmitting || pending.current) return;
    pending.current = true;
    const requestCurrent = mutationFence.begin();
    const monitorId = selected.id;
    const current = () => requestCurrent() && selectedRef.current === monitorId;
    const itemIds = deletionQueue.flatMap((file) => file.itemId ? [file.itemId] : []);
    deletionScope.current = { monitorId, channelId: activeChannelId, itemIds: new Set(itemIds) };
    setDeletionSubmitting(true); setDeletionProgress({ completed: 0, total: itemIds.length }); setDeletionResults([]);
    try {
      const results = deletionQueue.length > 1 ? await deleteFolderMonitorUploadedSources(itemIds, deletionConfirmation, monitorId, activeChannelId) : [await deleteFolderMonitorUploadedSource(target.itemId, deletionConfirmation, monitorId, activeChannelId)];
      if (!current()) return;
      setDeletionResults(results); setDeletionProgress({ completed: results.length, total: results.length }); setDeletionQueue([]); setDeletionConfirmation(""); requestRefresh.current();
      onNotice("Local source deletion finished for this folder. See the deletion log for any retained file.");
    } catch (failure) { if (current()) onNotice(workspaceErrorMessage(failure, "The local watched file could not be deleted.")); }
    finally { pending.current = false; deletionScope.current = undefined; if (current()) setDeletionSubmitting(false); }
  };
  const requeueCancelled = (itemIds: string[]) => { if (selected && itemIds.length) report(perform(selected, async () => { const count = await requeueCancelledFolderMonitorFiles(itemIds, selected.id, activeChannelId); return count; }, "Selected cancelled files were queued again for this folder.", true)); };
  const reconcileUploads = () => { if (selected && reconciliationFiles.length) report(perform(selected, () => reconcileFolderMonitorUploads(selected.id, activeChannelId), "Reconciliation finished for this folder. YouTube was checked before retrying any ambiguous upload.", true)); };

  return <section aria-labelledby="folder-monitor-heading" className="rounded-xl border border-line bg-surface p-4 text-ink sm:p-5">
    <header className="flex flex-wrap items-start justify-between gap-4"><div className="min-w-0"><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted uppercase">Opt-in automation</p><h2 className="m-0 text-lg font-bold tracking-tight" id="folder-monitor-heading">Watched folder uploads</h2><p className="mt-2 mb-0 max-w-2xl text-sm leading-relaxed text-muted">Monitor multiple local folders together while the app is running. Each folder keeps its own upload choices and activity.</p></div><button className={primaryButtonClass} disabled={!isTauri || !activeChannelId || blocked} onClick={() => activeChannelId && setSetup({ channelId: activeChannelId })} type="button"><Plus aria-hidden="true" size={18} />Add folder</button></header>
    {activeChannelId ? <p className="mt-4 mb-0 text-sm text-muted"><strong className="text-ink">{monitors.filter((monitor) => monitor.enabled).length} active</strong> · {monitors.length} {monitors.length === 1 ? "folder" : "folders"} · {activeChannel || "Connected channel"}</p> : <p className="mt-4 mb-0 rounded-lg bg-notice-surface p-4 text-sm text-notice-ink">Connect a YouTube channel before adding watched folders.</p>}
    {loadError ? <div className="mt-4 flex flex-wrap items-center gap-3 rounded-lg bg-warning-surface p-4"><p className="m-0 flex-1 text-sm text-warning-ink" role="alert">{loadError}</p><button className={buttonClass} disabled={busy || deletionSubmitting} onClick={() => requestRefresh.current()} type="button">Retry loading folders</button></div> : null}
    {loading ? <p className="mt-4 mb-0 text-sm text-muted" role="status">Loading watched folders…</p> : null}
    {monitors.length ? <FolderMonitorCards busy={blocked} monitors={monitors} onEdit={(monitor) => activeChannelId && setSetup({ channelId: activeChannelId, monitor })} onRemove={(monitor) => activeChannelId && setConfirmation({ channelId: activeChannelId, monitor })} onSelect={setSelectedId} onToggle={toggle} selectedId={selectedId} /> : !loading && activeChannelId && !loadError ? <div className="mt-5 rounded-lg border border-dashed border-line bg-subtle-surface p-5"><h3 className="m-0 text-base font-semibold">No watched folders yet</h3><p className="mt-2 mb-0 max-w-xl text-sm leading-relaxed text-muted">Add a folder and approve its channel, visibility, audience and optional cleanup. Other folders can be added with different choices.</p></div> : null}
    {settings ? <div aria-label="Selected folder details" className="mt-6 border-t border-line pt-5">
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><p className="mt-0 mb-1 text-xs font-semibold text-muted">Selected folder</p><h3 className="m-0 break-words text-base font-bold">{folderName(settings.folderPath)}</h3></div><div className="flex flex-wrap items-center gap-3"><span className="rounded-full bg-subtle-surface px-3 py-1 text-sm font-semibold text-muted">{folderMonitorStatusLabel(settings)}</span><button className={buttonClass} disabled={blocked || !isTauri || !activeChannelId || !settings.id} onClick={() => activeChannelId && setRulesReview({ channelId: activeChannelId, monitor: settings })} type="button"><ListFilter aria-hidden="true" size={16} />Rules & readiness</button></div></div>
          <dl className="grid grid-cols-2 gap-2.5 m-0 max-sm:grid-cols-1">
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Folder</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]" title={settings.folderPath}>
                {settings.folderPath ?? "Unavailable"}
              </dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Bound channel</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{settings.channelName ?? "Unavailable"}</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Visibility</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{settings.visibility}</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Audience</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">
                {settings.madeForKids ? "Made for kids" : "Not made for kids"}
              </dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Original source cleanup</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">
                {settings.deleteSourceAfterUpload
                  ? "Automatic after YouTube processing succeeds"
                  : "Keep original"}
              </dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Transfer source</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">Original watched file in place</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Playlist</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{settings.playlistTitle ?? "No playlist"}</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Last scan</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{formatScanTime(settings.lastScanAt)}</dd>
            </div>
            <div className="min-w-0 rounded-lg border border-[#e3e7ed] bg-[#fafbfc] px-3 py-2.5">
              <dt className="mb-1 text-[0.64rem] font-bold tracking-[0.07em] text-[#7a8799] uppercase">Last file</dt>
              <dd className="m-0 wrap-anywhere text-[0.78rem] text-[#34405a]">{settings.lastFileName ?? "No file processed yet"}</dd>
            </div>
          </dl>
          <p className="mt-3 mb-0 text-[0.76rem] leading-relaxed text-[#52617a]" role="status">
            {settings.detail}
          </p>
          <p className="mt-2 mb-0 text-[0.72rem] text-[#487057]">
            {settings.enabled ? "This folder continues monitoring while the app is running, even when you view another folder." : "This folder is paused. Existing uploads and receipts are retained; other folders keep monitoring."}
          </p>
          <section className="mt-3.5 rounded-lg border border-[#d8e5f8] bg-[#f8fbff] p-3" aria-labelledby="folder-monitor-live-heading">
            <div className="flex items-center justify-between gap-3 max-sm:flex-col max-sm:items-start max-sm:gap-1">
              <div>
                <p className="mb-0.5 text-[0.67rem] font-bold tracking-[0.1em] text-[#68748a] uppercase">THIS FOLDER</p>
                <h3 className="m-0 text-[0.92rem] text-[#25314a]" id="folder-monitor-live-heading">Live folder activity</h3>
              </div>
              <span className="text-[0.7rem] font-bold text-[#3d668e] whitespace-nowrap">{uploadingFiles.length} uploading · {queuedFiles.length} queued{reconciliationFiles.length > 0 ? ` · ${reconciliationFiles.length} to reconcile` : ""}</span>
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2.5 max-sm:grid-cols-1">
              <div className="min-w-0 rounded-md border border-[#e2e8f0] bg-white p-2.5">
                <h4 className="m-0 text-[0.72rem] text-[#384966]">Uploading now</h4>
                {uploadingFiles.length === 0 ? (
                  <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No files are currently being uploaded.</p>
                ) : (
                  <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                    {uploadingFiles.map((file) => {
                      const progress = fileProgress(file);
                      return (
                        <li className="border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={file.itemId || file.fileName}>
                          <div className="flex items-baseline justify-between gap-2">
                            <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]" title={file.fileName}>{file.fileName}</strong>
                            <span className="text-[0.66rem] text-[#4671a4] capitalize">{file.uploadStatus ?? file.observationState}</span>
                          </div>
                          <p className="mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{formatBytes(file.sizeBytes)}{progress === undefined ? "" : ` · ${progress}% uploaded`}</p>
                          {progress !== undefined && <progress className="mt-1 w-full accent-[#2463df]" aria-label={`${file.fileName} upload progress`} max={100} value={progress} />}
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
              <div className="min-w-0 rounded-md border border-[#e2e8f0] bg-white p-2.5">
                <h4 className="m-0 text-[0.72rem] text-[#384966]">Waiting in this folder’s queue</h4>
                {queuedFiles.length === 0 ? (
                  <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No watched-folder files are awaiting upload.</p>
                ) : (
                  <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                    {queuedFiles.map((file) => (
                      <li className="border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={file.itemId || file.fileName}>
                        <div className="flex items-baseline justify-between gap-2">
                          <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]" title={file.fileName}>{file.fileName}</strong>
                          <span className="text-[0.66rem] text-[#4671a4] capitalize">{file.uploadStatus ?? file.observationState}</span>
                        </div>
                        <p className="mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{formatBytes(file.sizeBytes)} · updated {formatScanTime(file.updatedAt)}</p>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
            {reconciliationFiles.length > 0 && (
              <div className="mt-2.5 rounded-md border border-[#f0d39e] bg-[#fffaf0] p-2.5">
                <div className="flex items-center justify-between gap-3 max-sm:flex-col max-sm:items-start">
                  <div>
                    <strong className="text-[0.72rem] text-[#805b16]">{reconciliationFiles.length} upload{reconciliationFiles.length === 1 ? " needs" : "s need"} reconciliation</strong>
                    <p className="mt-1 mb-0 text-[0.68rem] leading-snug text-[#806b43]">They are not treated as queued. Reconcile checks YouTube first, then retries only video IDs absent from the active channel.</p>
                  </div>
                  <button className="shrink-0 rounded-md border border-[#d9b970] bg-white px-3 py-2 text-[0.74rem] font-[680] text-[#765515] disabled:cursor-not-allowed disabled:opacity-50" disabled={busy} onClick={() => void reconcileUploads()} type="button">{busy ? "Checking YouTube…" : `Reconcile and upload (${reconciliationFiles.length})`}</button>
                </div>
              </div>
            )}
            <details className="mt-3 border-t border-[#dfe7f2] pt-2">
              <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Folder scan log ({logs.length})</summary>
              {logs.length === 0 ? (
                <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No scan events have been recorded for this folder yet.</p>
              ) : (
                <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                  {logs.map((log) => (
                    <li className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-2 border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={`${log.createdAt}-${log.kind}`}>
                      <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]">{log.kind.replaceAll("_", " ")}</strong>
                      <span className="text-[0.64rem] text-[#6e7d91] whitespace-nowrap">{formatScanTime(log.createdAt)}</span>
                      <p className="col-span-full mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{log.detail ?? "No further detail recorded."}</p>
                    </li>
                  ))}
                </ul>
              )}
            </details>
            <details className="mt-3 border-t border-[#dfe7f2] pt-2">
              <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Recently observed files ({files.length})</summary>
              <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                {recentFiles.map((file) => (
                  <li className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-2 border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={file.itemId || file.fileName}>
                    <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]">{file.fileName}</strong>
                    <span className="text-[0.64rem] text-[#6e7d91] whitespace-nowrap">{formatScanTime(file.updatedAt)}</span>
                    <p className="col-span-full mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{file.uploadTitle ? `Queued as ${file.uploadTitle}` : `${formatBytes(file.sizeBytes)} · ${folderObservationLabel(file.observationState)}`}</p>
                  </li>
                ))}
              </ul>
            </details>
            <details className="mt-3 border-t border-[#dfe7f2] pt-2">
              <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Cancelled watched files ({cancelledFiles.length})</summary>
              {cancelledFiles.length === 0 ? (
                <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No eligible cancelled watched-folder files are waiting to be queued again.</p>
              ) : (
                <>
                  <p className="my-2 text-[0.68rem] leading-snug text-[#68788e]">Cancelled files stay out of the queue until you explicitly add them back. Duplicate and integrity-safety stops cannot be requeued here.</p>
                  <button
                    className="rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] transition-colors hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={busy}
                    onClick={() => void requeueCancelled(cancelledFiles.flatMap((file) => file.itemId ? [file.itemId] : []))}
                    type="button"
                  >
                    {busy ? "Working…" : `Queue all ${cancelledFiles.length} again`}
                  </button>
                  <ul className="mt-2 grid max-h-56 list-none gap-1.5 overflow-auto p-0">
                    {cancelledFiles.map((file) => (
                      <li className="flex items-center justify-between gap-2 border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={file.itemId || file.fileName}>
                        <div className="min-w-0">
                          <div className="flex items-baseline justify-between gap-2">
                            <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]" title={file.fileName}>{file.fileName}</strong>
                            <span className="text-[0.66rem] text-[#4671a4] capitalize">cancelled</span>
                          </div>
                          <p className="mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{formatBytes(file.sizeBytes)} · cancelled {formatScanTime(file.updatedAt)}</p>
                        </div>
                        <button
                          className="shrink-0 rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] transition-colors hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-50"
                          disabled={busy}
                          onClick={() => void requeueCancelled(file.itemId ? [file.itemId] : [])}
                          type="button"
                        >
                          Queue again
                        </button>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </details>
            <details className="mt-4 border-t border-[#dfe7f2] pt-3">
              <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Uploaded to YouTube ({uploadedFiles.length})</summary>
              <p className="mt-2 mb-3 max-w-4xl text-[0.7rem] leading-relaxed text-[#68788e]">Only watched files whose completed upload still appears in the authenticated active-channel YouTube inventory are shown. Delete removes the local watched source only; the YouTube video is retained.</p>
              {uploadedFiles.length === 0 ? (
                <p className="mt-2 mb-0 text-[0.7rem] leading-snug text-[#75849a]">No watched-folder uploads are currently confirmed by YouTube.</p>
              ) : <button
                    className="rounded-md border border-[#e5c2c0] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#a4413b] transition-colors hover:border-[#d89d98] hover:bg-[#fff5f4] disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={busy || deletionSubmitting}
                    onClick={() => openDeletionReview(uploadedFiles)}
                    type="button"
                  >
                    Delete ({uploadedFiles.length}) files…
                  </button>}
                  {deletionQueue[0] && (
                    <div className="mt-3 grid gap-3 rounded-lg border border-[#e5c2c0] bg-[#fff8f7] p-3.5" role="dialog" aria-label="Confirm local watched file deletion">
                      <div className="grid gap-1">
                        <strong className="text-[0.75rem] text-[#8f3731]">{deletionQueue.length > 1 ? `Delete (${deletionQueue.length}) files` : "Delete local file"}</strong>
                        <p className="m-0 text-[0.69rem] leading-relaxed text-[#7b514d]">{deletionQueue.length > 1 ? <>Type <code className="rounded bg-white px-1 py-0.5 text-[#8f3731]">DELETE {deletionQueue.length} FILES</code> to permanently delete all selected local watched files. The YouTube videos are not deleted.</> : <>Type <code className="rounded bg-white px-1 py-0.5 text-[#8f3731]">{deletionQueue[0].fileName}</code> to permanently delete that local watched file. The YouTube video is not deleted.</>}</p>
                      </div>
                      <div className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2 max-sm:grid-cols-1">
                        <input
                          aria-label={deletionQueue.length > 1 ? "Exact bulk deletion phrase" : "Exact local filename"}
                          className="min-w-0 rounded-md border border-[#d9b8b4] bg-white px-3 py-2.5 text-[0.74rem] text-[#503532]"
                          disabled={deletionSubmitting}
                          onChange={(event) => setDeletionConfirmation(event.target.value)}
                          placeholder={deletionQueue.length > 1 ? `DELETE ${deletionQueue.length} FILES` : "Exact local filename"}
                          value={deletionConfirmation}
                        />
                        <button className="rounded-md border border-[#cbd3df] bg-white px-3 py-2.5 text-[0.74rem] font-[680] text-[#34405a] disabled:cursor-not-allowed disabled:opacity-50" disabled={deletionSubmitting} onClick={() => { setDeletionQueue([]); setDeletionConfirmation(""); }} type="button">Cancel</button>
                        <button className="rounded-md border border-[#b94842] bg-[#b94842] px-3 py-2.5 text-[0.74rem] font-[680] text-white disabled:cursor-not-allowed disabled:opacity-50" disabled={deletionSubmitting || deletionConfirmation.trim() !== (deletionQueue.length > 1 ? `DELETE ${deletionQueue.length} FILES` : deletionQueue[0].fileName)} onClick={() => void deleteLocalSource()} type="button">{deletionSubmitting ? "Deleting…" : deletionQueue.length > 1 ? `Delete (${deletionQueue.length}) files` : "Delete local file"}</button>
                      </div>
                      {deletionSubmitting && (
                        <div className="grid gap-1.5" aria-live="polite">
                          <div className="flex items-center justify-between gap-2 text-[0.68rem] font-semibold text-[#7b514d]"><span>Deleting local watched files…</span><span>{deletionProgress.completed} of {deletionProgress.total}</span></div>
                          <progress className="h-2 w-full accent-[#b94842]" max={Math.max(deletionProgress.total, 1)} value={deletionProgress.completed} />
                        </div>
                      )}
                    </div>
                  )}
                  {deletionResults.length > 0 && (
                    <details className="mt-3 rounded-lg border border-[#dfe7f2] bg-white px-3 py-2" open={deletionResults.some((result) => result.status !== "deleted")}>
                      <summary className="cursor-pointer text-[0.72rem] font-bold text-[#355776]">Deletion log ({deletionResults.length} files)</summary>
                      <div className="mt-2 grid gap-1.5" aria-live="polite">
                        <div className="flex items-center justify-between gap-2 text-[0.68rem] text-[#68788e]"><span>{deletionResults.filter((result) => result.status === "deleted").length} deleted locally · {deletionResults.filter((result) => result.status !== "deleted").length} retained</span><span>{deletionProgress.completed} of {deletionProgress.total || deletionResults.length}</span></div>
                        <progress className="h-2 w-full accent-[#26714e]" max={Math.max(deletionProgress.total || deletionResults.length, 1)} value={Math.min(deletionProgress.completed || deletionResults.length, deletionProgress.total || deletionResults.length)} />
                        <ul className="m-0 grid max-h-48 list-none gap-1.5 overflow-auto p-0">
                          {deletionResults.map((result) => <li className="grid gap-0.5 border-t border-[#edf0f5] pt-1.5 first:border-t-0 first:pt-0" key={result.itemId}><div className="flex items-center justify-between gap-2"><strong className="truncate text-[0.7rem] text-[#2f4262]" title={result.fileName}>{result.fileName}</strong><span className={result.status === "deleted" ? "text-[0.65rem] font-bold text-[#26714e]" : "text-[0.65rem] font-bold text-[#a4413b]"}>{result.status === "deleted" ? "Deleted locally" : "Retained"}</span></div><p className="m-0 text-[0.65rem] leading-snug text-[#68788e]">{result.detail}</p></li>)}
                        </ul>
                      </div>
                    </details>
                  )}
                  {uploadedFiles.length > 0 && <ul className="mt-3 grid max-h-64 list-none gap-2 overflow-auto p-0">
                    {uploadedFiles.map((file) => (
                      <li className="flex items-center justify-between gap-3 border-t border-[#edf0f5] py-2 first:border-t-0 first:pt-0" key={`${file.itemId}-${file.videoId}`}>
                        <div className="min-w-0">
                          <div className="flex items-baseline justify-between gap-2">
                            <strong className="min-w-0 truncate text-[0.72rem] text-[#2f4262]" title={file.fileName}>{file.uploadTitle ?? file.fileName}</strong>
                            <span className="text-[0.66rem] font-bold text-[#26714e]">YouTube confirmed</span>
                          </div>
                          <p className="mt-1 mb-0 text-[0.66rem] leading-snug text-[#68788e]">{file.fileName} · video ID {file.videoId}</p>
                        </div>
                        <button className="shrink-0 rounded-md border border-[#e5c2c0] bg-white px-3 py-2.5 text-[0.79rem] font-[680] text-[#a4413b] disabled:cursor-not-allowed disabled:opacity-50" disabled={busy || deletionSubmitting} onClick={() => openDeletionReview([file])} type="button">Delete local file…</button>
                      </li>
                    ))}
                  </ul>}
            </details>
          </section>

      <div className="mt-4 flex flex-wrap gap-2"><button className={primaryButtonClass} disabled={blocked || !settings.enabled} onClick={scanNow} type="button">{busy ? "Working…" : "Refresh this folder’s scan"}</button></div>
    </div> : null}
    {!isTauri ? <p className="mt-4 mb-0 text-sm text-muted">Open the desktop app to monitor local folders.</p> : null}
    <p className="mt-5 mb-0 max-w-4xl border-t border-line pt-4 text-sm leading-relaxed text-muted">All enabled folders are checked while the app is running. Only direct child videos are monitored; each must stop changing before acceptance. The original file remains the transfer source. Duplicate checks, YouTube’s file limits and source integrity checks apply to every folder.</p>
    {visibleSetup ? <FolderMonitorSetupDialog activeChannel={activeChannel} expectedChannelId={visibleSetup.channelId} monitor={visibleSetup.monitor} onClose={() => setSetup(undefined)} onSaved={(saved) => { setSelectedId(saved.id); requestRefresh.current(); onNotice("This folder is monitoring with its approved upload choices. Other folders were not changed."); }} /> : null}
    {visibleRules ? <Suspense fallback={<p className="mt-4 text-sm text-muted" role="status">Opening folder rule review…</p>}><FolderRulesDialog expectedChannelId={visibleRules.channelId} monitor={visibleRules.monitor} onClose={() => setRulesReview(undefined)} onSaved={() => { requestRefresh.current(); onNotice("Rules were saved for this folder’s future discovery. Existing queue items and other folders were not changed."); }} /></Suspense> : null}
    {visibleConfirmation ? <FolderMonitorConfirmationDialog key={visibleConfirmation.monitor.id} monitor={visibleConfirmation.monitor} onClose={() => setConfirmation(undefined)} onConfirm={() => perform(visibleConfirmation.monitor, () => removeFolderMonitor(visibleConfirmation.monitor.id!, visibleConfirmation.channelId), "This folder was removed from monitoring. Its original files, upload queue and receipts are retained.")} /> : null}
  </section>;
}

export default FolderMonitorPanel;
