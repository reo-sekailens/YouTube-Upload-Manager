import { ArrowLeft, ArrowRight, Calculator, ExternalLink, LoaderCircle, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { loadConnectionSettings } from "../lib/local";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import { defaultQuotaChoices, estimateQuotaOperations, formatQuotaTime, loadQuotaPlanner, openQuotaResource, parseQuotaLimit, quotaBucketLabels, quotaLimitsChanged, quotaSnapshotMatches, saveQuotaProjectLimits, validQuotaChoices, validQuotaTimeZone, type QuotaChoices, type QuotaEstimate, type QuotaProjectLimits, type QuotaSnapshot } from "../lib/quota-planner";
import "./QuotaPlannerDialog.lazy.css";
import { LiveQuotaPanel } from "./LiveQuotaPanel";

export type QuotaPlannerDialogProps = {
  expectedChannelId: string;
  activeChannel?: string;
  initialChoices?: Partial<QuotaChoices>;
  onClose: () => void;
  onSaved?: () => void;
};

const button = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const control = "min-h-11 w-full min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:opacity-50";
const steps = ["Choices", "Estimate", "Review"];
type LimitInputs = { uploadCalls: string; searchCalls: string; otherUnits: string };
const fields = [
  ["uploads", "New video uploads"], ["thumbnailWrites", "Thumbnail uploads"], ["captionInserts", "New caption tracks"], ["captionUpdates", "Caption replacements"], ["metadataUpdates", "Metadata or publication updates"], ["searchPages", "Search pages"],
] as const;

export async function loadQuotaForChannel(expected: string, current: () => boolean) {
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return undefined;
  const snapshot = await loadQuotaPlanner(expected);
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return undefined;
  if (!quotaSnapshotMatches(snapshot, expected)) throw new Error("The quota snapshot did not match the connected channel.");
  return snapshot;
}

export async function estimateQuotaForChannel(expected: string, scopeId: string, choices: QuotaChoices, current: () => boolean) {
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return undefined;
  const estimate = await estimateQuotaOperations(expected, choices);
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return undefined;
  if (!quotaSnapshotMatches(estimate.snapshot, expected, scopeId)) throw new Error("The connected Cloud project changed. Refresh the planner before estimating.");
  return estimate;
}

export async function saveReviewedQuotaLimits(expected: string, scopeId: string, limits: QuotaProjectLimits, approved: boolean, current: () => boolean) {
  if (!approved) throw new Error("Review and approve the local planning limits first.");
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return undefined;
  const saved = await saveQuotaProjectLimits(expected, scopeId, limits);
  assertWorkspaceChannel(await loadConnectionSettings(), expected);
  if (!current()) return undefined;
  if (!quotaSnapshotMatches(saved, expected, scopeId)) throw new Error("The planning-limit receipt did not match the reviewed project.");
  return saved;
}

export function QuotaHoldSummary({ snapshot, timeZone }: { snapshot: QuotaSnapshot; timeZone: string }) {
  return <section aria-label="Independent provider limits" className="grid gap-3 sm:grid-cols-2">
    <div className="rounded-lg border border-line bg-subtle-surface p-4"><h3 className="m-0 text-sm font-semibold">Google Cloud API quota</h3><p className="mt-2 mb-0 text-sm">{snapshot.cloudHold ? snapshot.cloudHold.active ? "Saved API pause is active" : "Saved API pause deadline elapsed" : "No saved API pause"}</p>{snapshot.cloudHold && <p className="mt-2 mb-0 text-xs text-muted">Saved deadline: {formatQuotaTime(snapshot.cloudHold.deadline, timeZone)} · {timeZone}</p>}<p className="mt-2 mb-0 text-xs text-muted">Expected daily reset: {formatQuotaTime(snapshot.pacificResetAt, timeZone)} · {timeZone} ({formatQuotaTime(snapshot.pacificResetAt, "America/Los_Angeles")} Pacific). A reset does not guarantee acceptance.</p></div>
    <div className="rounded-lg border border-line bg-subtle-surface p-4"><h3 className="m-0 text-sm font-semibold">YouTube channel upload limit</h3><p className="mt-2 mb-0 text-sm">{snapshot.channelHold ? snapshot.channelHold.active ? "Saved channel pause is active" : "Saved channel pause deadline elapsed" : "No saved channel pause"}</p>{snapshot.channelHold && <p className="mt-2 mb-0 text-xs text-muted">Saved deadline: {formatQuotaTime(snapshot.channelHold.deadline, timeZone)} · {timeZone}</p>}<p className="mt-2 mb-0 text-xs text-muted">Channel allowance is unknown. The separate conservative channel pause is preserved; Pacific quota reset does not clear it.</p></div>
  </section>;
}

export function QuotaEstimateReview({ estimate, proposedLimits, timeZone }: { estimate: QuotaEstimate; proposedLimits: QuotaProjectLimits; timeZone: string }) {
  const limitKeys = { video_uploads: "uploadCalls", search: "searchCalls", other: "otherUnits" } as const;
  return <div className="grid gap-5">
    <div className="grid gap-3 sm:grid-cols-3">{estimate.buckets.map((bucket) => {
      const limit = proposedLimits[limitKeys[bucket.bucket]];
      const exceeds = limit != null && bucket.estimatedUnits + bucket.observedUnits > limit;
      return <article key={bucket.bucket} className="min-w-0 rounded-lg border border-line bg-subtle-surface p-4"><h3 className="m-0 text-sm font-semibold">{quotaBucketLabels[bucket.bucket]}</h3><p className="mt-3 mb-0 text-2xl font-semibold tabular-nums">≈ {bucket.estimatedUnits.toLocaleString()}</p><p className="mt-1 mb-0 text-xs text-muted">Estimated for this plan, including retry reserve</p><dl className="mt-4 mb-0 grid gap-2 text-xs"><div><dt className="text-muted">This app/device observed</dt><dd className="mt-1 ml-0 tabular-nums">≈ {bucket.observedUnits.toLocaleString()} units today</dd></div><div><dt className="text-muted">Entered daily planning limit</dt><dd className="mt-1 ml-0">{limit == null ? "Unknown · no limit entered" : limit.toLocaleString()}</dd></div><div><dt className="text-muted">Actual project remaining quota</dt><dd className="mt-1 ml-0 font-semibold">Unknown</dd></div></dl>{exceeds && <p className="mt-3 mb-0 text-xs font-semibold">Recorded usage plus this estimate exceeds your entered limit.</p>}</article>;
    })}</div>
    <div><h3 className="mt-0 mb-3 text-sm font-semibold">Estimated operations</h3>{estimate.operations.length ? <ul className="m-0 grid list-none gap-2 p-0">{estimate.operations.map((operation, index) => <li key={`${operation.method}-${index}`} className="flex flex-wrap items-baseline justify-between gap-2 rounded-md bg-subtle-surface px-3 py-3 text-sm"><div className="min-w-0"><strong className="block break-words">{operation.label}</strong><span className="text-xs text-muted">{operation.requests} requests{operation.retryRequests ? ` + ${operation.retryRequests} retry reserve` : ""} · {quotaBucketLabels[operation.bucket]} · {operation.costPerRequest} per request</span></div><span className="shrink-0 font-semibold tabular-nums">≈ {operation.estimatedUnits.toLocaleString()}</span></li>)}</ul> : <p className="m-0 rounded-md bg-subtle-surface p-4 text-sm">No operations selected. Add reviewed counts in Choices to estimate a batch.</p>}</div>
    <QuotaHoldSummary snapshot={estimate.snapshot} timeZone={timeZone} />
    <p className="m-0 text-xs text-muted">{estimate.snapshot.observationLabel}. Pacific quota day: {estimate.snapshot.pacificDay}. {estimate.snapshot.unknownRequests ? `${estimate.snapshot.unknownRequests} unclassified requests have unknown bucket and unit costs. ` : ""}Unrecorded requests and usage from other devices or apps are invisible.</p>
    <details className="text-sm"><summary className="cursor-pointer font-semibold">Cost-table evidence and estimate boundaries</summary><p className="my-3 text-xs text-muted">{estimate.snapshot.costVersion} · checked {estimate.snapshot.costAccessedAt}</p>{[...estimate.snapshot.warnings, ...estimate.warnings].map((warning, index) => <p key={index} className="my-3 text-sm">{warning}</p>)}</details>
  </div>;
}

export function QuotaPlannerDialog({ expectedChannelId, activeChannel, initialChoices, onClose, onSaved }: QuotaPlannerDialogProps) {
  const id = useId(); const dialog = useRef<HTMLDialogElement>(null); const heading = useRef<HTMLHeadingElement>(null);
  const callbacks = useRef({ onClose, onSaved }); callbacks.current = { onClose, onSaved };
  const openedChannel = useRef(expectedChannelId); const closed = useRef(false); const pending = useRef(false);
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState(0); const [snapshot, setSnapshot] = useState<QuotaSnapshot>(); const [estimate, setEstimate] = useState<QuotaEstimate>();
  const [view, setView] = useState<"google" | "estimate">("google");
  const [choices, setChoices] = useState(() => defaultQuotaChoices(initialChoices));
  const [limitInputs, setLimitInputs] = useState<LimitInputs>({ uploadCalls: "", searchCalls: "", otherUnits: "" });
  const [limitsEditing, setLimitsEditing] = useState(false); const [approved, setApproved] = useState(false);
  const [timeZone, setTimeZone] = useState(() => Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC");
  const [busy, setBusy] = useState(true); const [error, setError] = useState("");

  const finish = useCallback(() => { if (closed.current) return; closed.current = true; fence.invalidate(); dialog.current?.close(); callbacks.current.onClose(); }, [fence]);
  const refresh = useCallback(async (initial = false) => {
    if (pending.current && !initial) return; pending.current = true; setBusy(true); setError(""); setApproved(false); setEstimate(undefined);
    const current = fence.begin();
    try { const result = await loadQuotaForChannel(expectedChannelId, current); if (result) { setSnapshot(result); setLimitInputs({ uploadCalls: result.limits.uploadCalls?.toString() ?? "", searchCalls: result.limits.searchCalls?.toString() ?? "", otherUnits: result.limits.otherUnits?.toString() ?? "" }); setLimitsEditing(false); } }
    catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "The local quota snapshot could not be loaded.")); }
    finally { if (current()) { pending.current = false; setBusy(false); } }
  }, [expectedChannelId, fence]);

  useEffect(() => { const element = dialog.current; const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined; element?.showModal(); heading.current?.focus(); return () => { element?.close(); if (previous?.isConnected) previous.focus(); }; }, []);
  useEffect(() => { void refresh(true); return () => fence.invalidate(); }, [refresh, fence]);
  useEffect(() => { if (openedChannel.current !== expectedChannelId) finish(); }, [expectedChannelId, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step, view]);

  const proposed = (): QuotaProjectLimits => limitsEditing ? { uploadCalls: parseQuotaLimit(limitInputs.uploadCalls), searchCalls: parseQuotaLimit(limitInputs.searchCalls), otherUnits: parseQuotaLimit(limitInputs.otherUnits) } : snapshot?.limits ?? {};
  let proposedLimits: QuotaProjectLimits = snapshot?.limits ?? {}; let invalidLimits = false;
  try { proposedLimits = proposed(); } catch { invalidLimits = true; }
  const changed = Boolean(snapshot && quotaLimitsChanged(snapshot.limits, proposedLimits));
  const timezoneValid = validQuotaTimeZone(timeZone);
  const countsValid = validQuotaChoices(choices);
  const update = (next: QuotaChoices) => { setChoices(next); setEstimate(undefined); setApproved(false); };
  const number = (key: keyof Omit<QuotaChoices, "playlists" | "refreshLibrary">, value: string) => update({ ...choices, [key]: value === "" ? 0 : Number(value) });
  const calculate = async () => {
    if (!snapshot || pending.current || invalidLimits || !timezoneValid || !countsValid) return;
    pending.current = true; setBusy(true); setError(""); setApproved(false);
    const current = fence.begin();
    try { const result = await estimateQuotaForChannel(expectedChannelId, snapshot.scopeId, choices, current); if (result) { setEstimate(result); setSnapshot(result.snapshot); setStep(1); } }
    catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "The estimate could not be prepared. No provider request was made.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  };
  const save = async () => {
    if (!snapshot || !estimate || pending.current || !approved || invalidLimits || !changed) return;
    pending.current = true; setBusy(true); setError(""); const current = fence.begin();
    try { const saved = await saveReviewedQuotaLimits(expectedChannelId, snapshot.scopeId, proposed(), approved, current); if (saved) { callbacks.current.onSaved?.(); finish(); } }
    catch (problem) { if (current()) { setApproved(false); setError(workspaceErrorMessage(problem, "Planning limits were not confirmed. Refresh before trying again.")); } }
    finally { pending.current = false; if (current()) setBusy(false); }
  };
  const resource = async (kind: "console" | "extension" | "costs") => { try { await openQuotaResource(kind); } catch (problem) { setError(workspaceErrorMessage(problem, "The official resource could not be opened.")); } };

  return <dialog ref={dialog} aria-modal="true" aria-labelledby={`${id}-heading`} aria-describedby={`${id}-description`} onCancel={(event) => { event.preventDefault(); finish(); }} className="m-auto max-h-[90dvh] w-[min(58rem,calc(100%-2rem))] max-w-none overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-xl backdrop:bg-black/60">
    <header className="border-b border-line px-5 pt-5 pb-4 sm:px-7"><div className="flex items-start justify-between gap-4"><div><p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">Quota planner · {activeChannel || "Connected channel"}</p><h2 ref={heading} tabIndex={-1} id={`${id}-heading`} className="m-0 text-xl font-semibold outline-none">{view === "google" ? "Check quota from Google" : step === 0 ? "Choose operations to estimate" : step === 1 ? "Estimate API usage by bucket" : "Review limits and next steps"}</h2></div><button type="button" aria-label="Close quota planner" className={`${button} px-3`} onClick={finish}><X size={18} aria-hidden="true" /></button></div><p id={`${id}-description`} className="mt-3 mb-0 max-w-2xl text-sm text-muted">Fetch Google-reported project quota, or estimate a plan from this device. Usage samples can be delayed; channel upload allowance remains unknown.</p><nav aria-label="Quota report source" className="mt-4 flex flex-wrap gap-2"><button type="button" aria-pressed={view === "google"} className={view === "google" ? primary : button} onClick={() => setView("google")}>Google report</button><button type="button" aria-pressed={view === "estimate"} className={view === "estimate" ? primary : button} onClick={() => setView("estimate")}>Local estimate</button></nav>{view === "estimate" && <ol aria-label="Quota planning steps" className="mt-5 mb-0 flex list-none gap-2 p-0">{steps.map((label, index) => <li key={label} aria-current={step === index ? "step" : undefined} className={`flex flex-1 items-center gap-2 rounded-md px-2 py-2 text-xs font-semibold ${step === index ? "bg-subtle-surface text-ink" : "text-muted"}`}><span>{index + 1}.</span>{label}</li>)}</ol>}</header>
    <div className="grid gap-5 px-5 py-5 sm:px-7">
      {error && <p role="alert" className="m-0 rounded-md bg-subtle-surface p-3 text-sm">{error}</p>}
      {!snapshot && <div className="rounded-lg bg-subtle-surface p-4 text-sm">{busy ? <p role="status" className="m-0">Loading local quota records…</p> : <><p className="mt-0 mb-3">Local quota records are unavailable. The planner has not changed saved work.</p><button className={button} type="button" onClick={() => void refresh()}>Retry local snapshot</button></>}</div>}
      {snapshot && view === "google" && <LiveQuotaPanel expectedChannelId={expectedChannelId} expectedProjectId={snapshot.configuredProjectId} timeZone={timeZone} />}
      {snapshot && view === "estimate" && step === 0 && <>
        <p className="m-0 text-sm"><strong>Configured Cloud project:</strong> {snapshot.configuredProjectId || "Unknown · no project identity in OAuth JSON"}<span className="mt-1 block text-xs text-muted">Project identity comes from the connected OAuth JSON. These counts are your planning assumptions; nothing is scheduled from this dialog.</span></p>
        <div className="grid gap-4 sm:grid-cols-2">{fields.map(([key, label]) => <label key={key} className="grid gap-2 text-sm font-semibold">{label}<input type="number" min="0" max="1000000" step="1" value={choices[key]} className={control} disabled={busy} onChange={(event) => number(key, event.target.value)} /></label>)}</div>
        {!countsValid && <p role="alert" className="m-0 text-sm">Use whole-number operation counts within the displayed limits.</p>}
        <fieldset className="grid gap-3 rounded-lg border border-line bg-subtle-surface p-4"><legend className="px-1 text-sm font-semibold">Reads, pagination and retry reserve</legend><label className="flex items-start gap-3 text-sm"><input type="checkbox" checked={choices.refreshLibrary} disabled={busy} onChange={(event) => update({ ...choices, refreshLibrary: event.target.checked })} className="mt-1 h-4 w-4 shrink-0 accent-brand" /><span>Include a fresh upload-library read in this estimate</span></label>{choices.refreshLibrary && <label className="grid gap-2 text-sm">Library video count (50 per page)<input type="number" min="0" max="1000000" step="1" disabled={busy} className={control} value={choices.libraryVideos} onChange={(event) => number("libraryVideos", event.target.value)} /></label>}<div className="grid gap-4 sm:grid-cols-2"><label className="grid gap-2 text-sm">Extra processing checks per uploaded video<input type="number" min="0" max="100" step="1" value={choices.processingChecksPerVideo} className={control} disabled={busy} onChange={(event) => number("processingChecksPerVideo", event.target.value)} /></label><label className="grid gap-2 text-sm">Additional request attempts to reserve<input type="number" min="0" max="5" step="1" value={choices.retryReserve} className={control} disabled={busy} onChange={(event) => number("retryReserve", event.target.value)} /></label></div><p className="m-0 text-xs text-muted">Reserve estimates include possible extra calls. They do not authorize repeating an uncertain write or creating a second upload.</p></fieldset>
        <fieldset className="grid gap-3 rounded-lg border border-line p-4"><legend className="px-1 text-sm font-semibold">Playlist membership</legend><p className="m-0 text-xs text-muted">Each playlist adds an owner read, at least one membership page, and only the new memberships remaining after skip-existing review.</p>{choices.playlists.map((playlist, index) => <div key={index} className="grid gap-3 rounded-md bg-subtle-surface p-3 sm:grid-cols-[1fr_1fr_auto]"><label className="grid gap-2 text-sm">Playlist {index + 1}: existing entries<input type="number" min="0" max="1000000" step="1" disabled={busy} className={control} value={playlist.existingItems} onChange={(event) => update({ ...choices, playlists: choices.playlists.map((item, position) => position === index ? { ...item, existingItems: Number(event.target.value) } : item) })} /></label><label className="grid gap-2 text-sm">Playlist {index + 1}: new memberships<input type="number" min="0" max="1000000" step="1" disabled={busy} className={control} value={playlist.additions} onChange={(event) => update({ ...choices, playlists: choices.playlists.map((item, position) => position === index ? { ...item, additions: Number(event.target.value) } : item) })} /></label><button type="button" className={`${button} self-end`} disabled={busy} aria-label={`Remove playlist ${index + 1} from estimate`} onClick={() => update({ ...choices, playlists: choices.playlists.filter((_, position) => position !== index) })}>Remove</button></div>)}<button type="button" className={`${button} justify-self-start`} disabled={busy || choices.playlists.length >= 64} onClick={() => update({ ...choices, playlists: [...choices.playlists, { existingItems: 0, additions: 0 }] })}>Add playlist assumptions</button></fieldset>
        <fieldset className="grid gap-3 rounded-lg border border-line p-4"><legend className="px-1 text-sm font-semibold">Optional project planning limits</legend><label className="flex items-start gap-3 text-sm"><input type="checkbox" checked={limitsEditing} disabled={busy || !snapshot.configuredProjectId} className="mt-1 h-4 w-4 shrink-0 accent-brand" onChange={(event) => { setLimitsEditing(event.target.checked); setApproved(false); }} /><span>Enter the daily limits shown in this project’s official Console</span></label>{!snapshot.configuredProjectId && <p className="m-0 text-xs text-muted">Project identity is unknown, so limits remain unknown. Check the official Console rather than changing credentials to avoid a quota.</p>}{limitsEditing && <div className="grid gap-4 sm:grid-cols-3">{([ ["uploadCalls", "Video upload calls"], ["searchCalls", "Search calls"], ["otherUnits", "Other API units"] ] as const).map(([key, label]) => <label key={key} className="grid gap-2 text-sm">{label}<input type="text" inputMode="numeric" placeholder="Unknown" className={control} disabled={busy} value={limitInputs[key]} onChange={(event) => { setLimitInputs({ ...limitInputs, [key]: event.target.value }); setApproved(false); }} /></label>)}</div>}<p className="m-0 text-xs text-muted">Blank means unknown; zero is a deliberate planning limit. Saving these references happens only after Review and never changes Google Cloud settings.</p>{invalidLimits && <p role="alert" className="m-0 text-sm">Use whole-number limits up to 1,000,000,000,000, or leave the field blank.</p>}</fieldset>
        <label className="grid gap-2 text-sm font-semibold">Your timezone<input type="text" className={control} value={timeZone} disabled={busy} onChange={(event) => setTimeZone(event.target.value)} /><span className="text-xs font-normal text-muted">For example: Asia/Singapore, Europe/London, America/New_York.</span></label>{!timezoneValid && <p role="alert" className="m-0 text-sm">Choose a valid timezone to convert the Pacific reset.</p>}
        <QuotaHoldSummary snapshot={snapshot} timeZone={timeZone} />
      </>}
      {view === "estimate" && estimate && step > 0 && <QuotaEstimateReview estimate={estimate} proposedLimits={proposedLimits} timeZone={timeZone} />}
      {view === "estimate" && step === 2 && estimate && <><div className="rounded-lg bg-subtle-surface p-4 text-sm"><h3 className="mt-0 mb-2 text-sm font-semibold">Keep the reviewed queue and existing pauses</h3><p className="m-0">This planner only estimates operations and optionally saves local reference limits. The provider determines acceptance; requests do not resume from this dialog.</p></div><div className="flex flex-wrap gap-2"><button type="button" className={button} onClick={() => void resource("console")}><ExternalLink size={16} aria-hidden="true" />Official quota Console</button><button type="button" className={button} onClick={() => void resource("extension")}><ExternalLink size={16} aria-hidden="true" />Request quota extension</button><button type="button" className={button} onClick={() => void resource("costs")}>Official cost table</button></div>{changed && <label className="flex items-start gap-3 rounded-lg border border-line bg-subtle-surface p-4 text-sm"><input type="checkbox" className="mt-1 h-4 w-4 shrink-0 accent-brand" checked={approved} disabled={busy} onChange={(event) => setApproved(event.target.checked)} /><span>I reviewed these local planning limits for {snapshot?.configuredProjectId}. They do not establish remaining quota or change Cloud settings, saved uploads or pauses.</span></label>}</>}
    </div>
    <footer className="flex flex-wrap justify-between gap-3 border-t border-line px-5 py-4 sm:px-7"><div className="flex flex-wrap gap-2">{view === "estimate" && step > 0 && <button type="button" className={button} disabled={busy} onClick={() => { setApproved(false); setStep(step - 1); }}><ArrowLeft size={16} aria-hidden="true" />Back</button>}<button type="button" className={button} onClick={finish}>Cancel</button></div><div className="flex flex-wrap gap-2">{view === "google" ? <button type="button" className={primary} onClick={finish}>Done</button> : step === 0 ? <><button type="button" className={button} disabled={busy} onClick={() => void refresh()}><RefreshCw size={16} aria-hidden="true" />Refresh local records</button><button type="button" className={primary} disabled={busy || !snapshot || invalidLimits || !timezoneValid || !countsValid} onClick={() => void calculate()}>{busy ? <LoaderCircle size={16} className="animate-spin" aria-hidden="true" /> : <Calculator size={16} aria-hidden="true" />}{busy ? "Preparing…" : "Next: Estimate"}</button></> : step === 1 ? <button type="button" className={primary} disabled={busy || !estimate} onClick={() => setStep(2)}>Next: Review<ArrowRight size={16} aria-hidden="true" /></button> : changed ? <button type="button" className={primary} disabled={busy || !approved || invalidLimits} onClick={() => void save()}>{busy ? "Saving…" : "Save planning limits"}</button> : <button type="button" className={primary} onClick={finish}>Done</button>}</div></footer>
  </dialog>;
}
