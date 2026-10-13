import { ArrowLeft, ArrowRight, CalendarClock, Download, FileCheck2, FolderOpen, LoaderCircle, RefreshCw, X } from "lucide-react";
import { lazy, Suspense, useCallback, useEffect, useId, useRef, useState } from "react";
import { createUploadPlan, discardUploadPlan, exportUploadPlan, fileNameFromSelection, loadUploadPlan, planIsCurrent, planMatchesChannel, planPublicationTimes, planSealMatchesBase, planWithMetadata, planWithPublication, planWithoutSourceChoices, type UploadPlanPage, type UploadPlanRow } from "../lib/upload-plan";
import { publicationLocalInput, publicationUtcLabel } from "../lib/publication-schedule";
import { listYouTubePlaylists, loadConnectionSettings } from "../lib/local";
import { loadManualUploadDefaults } from "../lib/manual-upload-defaults";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import type { ManualUploadSettings, YouTubePlaylist } from "../lib/types";
import "./UploadPlanDialog.lazy.css";
type Props = {
  expectedChannelId: string;
  activeChannel?: string;
  initialPaths?: string[];
  initialSettings?: ManualUploadSettings;
  onClose: () => void;
  /** The normal native intake adapter revalidates this plan, rather than trusting displayed paths. */
  onExecute?: (planId: string, expectedChannelId: string) => Promise<void>;
};
const button = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const control = "min-h-11 w-full min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring";
const steps = ["Select", "Resolve choices", "Review"];
const UploadMetadataDialog = lazy(() => import("./UploadMetadataDialog").then((module) => ({ default: module.UploadMetadataDialog })));
const PublicationCalendarDialog = lazy(() => import("./PublicationCalendarDialog").then((module) => ({ default: module.PublicationCalendarDialog })));
const VideoAssetsDialog = lazy(() => import("./VideoAssetsDialog").then((module) => ({ default: module.VideoAssetsDialog })));
/** Refresh is always an explicit owner read, separate from offline plan creation. */
export async function refreshPlanPlaylistsForChannel(expectedChannelId: string, current: () => boolean): Promise<YouTubePlaylist[] | undefined> {
  assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
  if (!current()) return undefined;
  const playlists = await listYouTubePlaylists();
  assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
  return current() ? playlists : undefined;
}
export async function resolvePlanForChannel(paths: string[], settings: ManualUploadSettings, expectedChannelId: string, online: boolean, current: () => boolean): Promise<UploadPlanPage | undefined> {
  assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
  if (!current()) return undefined;
  const result = await createUploadPlan(paths, settings, expectedChannelId, online);
  let accepted = false;
  try {
    if (!current()) return undefined;
    assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
    if (!current()) return undefined;
    if (!planMatchesChannel(result, expectedChannelId)) throw new Error("The preview belongs to another channel. Resolve it again.");
    if (!planIsCurrent(result)) throw new Error("The preview already expired. Resolve it again.");
    accepted = true;
    return result;
  } finally {
    if (!accepted) {
      void discardUploadPlan(result.id, expectedChannelId).catch(() => undefined);
    }
  }
}
export function UploadPlanFile({ file }: { file: UploadPlanRow }) {
  const labels = { ready: "Ready for review", review_duplicate: "Duplicate candidate · review needed", blocked: "Cannot import · review required" };
  const metadata = Object.entries(file.metadata).filter(([key, value]) => key !== "title" && key !== "publishAt" && value !== undefined && value !== null);
  const metadataLabels: Record<string, string> = { description: "Description", tags: "Tags", categoryId: "Category ID", language: "Title and description language", defaultAudioLanguage: "Spoken audio language", license: "License", notifySubscribers: "Notify subscribers", containsSyntheticMedia: "Realistic altered content" };
  const publishAt = typeof file.metadata.publishAt === "string" ? file.metadata.publishAt : undefined;
  const thumbnail = file.assetJobs?.filter((asset) => asset.kind === "thumbnail") ?? [];
  const captions = file.assetJobs?.filter((asset) => asset.kind === "caption") ?? [];
  return <article className="min-w-0 rounded-lg border border-line bg-surface p-4">
    <div className="flex flex-wrap items-start justify-between gap-2"><div className="min-w-0"><h3 className="m-0 break-words text-base font-semibold">{file.title}</h3><p className="mt-1 mb-0 break-all text-xs text-muted">{file.fileName}</p></div><span className="rounded-full bg-subtle-surface px-3 py-1 text-xs font-semibold">{labels[file.duplicateDisposition]}</span></div>
    <dl className="mt-4 mb-0 grid grid-cols-2 gap-x-5 gap-y-3 text-sm sm:grid-cols-4">
      <div className="min-w-0"><dt className="text-xs text-muted">Channel</dt><dd className="mt-1 ml-0 break-all">{file.channelId}</dd></div>
      <div><dt className="text-xs text-muted">Visibility</dt><dd className="mt-1 ml-0 capitalize">{file.visibility}</dd></div>
      <div><dt className="text-xs text-muted">Audience</dt><dd className="mt-1 ml-0">{file.madeForKids ? "Made for kids" : "Not made for kids"}</dd></div>
      <div><dt className="text-xs text-muted">Source</dt><dd className="mt-1 ml-0">{file.sourceStatus === "verified" ? `${(file.sizeBytes / 1_000_000).toFixed(1)} MB · bytes verified` : "Unavailable"}</dd></div>
      <div className="col-span-2 min-w-0"><dt className="text-xs text-muted">Playlist</dt><dd className="mt-1 ml-0 break-words">{file.playlistTitle || (file.playlistId ? file.playlistId : "No playlist")}{file.playlistEvidence === "unverified_cached" ? " · ownership pending" : file.playlistEvidence === "owner_verified" ? " · owner verified" : ""}</dd></div>
      <div className="col-span-2"><dt className="text-xs text-muted">Original source cleanup</dt><dd className="mt-1 ml-0">{file.deleteSourceAfterUpload ? "Delete only after YouTube processing succeeds" : "Keep original"}</dd></div>
      <div className="col-span-2 min-w-0"><dt className="text-xs text-muted">Publication schedule</dt><dd className="mt-1 ml-0 break-all">{publishAt ? <><time dateTime={publishAt}>{publicationUtcLabel(publishAt)}</time><span className="mt-1 block text-xs text-muted">{file.publicationContext ? <><span className="block break-normal">{file.publicationContext.timeZone} · rules {file.publicationContext.tzdbVersion}</span><span className="block break-normal">Local time: {publicationLocalInput(publishAt, file.publicationContext.timeZone).replace("T", " ")}</span></> : "Timezone receipt unavailable in this preview"}</span><span className="mt-1 block text-xs text-muted">Private upload, then a separate confirmed publication job after processing</span></> : "None selected"}</dd></div>
      <div className="col-span-2 min-w-0"><dt className="text-xs text-muted">Thumbnail</dt><dd className="mt-1 ml-0 break-words">{thumbnail.length ? thumbnail.map((asset, index) => <span className="block" key={`${asset.fileName}-${index}`}>{asset.fileName}{asset.dimensions ? ` · ${asset.dimensions}` : ""} · selected original</span>) : "None selected"}</dd></div>
      <div className="col-span-2 min-w-0"><dt className="text-xs text-muted">Captions</dt><dd className="mt-1 ml-0 break-words">{captions.length ? captions.map((asset, index) => <span className="block" key={`${asset.fileName}-${index}`}>{asset.fileName}{asset.language ? ` · ${asset.language}` : ""}{asset.name ? ` · ${asset.name}` : ""}{asset.cueCount !== undefined && asset.cueCount !== null ? ` · ${asset.cueCount} timed cues` : ""}{asset.draft !== undefined && asset.draft !== null ? asset.draft ? " · draft caption requested" : " · visible caption requested" : ""}</span>) : "None selected"}</dd></div>
    </dl>
    {thumbnail.length || captions.length ? <p className="mt-3 mb-0 text-xs leading-relaxed text-muted">Selected originals were inspected for this preview and are checked again before import. Attachment jobs are created with intake and run only after the video receipt is confirmed.</p> : null}
    {metadata.length > 0 && <details className="mt-3 text-sm"><summary className="cursor-pointer font-semibold">Resolved metadata</summary><dl className="mt-3 grid gap-3">{metadata.map(([key, value]) => <div key={key} className="min-w-0"><dt className="text-xs text-muted">{metadataLabels[key] || key}</dt><dd className="mt-1 ml-0 whitespace-pre-wrap break-words">{Array.isArray(value) ? value.length ? value.join(", ") : "Empty (explicit)" : typeof value === "object" ? JSON.stringify(value) : value === "" ? "Empty (explicit)" : String(value)}</dd></div>)}</dl></details>}
    {(file.duplicateMatches.length > 0 || file.warnings.length > 0) && <div className="mt-3 rounded-md bg-subtle-surface p-3 text-sm">{file.duplicateMatches.map((match, index) => <p key={`m-${index}`} className="my-1 break-words">{match}</p>)}{file.warnings.map((warning, index) => <p key={`w-${index}`} className="my-1 break-words">{warning}</p>)}</div>}
  </article>;
}
export function UploadPlanReview({ plan }: { plan: UploadPlanPage }) {
  return <>
    <div className="rounded-lg border border-line bg-subtle-surface p-4"><div className="flex flex-wrap gap-x-5 gap-y-2 text-sm font-semibold"><span>{plan.totalFiles} videos</span><span>{plan.readyFiles} ready</span><span>{plan.duplicateFiles} duplicate candidates</span><span>{plan.blockedFiles} blocked</span></div><p className="mt-3 mb-0 text-sm">{plan.evidenceMode === "online" ? "Owner-authorized YouTube reads" : "Offline · cached YouTube evidence"}{plan.evidenceAsOf ? ` · evidence from ${new Date(plan.evidenceAsOf).toLocaleString()}` : " · no cached library"}</p><p className="mt-2 mb-0 text-xs text-muted">Preview expires {new Date(plan.expiresAt).toLocaleTimeString()}. Originals and destination are rechecked before any import.</p></div>
    <div className="mt-4 grid gap-3">{plan.files.map((file) => <UploadPlanFile key={file.ordinal} file={file} />)}</div>
  </>;
}
export function UploadPlanDialog({ expectedChannelId, activeChannel, initialPaths = [], initialSettings, onClose, onExecute }: Props) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const callbacks = useRef({ onClose, onExecute });
  callbacks.current = { onClose, onExecute };
  const openedChannel = useRef(expectedChannelId);
  const openedSelection = useRef(JSON.stringify(initialPaths));
  const closed = useRef(false);
  const pending = useRef(false);
  const planRef = useRef<UploadPlanPage | undefined>(undefined);
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const playlistFence = useWorkspaceRequestFence(expectedChannelId);
  const defaultsFence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState(0);
  const [paths, setPaths] = useState(initialPaths);
  const [settings, setSettings] = useState<ManualUploadSettings>(() => initialSettings ? { ...initialSettings, visibility: Object.keys(planPublicationTimes(initialPaths, initialSettings)).length ? "private" : initialSettings.visibility } : { madeForKids: false, visibility: "private", deleteSourceAfterUpload: false });
  const [online, setOnline] = useState(false);
  const [plan, setPlan] = useState<UploadPlanPage>();
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [clock, setClock] = useState(Date.now());
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistBusy, setPlaylistBusy] = useState(false);
  const [playlistError, setPlaylistError] = useState("");
  const [playlistNotice, setPlaylistNotice] = useState("");
  const [metadataOpen, setMetadataOpen] = useState(false);
  const [publicationOpen, setPublicationOpen] = useState(false);
  const [assetsOpen, setAssetsOpen] = useState(false);
  const [assetsReviewed, setAssetsReviewed] = useState(false);
  const assetBase = useRef<UploadPlanPage | undefined>(undefined);
  const [defaultsBusy, setDefaultsBusy] = useState(!initialSettings);
  const choicesChanged = useRef(Boolean(initialSettings));
  const playlistPending = useRef(false);
  const channelMatches = openedChannel.current === expectedChannelId;
  const nestedOpen = metadataOpen || publicationOpen || assetsOpen;
  const publicationTimes = planPublicationTimes(paths, settings);
  const publicationCount = Object.keys(publicationTimes).length;
  const releasePlan = useCallback(() => {
    const previous = planRef.current;
    planRef.current = undefined;
    setPlan(undefined);
    setApproved(false);
    setAssetsOpen(false);
    setAssetsReviewed(false);
    assetBase.current = undefined;
    if (previous) void discardUploadPlan(previous.id, previous.channelId).catch(() => undefined);
  }, []);
  const finish = useCallback(() => {
    if (closed.current) return;
    closed.current = true;
    fence.invalidate();
    playlistFence.invalidate();
    defaultsFence.invalidate();
    releasePlan();
    setMetadataOpen(false);
    setPublicationOpen(false);
    setAssetsOpen(false);
    dialog.current?.close();
    callbacks.current.onClose();
  }, [fence, playlistFence, defaultsFence, releasePlan]);
  useEffect(() => {
    const element = dialog.current;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    element?.showModal();
    heading.current?.focus();
    return () => {
      element?.close();
      const retained = planRef.current;
      planRef.current = undefined;
      if (retained) void discardUploadPlan(retained.id, retained.channelId).catch(() => undefined);
      if (previous?.isConnected) previous.focus();
    };
  }, []);
  useEffect(() => { if (!channelMatches) finish(); }, [channelMatches, finish]);
  useEffect(() => { if (openedSelection.current !== JSON.stringify(initialPaths)) finish(); }, [initialPaths, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step]);
  useEffect(() => { if (!nestedOpen) heading.current?.focus(); }, [nestedOpen]);
  useEffect(() => { const timer = window.setInterval(() => setClock(Date.now()), 1000); return () => window.clearInterval(timer); }, []);
  useEffect(() => {
    if (initialSettings) return;
    const current = defaultsFence.begin();
    setDefaultsBusy(true);
    void loadManualUploadDefaults(expectedChannelId).then((defaults) => {
      if (!current() || choicesChanged.current) return;
      setSettings({ madeForKids: defaults.madeForKids, visibility: defaults.visibility ?? "private", deleteSourceAfterUpload: false,
        playlistId: defaults.playlistChannelId === expectedChannelId ? defaults.playlistId : undefined,
        playlistTitle: defaults.playlistChannelId === expectedChannelId ? defaults.playlistTitle : undefined });
    }).catch(() => { if (current()) setNotice("Saved defaults are unavailable. Review the displayed choices for this batch."); })
      .finally(() => { if (current()) setDefaultsBusy(false); });
    return () => defaultsFence.invalidate();
  }, [defaultsFence, expectedChannelId, initialSettings]);
  const updateSettings = (next: ManualUploadSettings) => {
    choicesChanged.current = true;
    if (planRef.current) setNotice("Changing upload choices discarded the previous preview and its attachment selections. Prepare them again before importing.");
    releasePlan();
    setSettings(next);
  };
  const refreshPlaylists = async () => {
    if (playlistPending.current || pending.current || !channelMatches) return;
    playlistPending.current = true; setPlaylistBusy(true); setPlaylistError("");
    const current = playlistFence.begin();
    try {
      const items = await refreshPlanPlaylistsForChannel(expectedChannelId, current);
      if (!items) return;
      setPlaylists(items);
      const selected = items.find((item) => item.id === settings.playlistId);
      if (settings.playlistId && !selected) {
        setPlaylistNotice("The selected playlist is unavailable. Choose another playlist or No playlist before resolving this batch.");
      } else {
        setPlaylistNotice("Playlists refreshed through owner-authorized reads.");
        if (selected && selected.title !== settings.playlistTitle) updateSettings({ ...settings, playlistTitle: selected.title });
      }
    } catch (problem) { if (current()) setPlaylistError(workspaceErrorMessage(problem, "Playlists could not be refreshed. The current choice is retained.")); }
    finally { playlistPending.current = false; if (current()) setPlaylistBusy(false); }
  };
  const chooseFiles = async () => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError("");
    const current = fence.begin();
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ title: "Choose videos to preview", multiple: true, directory: false, filters: [{ name: "Videos", extensions: ["mp4", "mov", "mkv", "webm", "avi", "mpeg", "mpg", "m4v", "wmv", "flv", "3gp", "3g2"] }] });
      if (current() && selected) {
        const next = typeof selected === "string" ? [selected] : selected;
        if (next.length > 512) { setError("Choose up to 512 videos for one batch preview."); return; }
        releasePlan(); setPaths(next); setMetadataOpen(false); setPublicationOpen(false); setAssetsOpen(false);
        if (JSON.stringify(next) !== JSON.stringify(paths)) { choicesChanged.current = true; setSettings(planWithoutSourceChoices(settings)); setNotice("The selected originals changed. Metadata and publication choices were cleared for a fresh review."); }
      }
    } catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "Videos could not be selected.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  };
  const resolve = async () => {
    if (pending.current || paths.length === 0 || !channelMatches) return;
    pending.current = true; setBusy(true); setError(""); setNotice(""); setApproved(false);
    const current = fence.begin();
    try {
      const retained = planRef.current;
      const result = retained && planIsCurrent(retained) ? await loadUploadPlan(retained.id, expectedChannelId, 0, 25) : await resolvePlanForChannel(paths, settings, expectedChannelId, online, current);
      if (!result) return;
      if (!current()) return;
      assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
      if (!current()) return;
      if (!planMatchesChannel(result, expectedChannelId) || !planIsCurrent(result)) throw new Error("The retained preview changed or expired. Prepare a fresh plan before importing.");
      if (!retained || retained.id !== result.id) releasePlan(); planRef.current = result; setPlan(result); setStep(2);
    } catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "Batch choices could not be resolved. Nothing was imported.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  };
  const chooseAssets = async () => {
    if (pending.current || !paths.length || !channelMatches) return;
    pending.current = true; setBusy(true); setError(""); setApproved(false);
    const current = fence.begin();
    try {
      const retained = planRef.current;
      const result = retained && planIsCurrent(retained) ? await loadUploadPlan(retained.id, expectedChannelId, 0, 25) : await resolvePlanForChannel(paths, settings, expectedChannelId, online, current);
      if (!result || !current()) return;
      assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
      if (!current()) return;
      if (!planMatchesChannel(result, expectedChannelId) || !planIsCurrent(result)) throw new Error("This source preview changed or expired. Resolve it again before choosing attachments.");
      if (!retained || retained.id !== result.id) releasePlan(); planRef.current = result; setPlan(result); assetBase.current = result; setAssetsOpen(true);
    } catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "Attachment choices could not be prepared. Nothing was imported or sent to YouTube.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  };
  const assetsApplied = (sealed: UploadPlanPage) => {
    const base = assetBase.current;
    if (!base || closed.current || !channelMatches || planRef.current?.id !== base.id || !planSealMatchesBase(base, sealed)) {
      void discardUploadPlan(sealed.id, sealed.channelId).catch(() => undefined);
      setError("The attachment result no longer matches this preview. Prepare the originals and choices again.");
      setAssetsOpen(false); return;
    }
    releasePlan(); planRef.current = sealed; setPlan(sealed); setAssetsReviewed(true); setNotice("Attachment selections are sealed into this preview. Review every video’s selected or absent attachments before importing.");
  };
  const changePage = async (offset: number) => {
    if (!plan || pending.current) return;
    pending.current = true; setBusy(true); setError("");
    const current = fence.begin();
    try {
      const result = await loadUploadPlan(plan.id, expectedChannelId, offset, 25);
      if (current()) { if (!planMatchesChannel(result, expectedChannelId)) throw new Error("The preview channel changed."); setPlan(result); planRef.current = result; }
    } catch (problem) { if (current()) { setApproved(false); setError(workspaceErrorMessage(problem, "This preview could not be loaded. Resolve it again.")); } }
    finally { pending.current = false; if (current()) setBusy(false); }
  };
  const exportPlan = async () => {
    if (!plan || pending.current) return;
    pending.current = true; setBusy(true); setError("");
    const current = fence.begin();
    try { const path = await exportUploadPlan(plan.id, expectedChannelId); if (current() && path) setNotice("Redacted preview exported. This file does not authorize uploads or restore queued work."); }
    catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "The preview could not be exported.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  };
  const execute = async () => {
    if (!plan || pending.current || !approved || !planIsCurrent(plan) || plan.blockedFiles > 0 || !callbacks.current.onExecute) return;
    pending.current = true; setBusy(true); setError("");
    const current = fence.begin();
    try { assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId); if (!current()) return; await callbacks.current.onExecute(plan.id, expectedChannelId); if (current()) finish(); }
    catch (problem) { if (current()) { setApproved(false); setError(workspaceErrorMessage(problem, "Import requires a fresh preview. Nothing was approved automatically.")); } }
    finally { pending.current = false; if (current()) setBusy(false); }
  };
  return <>
  <dialog ref={dialog} inert={nestedOpen} aria-modal={nestedOpen ? undefined : true} aria-labelledby={`${id}-title`} aria-describedby={`${id}-description`} onCancel={(event) => { event.preventDefault(); if (!nestedOpen) finish(); }} className="m-auto max-h-[90dvh] w-[min(60rem,calc(100%-2rem))] max-w-none overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-xl backdrop:bg-black/60">
    <div className="border-b border-line px-5 pt-5 pb-4 sm:px-7"><div className="flex items-start justify-between gap-4"><div><p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">Batch preview · {activeChannel || "Connected channel"}</p><h2 ref={heading} tabIndex={-1} id={`${id}-title`} className="m-0 text-xl font-semibold outline-none">{step === 0 ? "Select videos to preview" : step === 1 ? "Resolve upload choices" : "Review the complete batch"}</h2></div><button type="button" onClick={finish} aria-label="Close batch preview" className={`${button} px-3`}><X aria-hidden="true" size={18} /></button></div><p id={`${id}-description`} className="mt-3 mb-0 max-w-2xl text-sm text-muted">See the destination, metadata and duplicate evidence before approving a batch. Previewing leaves the queue and originals unchanged.</p><ol aria-label="Batch preview steps" className="mt-5 mb-0 flex list-none gap-2 p-0">{steps.map((label, index) => <li key={label} aria-current={index === step ? "step" : undefined} className={`flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-2 text-xs font-semibold ${index === step ? "bg-subtle-surface text-ink" : "text-muted"}`}><span className="shrink-0">{index + 1}.</span><span>{label}</span></li>)}</ol></div>
    <div className="px-5 py-5 sm:px-7">
      {error && <p role="alert" className="mt-0 mb-4 rounded-md border border-line bg-subtle-surface p-3 text-sm">{error}</p>}{notice && <p role="status" className="mt-0 mb-4 text-sm">{notice}</p>}
      {step === 0 && <><button className={primary} type="button" disabled={busy} onClick={() => void chooseFiles()}><FolderOpen size={18} aria-hidden="true" />Choose videos</button><p className="mt-4 text-sm">{paths.length ? `${paths.length} selected · originals stay in place` : "Choose up to 512 local videos. This creates a preview, without importing them."}</p>{paths.length > 0 && <ul aria-label="Selected videos" className="max-h-64 list-none overflow-y-auto rounded-lg border border-line bg-subtle-surface p-3">{paths.map((path, index) => <li key={`${path}-${index}`} className="break-all px-1 py-2 text-sm">{fileNameFromSelection(path)}</li>)}</ul>}</>}
      {step === 1 && <div className="grid gap-5">
        {defaultsBusy && <p role="status" className="m-0 text-sm text-muted">Loading saved device defaults…</p>}
        <div className="grid gap-4 sm:grid-cols-2">
          <label className="grid gap-2 text-sm font-semibold">Visibility<select disabled={busy || playlistBusy || defaultsBusy || publicationCount > 0} value={settings.visibility} className={control} onChange={(event) => updateSettings({ ...settings, visibility: event.target.value as ManualUploadSettings["visibility"] })}><option value="private">Private</option><option value="unlisted">Unlisted</option><option value="public">Public</option></select></label>
          <label className="grid gap-2 text-sm font-semibold">Audience<select disabled={busy || playlistBusy || defaultsBusy} value={settings.madeForKids ? "yes" : "no"} className={control} onChange={(event) => updateSettings({ ...settings, madeForKids: event.target.value === "yes" })}><option value="no">Not made for kids</option><option value="yes">Made for kids</option></select></label>
        </div>
        {publicationCount > 0 ? <p className="m-0 text-sm leading-relaxed text-muted" role="status">Scheduled videos upload privately. Publication is requested separately after processing; exact UTC dates and timezone rules appear in the review.</p> : null}
        <div className="grid gap-2">
          <label htmlFor={`${id}-playlist`} className="text-sm font-semibold">Add to playlist</label>
          <div className="flex flex-wrap gap-2">
            <select id={`${id}-playlist`} value={settings.playlistId || ""} disabled={busy || playlistBusy || defaultsBusy} className={`${control} flex-1 basis-64`} onChange={(event) => {
              const selected = playlists.find((item) => item.id === event.target.value);
              updateSettings({ ...settings, playlistId: event.target.value || undefined, playlistTitle: selected?.title });
              setPlaylistNotice("");
            }}>
              <option value="">No playlist</option>
              {settings.playlistId && !playlists.some((item) => item.id === settings.playlistId) && <option value={settings.playlistId}>{settings.playlistTitle || "Saved playlist"} · ownership pending</option>}
              {playlists.map((playlist) => <option key={playlist.id} value={playlist.id}>{playlist.title}</option>)}
            </select>
            <button type="button" className={button} disabled={busy || playlistBusy || defaultsBusy} onClick={() => void refreshPlaylists()}><RefreshCw size={16} aria-hidden="true" />{playlistBusy ? "Refreshing…" : "Refresh playlists"}</button>
          </div>
          <p className="m-0 text-xs text-muted">Refreshing performs owner-authorized YouTube reads and uses API quota. Offline preview can retain a saved choice; ownership is checked again before import.</p>
          {playlistError && <p role="alert" className="m-0 text-sm">{playlistError}</p>}
          {playlistNotice && <p role="status" className="m-0 text-sm">{playlistNotice}</p>}
        </div>
        <div className="rounded-lg border border-line bg-subtle-surface p-4">
          <div className="flex flex-wrap items-start justify-between gap-3"><div><h3 className="m-0 text-sm font-semibold">Metadata, presets and manifests</h3><p className="mt-2 mb-0 max-w-xl text-xs text-muted">Choose a channel preset or a local CSV/JSON manifest, then edit title, description, tags and declarations. Each video’s resolved values appear in the complete batch plan.</p></div><button type="button" className={button} disabled={busy || playlistBusy || defaultsBusy} onClick={() => setMetadataOpen(true)}>Edit metadata</button></div>
          <p className="mt-3 mb-0 text-sm">{settings.metadata || settings.itemMetadata ? `${Object.keys(settings.itemMetadata || {}).length} file overrides · batch metadata selected` : "Filename titles · no additional metadata selected"}</p>
        </div>
        <section className="rounded-lg border border-line bg-subtle-surface p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><h3 className="m-0 text-sm font-semibold">Publication calendar</h3><p className="mt-2 mb-0 max-w-xl text-xs leading-relaxed text-muted">Choose a timezone and each video’s date. Previously approved UTC instants stay fixed when the display timezone changes.</p></div><button type="button" className={button} disabled={busy || playlistBusy || defaultsBusy} onClick={() => setPublicationOpen(true)}><CalendarClock aria-hidden="true" size={16} />Choose publication dates</button></div><p className="mt-3 mb-0 text-sm">{publicationCount ? `${publicationCount} reviewed UTC publication times · private upload` : "No publication dates selected"}</p></section>
        <section className="rounded-lg border border-line bg-subtle-surface p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><h3 className="m-0 text-sm font-semibold">Thumbnails and captions</h3><p className="mt-2 mb-0 max-w-xl text-xs leading-relaxed text-muted">Prepare a source preview, then inspect original attachment files and assign them to these videos. No upload or attachment job is created while previewing.</p></div><button type="button" className={button} disabled={busy || playlistBusy || defaultsBusy} onClick={() => void chooseAssets()}>Choose thumbnails and captions</button></div><p className="mt-3 mb-0 text-sm">{assetsReviewed ? "Attachment choices are retained in this sealed preview. Each video’s selected or absent results appear in Review." : "None selected for this plan."}</p><p className="mt-2 mb-0 text-xs leading-relaxed text-muted">Changing originals, metadata, schedules or destination choices discards the preview and its attachment selections.</p></section>
        <label className="flex items-start gap-3 text-sm"><input type="checkbox" disabled={busy || playlistBusy || defaultsBusy} className="mt-1 h-4 w-4 shrink-0 accent-brand" checked={settings.deleteSourceAfterUpload} onChange={(event) => updateSettings({ ...settings, deleteSourceAfterUpload: event.target.checked })} /><span>Delete the original only after YouTube processing succeeds</span></label>
        <fieldset disabled={busy || playlistBusy || defaultsBusy} className="rounded-lg border border-line bg-subtle-surface p-4"><legend className="px-1 text-sm font-semibold">Evidence source</legend>
          <label className="flex items-start gap-3 text-sm"><input type="radio" name={`${id}-evidence`} checked={!online} onChange={() => { releasePlan(); setOnline(false); }} className="mt-1 accent-brand" /><span><strong className="block">Offline preview</strong><span className="text-muted">Read original bytes and the device’s cached channel library. Its age appears in Review.</span></span></label>
          <label className="mt-4 flex items-start gap-3 text-sm"><input type="radio" name={`${id}-evidence`} checked={online} onChange={() => { releasePlan(); setOnline(true); }} className="mt-1 accent-brand" /><span><strong className="block">Refresh YouTube evidence</strong><span className="text-muted">Explicit owner-authorized reads of your channel, upload library and selected playlist. These reads use API quota; no YouTube metadata changes.</span></span></label>
        </fieldset>
      </div>}
      {step === 2 && plan && <><UploadPlanReview plan={plan} /><div className="mt-4 flex flex-wrap items-center justify-between gap-3"><p className="m-0 text-xs text-muted">Showing {plan.offset + 1}–{Math.min(plan.offset + plan.files.length, plan.totalFiles)} of {plan.totalFiles}</p><div className="flex gap-2"><button className={button} disabled={busy || plan.offset === 0} type="button" onClick={() => void changePage(Math.max(0, plan.offset - 25))}>Previous</button><button className={button} disabled={busy || plan.offset + plan.files.length >= plan.totalFiles} type="button" onClick={() => void changePage(plan.offset + 25)}>Next page</button></div></div><details className="mt-4 text-sm"><summary className="cursor-pointer font-semibold">Preview boundaries</summary>{plan.warnings.map((warning, index) => <p key={index} className="my-2">{warning}</p>)}</details>{!planIsCurrent(plan, clock) && <p role="alert" className="mt-4 text-sm font-semibold">This preview expired. Resolve it again before exporting or importing.</p>}{onExecute && <label className="mt-5 flex items-start gap-3 rounded-lg border border-line bg-subtle-surface p-4 text-sm"><input checked={approved} onChange={(event) => setApproved(event.target.checked)} type="checkbox" className="mt-1 h-4 w-4 shrink-0 accent-brand" disabled={busy || plan.blockedFiles > 0 || !planIsCurrent(plan, clock)} /><span>I reviewed this batch’s choices and duplicate candidates. Import rechecks original bytes, channel, playlist ownership and current library evidence through YouTube reads that use API quota.</span></label>}</>}
    </div>
    <footer className="flex flex-wrap justify-between gap-3 border-t border-line px-5 py-4 sm:px-7"><div className="flex flex-wrap gap-2">{step > 0 && <button type="button" className={button} disabled={busy} onClick={() => { setApproved(false); setStep(step - 1); }}><ArrowLeft aria-hidden="true" size={16} />Back</button>}<button type="button" className={button} onClick={finish}>Cancel</button></div><div className="flex flex-wrap gap-2">{step === 0 ? <button type="button" className={primary} disabled={busy || playlistBusy || defaultsBusy || paths.length === 0 || paths.length > 512} onClick={() => setStep(1)}>Next: Resolve choices<ArrowRight size={16} aria-hidden="true" /></button> : step === 1 ? <button type="button" className={primary} disabled={busy || playlistBusy || defaultsBusy || paths.length === 0 || paths.length > 512} onClick={() => void resolve()}>{busy ? <LoaderCircle className="animate-spin" size={16} aria-hidden="true" /> : <FileCheck2 size={16} aria-hidden="true" />}{busy ? "Verifying original bytes…" : "Resolve and review"}</button> : plan && <><button type="button" className={button} disabled={busy} onClick={() => { setStep(1); setApproved(false); }}><RefreshCw size={16} aria-hidden="true" />Resolve again</button><button type="button" className={onExecute ? button : primary} disabled={busy || !planIsCurrent(plan, clock)} onClick={() => void exportPlan()}><Download size={16} aria-hidden="true" />Export redacted plan</button>{onExecute && <button type="button" className={primary} disabled={busy || !approved || plan.blockedFiles > 0 || !planIsCurrent(plan, clock)} onClick={() => void execute()}>{busy ? "Revalidating…" : "Import reviewed batch"}</button>}</>}</div></footer>
  </dialog>
  {metadataOpen && <Suspense fallback={<div role="dialog" aria-modal="true" aria-label="Preparing metadata review" className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"><div className="rounded-lg bg-surface p-5 text-ink"><p role="status">Preparing metadata review…</p><button type="button" className={button} onClick={() => setMetadataOpen(false)}>Cancel</button></div></div>}><UploadMetadataDialog expectedChannelId={expectedChannelId} paths={paths} initialMetadata={settings.metadata} initialItemMetadata={settings.itemMetadata} onApply={({ metadata, itemMetadata }) => { updateSettings(planWithMetadata(paths, settings, metadata, itemMetadata)); setMetadataOpen(false); }} onClose={() => { setMetadataOpen(false); heading.current?.focus(); }} /></Suspense>}
  {publicationOpen && <Suspense fallback={<div role="dialog" aria-modal="true" aria-label="Preparing publication calendar" className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"><div className="rounded-lg bg-surface p-5 text-ink"><p role="status">Preparing publication calendar…</p><button type="button" className={button} onClick={() => setPublicationOpen(false)}>Cancel</button></div></div>}><PublicationCalendarDialog expectedChannelId={expectedChannelId} paths={paths} initialTimesByPath={publicationTimes} initialTimeZone={paths.map((path) => settings.publicationContexts?.[path]?.timeZone).find(Boolean)} onApply={(result) => { updateSettings(planWithPublication(paths, settings, result)); setPublicationOpen(false); }} onClose={() => setPublicationOpen(false)} /></Suspense>}
  {assetsOpen && plan && <Suspense fallback={<div role="dialog" aria-modal="true" aria-label="Preparing attachment preview" className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"><div className="rounded-lg bg-surface p-5 text-ink"><p role="status">Preparing attachment preview…</p><button type="button" className={button} onClick={() => setAssetsOpen(false)}>Cancel</button></div></div>}><VideoAssetsDialog expectedChannelId={expectedChannelId} activeChannel={activeChannel} prospectivePlanId={plan.id} onPlanApplied={assetsApplied} onClose={() => setAssetsOpen(false)} /></Suspense>}
  </>;
}
