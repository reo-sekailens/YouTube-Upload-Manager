import { useCallback, useEffect, useId, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, ExternalLink, LoaderCircle, RefreshCw, ShieldCheck } from "lucide-react";
import { loadConnectionSettings } from "../lib/local";
import { beginLiveQuotaAuthorization, cancelLiveQuotaAuthorization, liveQuotaStatusMatches, loadLiveQuotaStatus, openLiveQuotaAuthorization, refreshLiveQuota, reportedQuotaNumber, type LiveQuotaSnapshot, type LiveQuotaStatus } from "../lib/live-quota";
import { formatQuotaTime } from "../lib/quota-planner";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";

const button = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";

export async function readLiveQuotaForChannel(channelId: string, projectId: string, current: () => boolean, refresh = false) {
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  const status = await (refresh ? refreshLiveQuota(channelId) : loadLiveQuotaStatus(channelId));
  assertWorkspaceChannel(await loadConnectionSettings(), channelId);
  if (!current()) return undefined;
  if (!liveQuotaStatusMatches(status, channelId, projectId)) throw new Error("The connected Cloud project changed. Reopen the quota report.");
  return status;
}

export function LiveQuotaReport({ snapshot, projectId, timeZone }: { snapshot: LiveQuotaSnapshot; projectId: string; timeZone: string }) {
  return <section aria-label="Google-reported project quota" className="grid min-w-0 gap-4">
    <div><h3 className="m-0 text-base font-semibold">Google-reported quota</h3><p className="mt-1 mb-0 wrap-anywhere text-sm text-muted">Project {projectId} · all apps and devices using this project</p></div>
    <div className="grid gap-3 sm:grid-cols-2"><div className="rounded-md border border-line bg-subtle-surface p-3"><p className="m-0 text-xs text-muted">Fetched from Google</p><p className="mt-1 mb-0 text-sm font-semibold">{formatQuotaTime(snapshot.fetchedAt, timeZone)} · {timeZone}</p></div><div className="rounded-md border border-line bg-subtle-surface p-3"><p className="m-0 text-xs text-muted">Pacific quota day</p><p className="mt-1 mb-0 text-sm">{formatQuotaTime(snapshot.periodStart, timeZone)} – {formatQuotaTime(snapshot.periodEnd, timeZone)}</p></div></div>
    <p className="m-0 text-sm text-muted">Google samples usage every minute. Reports can lag by {Math.ceil(snapshot.metricLagSeconds / 60)} minutes after sampling; remaining quota is the latest reported amount, not an instantaneous allowance.</p>
    {snapshot.limits.length ? <div className="grid gap-3">{snapshot.limits.map((limit, index) => <article key={`${limit.metric}-${index}`} className="min-w-0 rounded-lg border border-line p-4">
      <h4 className="m-0 wrap-anywhere text-sm font-semibold">{limit.displayName}</h4>
      <dl className="mt-4 mb-0 grid grid-cols-3 gap-3"><div><dt className="text-xs text-muted">Used</dt><dd className="mt-1 ml-0 wrap-anywhere text-lg font-semibold tabular-nums">{reportedQuotaNumber(limit.used)}</dd></div><div><dt className="text-xs text-muted">Daily limit</dt><dd className="mt-1 ml-0 wrap-anywhere text-lg font-semibold tabular-nums">{reportedQuotaNumber(limit.effectiveLimit)}</dd></div><div><dt className="text-xs text-muted">Remaining</dt><dd className="mt-1 ml-0 wrap-anywhere text-lg font-semibold tabular-nums">{reportedQuotaNumber(limit.remaining)}</dd></div></dl>
      <p className="mt-3 mb-0 text-xs text-muted">{limit.sampleThrough ? `Usage sampled through ${formatQuotaTime(limit.sampleThrough, timeZone)} · ${timeZone}` : "No usage sample returned; usage and remaining quota are unknown."}</p>
      {limit.state !== "reported" && <p className="mt-2 mb-0 text-xs text-muted">{limit.state === "unsupported_bucket" ? "Google has not exposed a supported usage counter for this bucket." : limit.state === "limit_unavailable" ? "Google did not report an effective limit for this bucket." : "Google has not reported usage for this quota day. Missing data does not mean zero usage."}</p>}
      <details className="mt-3 text-xs text-muted"><summary className="cursor-pointer">Quota metric details</summary><p className="my-2 wrap-anywhere">{limit.metric} · {limit.unit}</p>{Object.entries(limit.dimensions).map(([name, value]) => <p key={name} className="my-1 wrap-anywhere">{name}: {value}</p>)}</details>
    </article>)}</div> : <p className="m-0 rounded-md bg-subtle-surface p-4 text-sm">Google returned no supported daily quota buckets. Remaining quota is unknown.</p>}
    <p className="m-0 text-xs text-muted">This report does not expose YouTube’s separate channel upload allowance. A positive reported balance does not clear a saved provider pause or guarantee the next upload.</p>
  </section>;
}

export function LiveQuotaPanel({ expectedChannelId, expectedProjectId, timeZone }: { expectedChannelId: string; expectedProjectId?: string | null; timeZone: string }) {
  const [status, setStatus] = useState<LiveQuotaStatus>();
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const [setupStep, setSetupStep] = useState(0);
  const [approved, setApproved] = useState(false);
  const pending = useRef(false);
  const attempt = useRef<string | null>(null);
  const setupHeading = useRef<HTMLHeadingElement>(null);
  const approvalId = useId();
  const fence = useWorkspaceRequestFence(`${expectedChannelId}\0${expectedProjectId ?? ""}`);
  const read = useCallback(async (refresh = false, initial = false) => {
    if (!expectedProjectId || (pending.current && !initial)) return;
    pending.current = true; setBusy(true); setError("");
    const current = fence.begin();
    try {
      const next = await readLiveQuotaForChannel(expectedChannelId, expectedProjectId, current, refresh);
      if (next) { setStatus(next); attempt.current = next.phase === "authorizing" ? next.attemptId : null; if (next.phase === "connected") { setSetupStep(0); setApproved(false); } }
    } catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "Google quota could not be loaded. Remaining quota is unknown.")); }
    finally { if (current()) { pending.current = false; setBusy(false); } }
  }, [expectedChannelId, expectedProjectId, fence]);
  useEffect(() => {
    setStatus(undefined); setSetupStep(0); setApproved(false); setError(""); attempt.current = null;
    if (expectedProjectId) void read(false, true); else setBusy(false);
    return () => { fence.invalidate(); const id = attempt.current; attempt.current = null; if (id) void cancelLiveQuotaAuthorization(expectedChannelId, id).catch(() => undefined); };
  }, [expectedChannelId, expectedProjectId, fence, read]);
  useEffect(() => {
    if (status?.phase !== "authorizing") return;
    // This checks native authorization state only. It never fetches Google metrics.
    const interval = window.setInterval(() => { void read(); }, 2000);
    return () => window.clearInterval(interval);
  }, [status?.phase, read]);
  useEffect(() => { if (setupStep) setupHeading.current?.focus(); }, [setupStep]);
  const authorize = async () => {
    if (!approved || pending.current || !expectedProjectId) return;
    pending.current = true; setBusy(true); setError(""); const current = fence.begin();
    let started: string | undefined;
    try {
      assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
      if (!current()) return;
      const authorization = await beginLiveQuotaAuthorization(expectedChannelId); started = authorization.attemptId;
      if (!current()) { await cancelLiveQuotaAuthorization(expectedChannelId, started); return; }
      attempt.current = started;
      await openLiveQuotaAuthorization(authorization);
      if (!current()) return;
      setStatus({ phase: "authorizing", channelId: expectedChannelId, projectId: expectedProjectId, detail: "Waiting for Google approval in your browser.", attemptId: started, snapshot: status?.snapshot ?? null });
    } catch (problem) {
      if (started) { await cancelLiveQuotaAuthorization(expectedChannelId, started).catch(() => undefined); attempt.current = null; }
      if (current()) setError(workspaceErrorMessage(problem, "Read-only Google Cloud access could not be started."));
    } finally { if (current()) { pending.current = false; setBusy(false); } }
  };
  const cancel = async () => {
    const id = attempt.current;
    if (!id || pending.current) return;
    pending.current = true; setBusy(true); setError(""); const current = fence.begin();
    try { const next = await cancelLiveQuotaAuthorization(expectedChannelId, id); if (current() && liveQuotaStatusMatches(next, expectedChannelId, expectedProjectId ?? "")) { attempt.current = null; setStatus(next); setSetupStep(0); setApproved(false); } }
    catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "Google authorization could not be cancelled.")); }
    finally { if (current()) { pending.current = false; setBusy(false); } }
  };
  if (!expectedProjectId) return <p className="m-0 rounded-lg border border-line p-4 text-sm">The OAuth JSON does not identify a Cloud project. Import the correct project’s Desktop OAuth JSON to load Google-reported quota. Remaining quota is unknown.</p>;
  const connected = status?.phase === "connected";
  return <div className="grid min-w-0 gap-5">
    {error && <div role="alert" className="rounded-md bg-warning-surface p-3 text-sm text-warning-ink"><p className="m-0">{error}</p>{status?.snapshot && <p className="mt-2 mb-0">The earlier Google report remains below with its original timestamps.</p>}</div>}
    {!status && <p role="status" className="m-0 text-sm text-muted">{busy ? "Loading saved Google Cloud connection…" : "Google quota is unavailable. Remaining quota is unknown."}</p>}
    {status?.snapshot ? <LiveQuotaReport snapshot={status.snapshot} projectId={expectedProjectId} timeZone={timeZone} /> : <div><h3 className="m-0 text-base font-semibold">Read quota directly from Google</h3><p className="mt-2 mb-0 max-w-2xl text-sm text-muted">Get this project’s reported daily usage, effective limits and remaining quota across apps and devices. Local request estimates are kept separately.</p><p className="mt-3 mb-0 text-sm"><strong>Project:</strong> <span className="wrap-anywhere">{expectedProjectId}</span></p></div>}
    {status?.phase === "authorizing" ? <div role="status" className="rounded-lg border border-line bg-subtle-surface p-4"><p className="m-0 text-sm font-semibold">Waiting for Google approval</p><p className="mt-2 mb-3 text-sm text-muted">Finish read-only access in the browser. Only authorization status is checked automatically; quota metrics load when you choose Refresh from Google.</p><button type="button" className={button} disabled={busy} onClick={() => void cancel()}>Cancel authorization</button></div> : connected && !setupStep ? <div className="flex flex-wrap items-center justify-between gap-3"><p className="m-0 text-xs text-muted">Read-only Cloud access connected. Quota is fetched only on request.</p><div className="flex flex-wrap gap-2"><button type="button" className={button} disabled={busy} onClick={() => { setApproved(false); setSetupStep(1); }}>Review Cloud access</button><button type="button" className={primary} disabled={busy} onClick={() => void read(true)}>{busy ? <LoaderCircle size={16} className="animate-spin" aria-hidden="true" /> : <RefreshCw size={16} aria-hidden="true" />}{busy ? "Fetching Google report…" : "Refresh from Google"}</button></div></div> : setupStep ? <section aria-label="Connect Google Cloud quota access" className="rounded-lg border border-line bg-subtle-surface p-4">
      <p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">Step {setupStep} of 2 · Read-only Cloud access</p>
      <h3 ref={setupHeading} tabIndex={-1} className="m-0 text-base font-semibold outline-none">{setupStep === 1 ? "Choose an account with project access" : "Review Google Cloud access"}</h3>
      <p className="mt-3 mb-0 text-sm text-muted">Use a Google account allowed to read project details, quota and Monitoring metrics for <strong className="wrap-anywhere text-ink">{expectedProjectId}</strong>. This may be a different account from your YouTube channel. The Service Usage Viewer and Monitoring Viewer roles provide this read access.</p>
      <p className="mt-3 mb-0 text-sm text-muted">Google will request read-only Cloud and Monitoring permissions. Credentials stay in the operating system’s protected storage. This connection cannot upload videos or change project settings.</p>
      {setupStep === 2 && <label htmlFor={approvalId} className="mt-4 flex items-start gap-3 text-sm"><input id={approvalId} type="checkbox" className="mt-1 h-4 w-4 shrink-0 accent-brand" disabled={busy} checked={approved} onChange={(event) => setApproved(event.target.checked)} /><span>Allow this app to read Google-reported quota for this project.</span></label>}
      <div className="mt-5 flex flex-wrap justify-between gap-3"><button type="button" className={button} disabled={busy} onClick={() => { setSetupStep(setupStep - 1); setApproved(false); }}>{setupStep === 2 && <ArrowLeft size={16} aria-hidden="true" />}{setupStep === 1 ? "Cancel setup" : "Back"}</button>{setupStep === 1 ? <button type="button" className={primary} disabled={busy} onClick={() => setSetupStep(2)}>Next: Review access<ArrowRight size={16} aria-hidden="true" /></button> : <button type="button" className={primary} disabled={busy || !approved} onClick={() => void authorize()}><ExternalLink size={16} aria-hidden="true" />Authorize with Google</button>}</div>
    </section> : <div className="rounded-lg border border-line bg-subtle-surface p-4">{status?.detail && <p className="mt-0 mb-3 text-sm text-muted">{status.detail}</p>}<button type="button" className={primary} disabled={busy} onClick={() => { setApproved(false); setSetupStep(1); }}><ShieldCheck size={16} aria-hidden="true" />Set up Google quota access</button><p className="mt-3 mb-0 text-xs text-muted">Remaining quota stays unknown until Google reports both a limit and usage. Missing data is never counted as zero.</p></div>}
    {!status && !busy && <button type="button" className={`${button} justify-self-start`} onClick={() => void read()}>Retry connection status</button>}
  </div>;
}
