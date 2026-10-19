import { ArrowLeft, ArrowRight, CheckCircle2, FileJson, LoaderCircle, Plus, RefreshCw, ShieldCheck, Users, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { beginChannelProfileAuthorization, cancelChannelProfileReview, commitChannelProfile, loadChannelProfileReview, loadChannelProfiles, prepareChannelProfile, profileWorkspaceMatches, removeChannelProfile, switchChannelProfile, verifiedProfileReview, type ChannelProfile, type ChannelProfileReview, type ChannelProfileWorkspace } from "../lib/channel-profiles";
import { openAndCopyGoogleAuthorization, validateGoogleAuthorizationUrl } from "../lib/google-authorization";
import { loadConnectionSettings } from "../lib/local";
import type { ConnectionSettings, YouTubeConnectionStart } from "../lib/types";
import { useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import "./ChannelProfilesDialog.lazy.css";

export type ChannelProfilesDialogProps = {
  expectedChannelId?: string;
  activeChannel?: string;
  onChanged: (settings?: ConnectionSettings) => void;
  onClose: () => void;
  /** Reuse the installed-app consent launcher; authorization remains native-owned. */
  onAuthorize?: (started: YouTubeConnectionStart) => Promise<unknown>;
};
const button = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";

export async function assertProfileConnection(expectedChannelId?: string) {
  const settings = await loadConnectionSettings();
  if ((settings.activeChannelId ?? undefined) !== expectedChannelId)
    throw new Error("The selected channel changed. Open saved profiles again before continuing.");
  return settings;
}
export async function loadProfilesForScope(expectedChannelId: string | undefined, current: () => boolean) {
  await assertProfileConnection(expectedChannelId);
  if (!current()) return undefined;
  const workspace = await loadChannelProfiles(expectedChannelId);
  if (!current()) return undefined;
  if (!profileWorkspaceMatches(workspace, expectedChannelId)) throw new Error("The profile list did not match the selected channel. Refresh before continuing.");
  return workspace;
}
export async function prepareProfileForScope(path: string, expectedChannelId: string | undefined, current: () => boolean) {
  await assertProfileConnection(expectedChannelId);
  if (!current()) return undefined;
  const review = await prepareChannelProfile(path, expectedChannelId);
  if (!current()) { await cancelChannelProfileReview(review.reviewId); return undefined; }
  if (!review.reviewId || review.state !== "ready" || review.channelId) {
    await cancelChannelProfileReview(review.reviewId);
    throw new Error("The account preparation was not confirmed. Choose its Desktop OAuth JSON again.");
  }
  return review;
}
export async function authorizeProfileForScope(review: ChannelProfileReview, expectedChannelId: string | undefined, current: () => boolean) {
  if (review.state !== "ready") throw new Error("Choose a fresh account configuration before starting Google authorization.");
  await assertProfileConnection(expectedChannelId);
  if (!current()) return undefined;
  const started = await beginChannelProfileAuthorization(review.reviewId, expectedChannelId);
  if (!current()) { await cancelChannelProfileReview(review.reviewId); return undefined; }
  try { validateGoogleAuthorizationUrl(started.authorizationUrl); } catch (failure) { await cancelChannelProfileReview(review.reviewId); throw failure; }
  return started;
}
export async function readProfileVerification(reviewId: string, expectedChannelId: string | undefined, current: () => boolean) {
  await assertProfileConnection(expectedChannelId);
  if (!current()) return undefined;
  const loaded = await loadChannelProfileReview(reviewId);
  if (!current()) return undefined;
  if (loaded.reviewId !== reviewId || (loaded.state === "verified" && !verifiedProfileReview(loaded)))
    throw new Error("Google channel verification did not match this account review. Cancel and add it again.");
  return loaded;
}
export async function saveProfileForScope(review: ChannelProfileReview, expectedChannelId: string | undefined, approval: boolean, current: () => boolean) {
  if (!approval || !verifiedProfileReview(review)) throw new Error("Verify the channel and approve its account review before saving.");
  await assertProfileConnection(expectedChannelId);
  if (!current()) return undefined;
  const workspace = await commitChannelProfile(review.reviewId, expectedChannelId, approval);
  if (!current()) return undefined;
  if (!profileWorkspaceMatches(workspace, expectedChannelId) || !workspace.profiles.some((profile) => profile.channelId === review.channelId))
    throw new Error("The saved profile was not confirmed. Refresh saved profiles before continuing.");
  return workspace;
}
export async function changeReviewedProfile(workspace: ChannelProfileWorkspace, profile: ChannelProfile, action: "switch" | "remove", expectedChannelId: string | undefined, approval: boolean, current: () => boolean) {
  if (!approval || !workspace.canSwitch || !profileWorkspaceMatches(workspace, expectedChannelId) || !workspace.profiles.some((saved) => saved.profileId === profile.profileId && saved.channelId === profile.channelId) || (action === "switch" && profile.selected))
    throw new Error("Refresh, choose an available profile and approve its review before continuing.");
  await assertProfileConnection(expectedChannelId);
  if (!current()) return undefined;
  const result = action === "switch" ? await switchChannelProfile(profile.profileId, expectedChannelId, approval) : await removeChannelProfile(profile.profileId, expectedChannelId, approval);
  if (!current()) return undefined;
  const target = action === "switch" ? profile.channelId : profile.selected ? undefined : expectedChannelId;
  if ((result.activeChannelId ?? undefined) !== target || (action === "switch" && !result.connected))
    throw new Error("The selected channel result was not confirmed. Refresh the account before continuing.");
  return result;
}

export function ChannelProfileList({ workspace, page, busy, onPage, onSelect }: { workspace: ChannelProfileWorkspace; page: number; busy: boolean; onPage: (page: number) => void; onSelect: (profile: ChannelProfile, action: "switch" | "remove") => void }) {
  const pages = Math.max(1, Math.ceil(workspace.profiles.length / 20)); const visible = Math.min(page, pages);
  return <section aria-label="Saved channel profiles" className="grid min-w-0 gap-4">
    {workspace.holdReason ? <p className="m-0 rounded-lg bg-warning-surface p-4 text-sm leading-relaxed text-warning-ink">{workspace.holdReason}</p> : null}
    {workspace.profiles.length ? <ul className="m-0 grid list-none gap-3 p-0">{workspace.profiles.slice((visible - 1) * 20, visible * 20).map((profile) => <li className="min-w-0 rounded-lg border border-line bg-subtle-surface p-4" key={profile.profileId}><div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><strong className="block break-words text-base font-semibold">{profile.channelName}</strong><span className="mt-1 block break-all text-xs text-muted">{profile.channelId}</span>{profile.selected ? <span className="mt-3 inline-flex items-center gap-1 text-xs font-semibold"><CheckCircle2 size={15} aria-hidden="true" />Selected channel</span> : <span className="mt-3 block text-xs text-muted">Queue held until explicitly selected</span>}</div><div className="flex flex-wrap gap-2">{!profile.selected ? <button className={button} type="button" disabled={busy || !workspace.canSwitch} aria-label={`Review switching to ${profile.channelName}`} onClick={() => onSelect(profile, "switch")}>Switch…</button> : null}<button className={button} type="button" disabled={busy || !workspace.canSwitch} aria-label={`Review removing ${profile.channelName}`} onClick={() => onSelect(profile, "remove")}>Remove…</button></div></div><dl className="mt-4 mb-0 grid gap-3 text-xs sm:grid-cols-2"><div className="min-w-0"><dt className="font-semibold text-muted">Cloud project</dt><dd className="mt-1 ml-0 break-all">{profile.projectId || "Unknown project identity"}</dd></div><div><dt className="font-semibold text-muted">Management permission</dt><dd className="mt-1 ml-0">{profile.managementAuthorized ? "Separately authorized for this profile" : "Not granted for this profile"}</dd></div></dl></li>)}</ul> : <p className="m-0 rounded-lg bg-subtle-surface p-4 text-sm leading-relaxed text-muted">No saved channel profiles. Add an account, verify its YouTube channel, then review before saving.</p>}
    {pages > 1 ? <nav aria-label="Profile pages" className="flex flex-wrap items-center justify-between gap-3"><button className={button} disabled={busy || visible === 1} onClick={() => onPage(visible - 1)} type="button">Previous</button><span className="text-xs text-muted">Page {visible} of {pages}</span><button className={button} disabled={busy || visible === pages} onClick={() => onPage(visible + 1)} type="button">Next page</button></nav> : null}
  </section>;
}
export function ChannelProfileIdentity({ channelName, channelId, projectId }: { channelName?: string | null; channelId?: string | null; projectId?: string | null }) {
  return <dl aria-label="Verified channel identity" className="m-0 grid gap-4 rounded-lg border border-line bg-subtle-surface p-4"><div className="min-w-0"><dt className="text-xs font-semibold text-muted">YouTube channel</dt><dd className="mt-1 ml-0 break-words text-base font-semibold">{channelName || "Awaiting Google verification"}</dd>{channelId ? <dd className="mt-1 ml-0 break-all text-xs text-muted">{channelId}</dd> : null}</div><div className="min-w-0"><dt className="text-xs font-semibold text-muted">Cloud project</dt><dd className="mt-1 ml-0 break-all text-sm">{projectId || "Unknown project identity"}</dd></div></dl>;
}

export function ChannelProfilesDialog({ expectedChannelId, activeChannel, onClose, onChanged, onAuthorize }: ChannelProfilesDialogProps) {
  const id = useId(); const dialog = useRef<HTMLDialogElement>(null); const heading = useRef<HTMLHeadingElement>(null);
  const callbacks = useRef({ onClose, onChanged, onAuthorize }); callbacks.current = { onClose, onChanged, onAuthorize };
  const openedScope = useRef(expectedChannelId); const closed = useRef(false); const mounted = useRef(false); const pending = useRef(""); const reviewRef = useRef<ChannelProfileReview | undefined>(undefined); const authorization = useRef<YouTubeConnectionStart | undefined>(undefined);
  // A reviewed switch changes the provider revision before this dialog receives its receipt.
  const fence = useWorkspaceRequestFence(`profiles:${expectedChannelId ?? "disconnected"}`, false);
  const [workspace, setWorkspace] = useState<ChannelProfileWorkspace>(); const [review, setReview] = useState<ChannelProfileReview>(); const [selected, setSelected] = useState<ChannelProfile>();
  const [mode, setMode] = useState<"profiles" | "add" | "switch" | "remove">("profiles"); const [step, setStep] = useState(0); const [page, setPage] = useState(1); const [approved, setApproved] = useState(false); const [busy, setBusy] = useState(""); const [error, setError] = useState(""); const [notice, setNotice] = useState("");
  const finish = useCallback(() => { if (closed.current) return; closed.current = true; fence.invalidate(); dialog.current?.close(); callbacks.current.onClose(); }, [fence]);
  const run = useCallback(async (kind: string, operation: (current: () => boolean) => Promise<void>) => {
    if (pending.current || closed.current) return;
    pending.current = kind; setBusy(kind); setError("");
    const valid = fence.begin(); const current = () => valid() && mounted.current && !closed.current;
    try { await operation(current); } catch (failure) { if (current()) { setApproved(false); setError(workspaceErrorMessage(failure, "Saved profiles could not be confirmed. Refresh before continuing.")); } }
    finally { pending.current = ""; if (current()) setBusy(""); }
  }, [fence]);
  const close = useCallback(() => run("cancel", async (current) => { if (reviewRef.current && reviewRef.current.state !== "completed") await cancelChannelProfileReview(reviewRef.current.reviewId); if (!current()) return; reviewRef.current = undefined; finish(); }), [finish, run]);
  const refresh = useCallback(() => run("load", async (current) => { const loaded = await loadProfilesForScope(expectedChannelId, current); if (loaded) { setWorkspace(loaded); setApproved(false); } }), [expectedChannelId, run]);
  useEffect(() => { mounted.current = true; const element = dialog.current; const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined; element?.showModal(); heading.current?.focus(); return () => { mounted.current = false; fence.invalidate(); element?.close(); if (!closed.current && reviewRef.current) void cancelChannelProfileReview(reviewRef.current.reviewId).catch(() => undefined); if (previous?.isConnected) previous.focus(); }; }, [fence]);
  useEffect(() => { let cancelled = false; queueMicrotask(() => { if (!cancelled) void refresh(); }); return () => { cancelled = true; }; }, [refresh]);
  useEffect(() => { if (openedScope.current !== expectedChannelId) { const previousReview = reviewRef.current; finish(); if (previousReview) void cancelChannelProfileReview(previousReview.reviewId).catch(() => undefined); } }, [expectedChannelId, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step, mode]);
  const choose = () => run("choose", async (current) => { const { open } = await import("@tauri-apps/plugin-dialog"); const path = await open({ multiple: false, directory: false, filters: [{ name: "Google Desktop OAuth JSON", extensions: ["json"] }] }); if (typeof path !== "string" || !current()) return; if (reviewRef.current) { await cancelChannelProfileReview(reviewRef.current.reviewId); reviewRef.current = undefined; if (current()) setReview(undefined); } if (!current()) return; const prepared = await prepareProfileForScope(path, expectedChannelId, current); if (prepared) { reviewRef.current = prepared; authorization.current = undefined; setReview(prepared); setNotice(""); setApproved(false); setStep(1); } });
  const authorize = () => run("authorize", async (current) => { if (!reviewRef.current) return; const started = await authorizeProfileForScope(reviewRef.current, expectedChannelId, current); if (!started) return; authorization.current = started; const waiting: ChannelProfileReview = { ...reviewRef.current, state: "authorizing" }; reviewRef.current = waiting; setReview(waiting); try { await (callbacks.current.onAuthorize ? callbacks.current.onAuthorize(started) : openAndCopyGoogleAuthorization(started.authorizationUrl)); if (current()) setNotice("Google authorization opened. Choose the YouTube channel to save, then return to verify it here."); } catch (failure) { if (current()) setError(workspaceErrorMessage(failure, "Google authorization could not be opened. Open it again or cancel this account review.")); } });
  const verify = useCallback(() => run("verify", async (current) => { const currentReview = reviewRef.current; if (!currentReview) return; const loaded = await readProfileVerification(currentReview.reviewId, expectedChannelId, current); if (!loaded) return; reviewRef.current = loaded; setReview(loaded); setApproved(false); if (loaded.state === "completed") { const saved = await loadProfilesForScope(expectedChannelId, current); if (!saved) return; if (!loaded.channelId || !saved.profiles.some((profile) => profile.channelId === loaded.channelId)) throw new Error("The saved account result is not confirmed. Refresh the device’s connection before continuing."); reviewRef.current = undefined; callbacks.current.onChanged(); finish(); return; } if (loaded.state === "verified") setNotice("Google verified this channel. Its profile has not been saved yet."); if (loaded.state === "failed" || loaded.state === "cancelled") setError(loaded.detail || "Google authorization did not finish. Go back and choose the JSON again."); }), [expectedChannelId, finish, run]);
  useEffect(() => { if (mode !== "add" || review?.state !== "authorizing") return; let active = true; let timeout: number | undefined; const delays = [500, 1000, 2000, 4000, 8000, 16000, 16000]; const check = async (index: number) => { if (!active) return; await verify(); if (active && index + 1 < delays.length) timeout = window.setTimeout(() => void check(index + 1), delays[index + 1]); }; timeout = window.setTimeout(() => void check(0), delays[0]); return () => { active = false; if (timeout !== undefined) window.clearTimeout(timeout); }; }, [mode, review?.state, verify]);
  const save = () => run("save", async (current) => { if (!reviewRef.current) return; const saved = await saveProfileForScope(reviewRef.current, expectedChannelId, approved, current); if (!saved) return; reviewRef.current = undefined; callbacks.current.onChanged(); finish(); });
  const change = () => run("save", async (current) => { if (!workspace || !selected || (mode !== "switch" && mode !== "remove")) return; const settings = await changeReviewedProfile(workspace, selected, mode, expectedChannelId, approved, current); if (!settings) return; callbacks.current.onChanged(settings); finish(); });
  const reopenGoogle = () => run("open", async (current) => { if (!authorization.current) return; await (callbacks.current.onAuthorize ? callbacks.current.onAuthorize(authorization.current) : openAndCopyGoogleAuthorization(authorization.current.authorizationUrl)); if (current()) setNotice("Google authorization opened again. Return here after completing consent."); });
  const back = () => {
    setApproved(false); setError("");
    if (mode === "add" && step > 0) { setStep((value) => value - 1); return; }
    if (mode === "add" && reviewRef.current) {
      void run("cancel", async (current) => { const staged = reviewRef.current; if (staged) await cancelChannelProfileReview(staged.reviewId); if (!current()) return; reviewRef.current = undefined; authorization.current = undefined; setReview(undefined); const loaded = await loadProfilesForScope(expectedChannelId, current); if (!loaded) return; setWorkspace(loaded); setMode("profiles"); setStep(0); setNotice(""); });
    } else { setMode("profiles"); setStep(0); setNotice(""); }
  };
  const isBusy = Boolean(busy); const availability = workspace?.canSwitch === true; const add = mode === "add"; const choosing = mode === "profiles"; const labels = add ? ["Add account", "Verify channel", "Review"] : ["Choose profile", "Review"];
  const title = choosing ? "Saved YouTube channels" : add ? step === 0 ? "Add a channel profile" : step === 1 ? "Verify the YouTube channel" : "Review the verified profile" : mode === "switch" ? "Review channel switching" : "Review profile removal";
  return <dialog ref={dialog} aria-modal="true" aria-labelledby={`${id}-heading`} aria-describedby={`${id}-description`} onCancel={(event) => { event.preventDefault(); if (!isBusy) void close(); }} className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-3xl overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55">
    <header className="flex items-start justify-between gap-4 border-b border-line p-4 sm:p-6"><div className="min-w-0"><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted">Channel profiles · This device</p><h2 ref={heading} tabIndex={-1} id={`${id}-heading`} className="m-0 text-lg font-bold tracking-tight focus:outline-none">{title}</h2><p id={`${id}-description`} className="mt-2 mb-0 max-w-xl text-sm leading-relaxed text-muted">Only the selected channel dispatches new uploads. Every saved queue, playlist and recovery receipt retains its original channel.</p></div><button className={`${button} shrink-0 px-2`} type="button" aria-label="Close saved profiles" disabled={isBusy} onClick={() => void close()}><X size={20} aria-hidden="true" /></button></header>
    <ol aria-label="Channel profile steps" className="m-0 flex list-none gap-3 border-b border-line bg-subtle-surface px-4 py-3 text-xs sm:px-6">{labels.map((label, index) => <li key={label} aria-current={step === index ? "step" : undefined} className="flex min-w-0 flex-1 items-center gap-2"><span aria-hidden="true" className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-line bg-surface">{index + 1}</span><span>{label}</span></li>)}</ol>
    <div aria-busy={isBusy} className="grid min-w-0 gap-5 p-4 sm:p-6">
      {isBusy ? <p className="m-0 flex items-center gap-2 text-sm text-muted" role="status"><LoaderCircle size={16} aria-hidden="true" className="animate-spin" />{busy === "save" ? "Saving the reviewed profile change…" : busy === "verify" ? "Checking Google channel verification…" : busy === "cancel" ? "Cancelling the staged account review…" : "Reading this device’s profile state…"}</p> : null}
      {error ? <div role="alert" className="rounded-lg bg-warning-surface p-4 text-sm leading-relaxed text-warning-ink"><p className="m-0">{error}</p>{mode === "add" && review ? <button className={`${button} mt-3`} disabled={isBusy} onClick={() => void verify()} type="button"><RefreshCw size={16} aria-hidden="true" />Check account result</button> : choosing ? <button className={`${button} mt-3`} disabled={isBusy} onClick={() => void refresh()} type="button">Retry profile list</button> : null}</div> : null}
      {notice ? <p role="status" className="m-0 text-sm leading-relaxed text-muted">{notice}</p> : null}
      {choosing ? <><p className="m-0 text-sm leading-relaxed"><strong>Selected channel:</strong> {activeChannel || workspace?.profiles.find((profile) => profile.selected)?.channelName || "None"}</p>{workspace ? <ChannelProfileList workspace={workspace} page={page} busy={isBusy} onPage={setPage} onSelect={(profile, action) => { setSelected(profile); setMode(action); setStep(1); setApproved(false); setError(""); }} /> : !isBusy ? <p className="m-0 text-sm text-muted">Saved profiles could not be loaded. Refresh this device’s account state to try again.</p> : null}<p className="m-0 text-xs leading-relaxed text-muted">Cloud quota is shared across profiles using the same project; channel upload pauses remain separate. Switching preserves both limits.</p></> : add ? <>
        {step === 0 ? <><p className="m-0 text-sm leading-relaxed">Choose this account’s Google Desktop OAuth JSON, then approve access through the existing Google consent window. The app verifies the channel before you save its protected profile.</p><button className={`${button} justify-self-start`} disabled={isBusy || !availability} onClick={() => void choose()} type="button"><FileJson size={18} aria-hidden="true" />Choose Desktop OAuth JSON</button><p className="m-0 text-xs leading-relaxed text-muted">Credential material stays in native OS-protected storage. Adding an account keeps {activeChannel || "the current channel"} selected until you explicitly switch.</p></> : <><ChannelProfileIdentity channelId={review?.channelId} channelName={review?.channelName} projectId={review?.projectId} />{step === 1 ? <><p className="m-0 text-sm leading-relaxed text-muted">{review && verifiedProfileReview(review) ? "Google confirmed this channel identity. Check it before proceeding to the final review." : review?.state === "ready" ? "Start Google authorization, choose the intended YouTube channel, then return here." : review?.state === "authorizing" ? "Waiting for Google to verify the channel. You can refresh the result after completing consent." : "Choose the JSON again to start a fresh channel verification."}</p>{review?.state === "ready" ? <button className={`${primary} justify-self-start`} disabled={isBusy || !availability} onClick={() => void authorize()} type="button">Continue to Google<ArrowRight size={16} aria-hidden="true" /></button> : review?.state === "authorizing" ? <div className="flex flex-wrap gap-2"><button className={button} disabled={isBusy} onClick={() => void verify()} type="button"><RefreshCw size={16} aria-hidden="true" />Refresh verification</button>{authorization.current ? <button className={button} disabled={isBusy} onClick={() => void reopenGoogle()} type="button">Open Google again</button> : null}</div> : null}</> : <><p className="m-0 rounded-lg bg-subtle-surface p-4 text-sm leading-relaxed">Save this independently protected channel profile on this device. The selected channel stays {activeChannel || "unchanged"}. Switch to this channel through a separate review when you want its saved queue to dispatch.</p><p className="m-0 text-xs leading-relaxed text-muted">Upload and management permissions belong to this profile. This addition does not clear project quota or channel upload pauses.</p><label className="flex items-start gap-3 text-sm font-semibold leading-relaxed"><input className="mt-1 h-5 w-5 shrink-0 accent-brand" type="checkbox" checked={approved} disabled={isBusy || !availability} onChange={(event) => setApproved(event.target.checked)} /><span>I reviewed the verified channel and approve saving its protected profile on this device.</span></label></>}</>}
      </> : selected ? <><ChannelProfileIdentity channelId={selected.channelId} channelName={selected.channelName} projectId={selected.projectId} /><p className="m-0 rounded-lg bg-subtle-surface p-4 text-sm leading-relaxed">{mode === "switch" ? `Select ${selected.channelName} for new upload dispatch. Existing queue items, folder approvals, playlists and recovery evidence retain their original channel; other profiles wait until selected.` : `Remove the saved connection for ${selected.channelName} on this device${selected.selected ? " and disconnect this selected channel" : ""}. Its queue, original-file references, confirmed video IDs and recovery evidence remain retained.`}</p><p className="m-0 text-xs leading-relaxed text-muted">{mode === "switch" ? "Cloud project quota remains shared across profiles using that project. Switching does not reset either project or channel limits." : "This action does not delete YouTube videos or original files. Reconnect this same channel before resuming its retained queue."}</p>{workspace?.holdReason ? <p className="m-0 text-sm text-warning-ink">{workspace.holdReason}</p> : null}<label className="flex items-start gap-3 text-sm font-semibold leading-relaxed"><input className="mt-1 h-5 w-5 shrink-0 accent-brand" type="checkbox" checked={approved} disabled={isBusy || !availability} onChange={(event) => setApproved(event.target.checked)} /><span>{mode === "switch" ? `I approve switching the selected channel to ${selected.channelName}.` : `I approve removing this device’s saved connection for ${selected.channelName} while retaining its queue and recovery evidence.`}</span></label></> : null}
    </div>
    <footer className="sticky bottom-0 flex flex-wrap items-center justify-between gap-3 border-t border-line bg-surface p-4 sm:px-6"><div className="flex flex-wrap gap-2">{!choosing ? <button className={button} type="button" disabled={isBusy} onClick={back}><ArrowLeft size={16} aria-hidden="true" />Back</button> : null}<button className={button} type="button" disabled={isBusy} onClick={() => void close()}>{choosing ? "Done" : "Cancel"}</button></div><div className="flex flex-wrap gap-2">{choosing ? <><button className={button} type="button" disabled={isBusy} onClick={() => void refresh()}><RefreshCw size={16} aria-hidden="true" />Refresh profiles</button><button className={primary} type="button" disabled={isBusy || !availability} onClick={() => { setMode("add"); setStep(0); setApproved(false); setError(""); }}><Plus size={16} aria-hidden="true" />Add account</button></> : add ? step === 1 ? <button className={primary} type="button" disabled={isBusy || !review || !verifiedProfileReview(review) || !availability} onClick={() => { setStep(2); setApproved(false); }}>Next: Review<ArrowRight size={16} aria-hidden="true" /></button> : step === 2 ? <button className={primary} type="button" disabled={isBusy || !approved || !availability} onClick={() => void save()}><ShieldCheck size={16} aria-hidden="true" />Save channel profile</button> : null : <button className={primary} type="button" disabled={isBusy || !approved || !availability} onClick={() => void change()}><Users size={16} aria-hidden="true" />{mode === "switch" ? "Switch selected channel" : "Remove saved profile"}</button>}</div></footer>
  </dialog>;
}
