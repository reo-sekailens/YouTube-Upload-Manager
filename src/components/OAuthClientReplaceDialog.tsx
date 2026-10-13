import { ArrowLeft, ArrowRight, CheckCircle2, FileJson, LoaderCircle, RefreshCw, ShieldCheck, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { beginOAuthClientReplacement, cancelOAuthClientReplacement, loadConnectionSettings, loadOAuthClientReplacementStatus, prepareOAuthClientReplacement } from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import type { ConnectionSettings, OAuthClientReplacementReview, OAuthClientReplacementStatus, UploadLimitStatus } from "../lib/types";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import "./OAuthClientReplaceDialog.lazy.css";

type OAuthClientReplaceDialogProps = {
  expectedChannelId: string;
  channelName?: string;
  limit?: UploadLimitStatus;
  onReplaced: (settings: ConnectionSettings) => void;
  onClose: () => void;
};

const buttonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primaryButtonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const steps = [
  { key: "choose", label: "Choose JSON", heading: "Choose a replacement JSON", description: "Select a Google Desktop OAuth JSON file for this account." },
  { key: "review", label: "Review", heading: "Review the account change", description: "Check the account and saved limits before continuing to Google." },
  { key: "reconnect", label: "Reconnect", heading: "Reconnect the same channel", description: "Complete Google authorization. The app verifies the channel before replacing its connection." },
] as const;
type ReplacementStep = typeof steps[number]["key"];

function resumeTime(value?: string): string | undefined {
  if (!value) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? undefined : date.toLocaleString();
}

export function OAuthReplacementLimitSummary({ review, limit }: { review?: OAuthClientReplacementReview; limit?: UploadLimitStatus }) {
  const cloudPaused = review ? review.cloudQuotaPaused : limit?.kind === "api_quota";
  const channelPaused = review ? review.channelUploadPaused : limit?.kind === "channel_upload" || Boolean(limit?.channelResumesAt);
  const channelResume = resumeTime(limit?.channelResumesAt ?? (limit?.kind === "channel_upload" ? limit.resumesAt : undefined));
  if (!cloudPaused && !channelPaused) return null;
  return <section aria-label="Saved upload limits" className="grid gap-3 rounded-lg border border-line bg-subtle-surface p-4">
    {cloudPaused ? <div><h3 className="m-0 text-sm font-semibold text-ink">Google Cloud API quota</h3><p className="mt-1 mb-0 text-sm leading-relaxed text-muted">Check the Google Cloud project’s quota first. Replacing its OAuth JSON does not guarantee more API quota.</p></div> : null}
    {channelPaused ? <div><h3 className="m-0 text-sm font-semibold text-ink">YouTube channel upload limit</h3><p className="mt-1 mb-0 text-sm leading-relaxed text-muted">The channel’s saved 24-hour upload pause stays in place{channelResume ? ` until ${channelResume}` : ""}. Changing the JSON does not reset this limit.</p></div> : null}
  </section>;
}

export function OAuthReplacementReview({ review, limit }: { review: OAuthClientReplacementReview; limit?: UploadLimitStatus }) {
  return <div className="grid gap-4">
    <dl aria-label="OAuth replacement details" className="m-0 grid min-w-0 gap-4 rounded-lg border border-line bg-subtle-surface p-4">
      <div><dt className="text-xs font-semibold text-muted">Channel to reconnect</dt><dd className="mt-1 ml-0 break-words text-base font-semibold text-ink">{review.channelName || "Connected channel"}</dd></div>
      <div><dt className="text-xs font-semibold text-muted">Selected configuration</dt><dd className="mt-1 ml-0 text-sm font-semibold text-ink">Validated Google Desktop OAuth JSON</dd></div>
    </dl>
    <OAuthReplacementLimitSummary limit={limit} review={review} />
    <p className="m-0 flex items-start gap-2 text-sm leading-relaxed text-muted"><ShieldCheck aria-hidden="true" className="mt-0.5 shrink-0" size={18} /><span>Your current connection is retained until Google confirms this same channel. Uploads pause during review. Queued videos and saved upload progress remain on this device.</span></p>
  </div>;
}

/** A late preparation owns a native review and must release it without displaying it. */
export async function prepareReplacementForChannel(path: string, expectedChannelId: string, current: () => boolean): Promise<OAuthClientReplacementReview | undefined> {
  assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
  if (!current()) return undefined;
  const review = await prepareOAuthClientReplacement(path, expectedChannelId);
  if (!current() || review.channelId !== expectedChannelId) {
    await cancelOAuthClientReplacement(review.reviewId);
    if (current()) throw new Error("The connected channel changed. Choose the JSON again for this account.");
    return undefined;
  }
  return review;
}

/** Consent starts only from the reviewed, still-connected channel. */
export async function startReplacementForChannel(review: OAuthClientReplacementReview, expectedChannelId: string, current: () => boolean) {
  if (review.channelId !== expectedChannelId) throw new Error("The connected channel changed. Choose the JSON again for this account.");
  assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
  if (!current()) return undefined;
  const started = await beginOAuthClientReplacement(review.reviewId, expectedChannelId);
  if (!current()) {
    await cancelOAuthClientReplacement(review.reviewId);
    return undefined;
  }
  return started;
}

export function replacementCompletedSettings(status: OAuthClientReplacementStatus, expectedChannelId: string): ConnectionSettings | undefined {
  if (status.state !== "completed") return undefined;
  if (!status.settings) throw new Error("The replacement could not be confirmed. Refresh the account connection before trying again.");
  assertWorkspaceChannel(status.settings, expectedChannelId);
  return status.settings;
}

export function OAuthClientReplaceDialog({ expectedChannelId, channelName, limit, onReplaced, onClose }: OAuthClientReplaceDialogProps) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const chooseButton = useRef<HTMLButtonElement>(null);
  const channelAtOpen = useRef(expectedChannelId);
  const callbacks = useRef({ onReplaced, onClose });
  callbacks.current = { onReplaced, onClose };
  const reviewRef = useRef<OAuthClientReplacementReview | undefined>(undefined);
  const authorizationUrl = useRef<string | undefined>(undefined);
  const browserOpenFailed = useRef(false);
  const closed = useRef(false);
  const mounted = useRef(true);
  const pending = useRef(false);
  const reading = useRef(false);
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState<ReplacementStep>("choose");
  const [review, setReview] = useState<OAuthClientReplacementReview>();
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<OAuthClientReplacementStatus>();
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [refreshing, setRefreshing] = useState(false);
  const [openingFailed, setOpeningFailed] = useState(false);
  const stepDetails = steps.find((item) => item.key === step)!;
  const waiting = status?.state === "authorizing";
  const channelMatches = channelAtOpen.current === expectedChannelId;

  const finish = useCallback(() => {
    if (closed.current) return;
    closed.current = true;
    fence.invalidate();
    dialog.current?.close();
    callbacks.current.onClose();
  }, [fence]);

  const close = useCallback(async () => {
    if (pending.current || closed.current) return;
    pending.current = true;
    fence.invalidate();
    setBusy(true);
    setError("");
    try {
      if (reviewRef.current) await cancelOAuthClientReplacement(reviewRef.current.reviewId);
      finish();
    } catch (cause) {
      if (mounted.current && !closed.current) setError(workspaceErrorMessage(cause, "The replacement could not be cancelled. Try cancelling again before closing."));
    } finally {
      pending.current = false;
      if (mounted.current && !closed.current) setBusy(false);
    }
  }, [fence, finish]);

  useEffect(() => {
    mounted.current = true;
    const element = dialog.current;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    element?.showModal();
    chooseButton.current?.focus();
    return () => {
      mounted.current = false;
      fence.invalidate();
      element?.close();
      if (!closed.current && reviewRef.current) void cancelOAuthClientReplacement(reviewRef.current.reviewId).catch(() => undefined);
      if (previousFocus?.isConnected) previousFocus.focus();
    };
  }, [fence]);

  useEffect(() => {
    if (!channelMatches && !busy) void close();
  }, [busy, channelMatches, close]);

  useEffect(() => {
    if (step === "choose") chooseButton.current?.focus();
    else heading.current?.focus();
    dialog.current?.scrollTo({ top: 0 });
  }, [step]);

  const choose = async () => {
    if (pending.current || closed.current || !channelMatches) return;
    pending.current = true;
    setBusy(true);
    setError("");
    const requestCurrent = fence.begin();
    const current = () => mounted.current && !closed.current && requestCurrent();
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, directory: false, filters: [{ name: "Google Desktop OAuth JSON", extensions: ["json"] }] });
      if (typeof selected !== "string" || !current()) return;
      if (reviewRef.current) {
        await cancelOAuthClientReplacement(reviewRef.current.reviewId);
        reviewRef.current = undefined;
        if (current()) setReview(undefined);
      }
      if (!current()) return;
      const prepared = await prepareReplacementForChannel(selected, expectedChannelId, current);
      if (prepared && current()) {
        reviewRef.current = prepared;
        setReview(prepared);
        setStatus({ state: "ready" });
        setNotice("");
        setStep("review");
      }
    } catch (cause) {
      if (current()) setError(workspaceErrorMessage(cause, "The Desktop OAuth JSON could not be prepared. Choose a valid JSON file and try again."));
    } finally {
      pending.current = false;
      if (mounted.current && !closed.current) setBusy(false);
    }
  };

  const acceptStatus = useCallback((loaded: OAuthClientReplacementStatus) => {
    const settings = replacementCompletedSettings(loaded, channelAtOpen.current);
    if (settings) {
      closed.current = true;
      fence.invalidate();
      dialog.current?.close();
      callbacks.current.onReplaced(settings);
      return;
    }
    setStatus(loaded);
    if (loaded.state === "failed") setError(loaded.detail || "Google authorization was not completed. Try reconnecting this same channel.");
    if (loaded.state === "cancelled") setError(loaded.detail || "This replacement was cancelled. Go back and choose the JSON again.");
  }, [fence]);

  const refresh = useCallback(async () => {
    const activeReview = reviewRef.current;
    if (!activeReview || pending.current || reading.current || closed.current || !channelMatches) return;
    reading.current = true;
    setRefreshing(true);
    const requestCurrent = fence.begin();
    const current = () => mounted.current && !closed.current && requestCurrent();
    try {
      const loaded = await loadOAuthClientReplacementStatus(activeReview.reviewId);
      if (current()) {
        if (!browserOpenFailed.current) setError("");
        acceptStatus(loaded);
      }
    } catch (cause) {
      if (current()) setError(workspaceErrorMessage(cause, "Authorization status could not be checked. Refresh to try again."));
    } finally {
      reading.current = false;
      if (mounted.current && !closed.current) setRefreshing(false);
    }
  }, [acceptStatus, channelMatches, fence]);

  useEffect(() => {
    if (!waiting || !review) return;
    let active = true;
    let timeout: number | undefined;
    const check = async (index: number) => {
      if (!active) return;
      await refresh();
      if (active) timeout = window.setTimeout(() => void check(index + 1), Math.min(500 * (2 ** Math.min(index, 3)), 4_000));
    };
    timeout = window.setTimeout(() => void check(0), 250);
    return () => { active = false; if (timeout !== undefined) window.clearTimeout(timeout); };
  }, [waiting, review, refresh]);

  const reconnect = async () => {
    if (!review || pending.current || closed.current || waiting || !channelMatches) return;
    pending.current = true;
    setBusy(true);
    setError("");
    browserOpenFailed.current = false;
    setOpeningFailed(false);
    const requestCurrent = fence.begin();
    const current = () => mounted.current && !closed.current && requestCurrent();
    try {
      const started = await startReplacementForChannel(review, expectedChannelId, current);
      if (!started || !current()) return;
      authorizationUrl.current = started.authorizationUrl;
      setStep("reconnect");
      setStatus({ state: "authorizing" });
      try {
        const copied = await openAndCopyGoogleAuthorization(started.authorizationUrl);
        if (current()) setNotice(copied ? "Google opened and the authorization link was copied. Select the same channel, then return here." : "Google opened for approval. Select the same channel, then return here.");
      } catch (cause) {
        if (current()) {
          browserOpenFailed.current = true;
          setOpeningFailed(true);
          setError(workspaceErrorMessage(cause, "Google authorization could not be opened. Open the authorization link again, or cancel this replacement."));
        }
      }
    } catch (cause) {
      if (current()) {
        setStatus({ state: "failed" });
        setError(workspaceErrorMessage(cause, "Authorization could not be started. Try reconnecting again."));
      }
    } finally {
      pending.current = false;
      if (mounted.current && !closed.current) setBusy(false);
    }
  };

  const reopenGoogle = async () => {
    if (!authorizationUrl.current || pending.current || closed.current || !waiting || !channelMatches) return;
    pending.current = true;
    setBusy(true);
    const requestCurrent = fence.begin();
    const current = () => mounted.current && !closed.current && requestCurrent();
    try {
      const copied = await openAndCopyGoogleAuthorization(authorizationUrl.current);
      if (current()) {
        browserOpenFailed.current = false;
        setOpeningFailed(false);
        setError("");
        setNotice(copied ? "Google opened and the authorization link was copied. Select the same channel, then return here." : "Google opened for approval. Select the same channel, then return here.");
      }
    } catch (cause) {
      if (current()) setError(workspaceErrorMessage(cause, "Google authorization could not be opened. Try again, or cancel this replacement."));
    } finally {
      pending.current = false;
      if (mounted.current && !closed.current) setBusy(false);
    }
  };

  const back = async () => {
    if (pending.current || closed.current) return;
    setError("");
    if (step === "reconnect") {
      pending.current = true;
      setBusy(true);
      fence.invalidate();
      try {
        if (reviewRef.current) await cancelOAuthClientReplacement(reviewRef.current.reviewId);
        reviewRef.current = undefined;
        if (mounted.current && !closed.current) { setReview(undefined); setStatus(undefined); setStep("choose"); }
      } catch (cause) {
        if (mounted.current && !closed.current) setError(workspaceErrorMessage(cause, "The current attempt could not be cancelled. Try again before choosing another JSON."));
      } finally { pending.current = false; if (mounted.current && !closed.current) setBusy(false); }
    } else setStep("choose");
  };

  return <dialog aria-describedby={`${id}-description`} aria-labelledby={`${id}-heading`} aria-modal="true" className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-lg overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55" onCancel={(event) => { event.preventDefault(); void close(); }} ref={dialog}>
    <header className="flex items-start justify-between gap-4 border-b border-line p-4 sm:p-6">
      <div><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted uppercase">Replace account OAuth JSON</p><h2 className="m-0 text-lg font-bold tracking-tight focus:outline-none" id={`${id}-heading`} ref={heading} tabIndex={-1}>{stepDetails.heading}</h2><p className="mt-2 mb-0 text-sm leading-relaxed text-muted" id={`${id}-description`}>{stepDetails.description}</p></div>
      <button aria-label="Close OAuth JSON replacement" className={`${buttonClass} shrink-0 px-2`} disabled={busy} onClick={() => void close()} title="Close (Escape)" type="button"><X aria-hidden="true" size={20} /></button>
    </header>
    <ol aria-label="OAuth JSON replacement steps" className="m-0 flex list-none items-center gap-3 border-b border-line bg-subtle-surface px-4 py-3 sm:px-6">
      {steps.map((item, index) => <li aria-current={step === item.key ? "step" : undefined} className={`flex min-w-0 flex-1 items-center gap-2 text-xs ${step === item.key ? "font-semibold text-active-ink" : "text-muted"}`} key={item.key}><span aria-hidden="true" className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-full ${step === item.key ? "bg-active-surface" : "border border-line bg-surface"}`}>{index + 1}</span><span>{item.label}</span></li>)}
    </ol>
    <div aria-busy={busy} className="grid min-w-0 gap-5 p-4 sm:p-6">
      {step === "choose" ? <>
        <div className="grid gap-3"><p className="m-0 break-words text-base font-semibold text-ink">{channelName?.trim() || "Connected channel"}</p><p className="m-0 text-sm leading-relaxed text-muted">Choose the downloaded Desktop OAuth JSON. The file is checked locally; its contents and secret are never shown here.</p></div>
        <OAuthReplacementLimitSummary limit={limit} />
        <p className="m-0 flex items-start gap-2 text-sm leading-relaxed text-muted"><ShieldCheck aria-hidden="true" className="mt-0.5 shrink-0" size={18} /><span>Your current connection, queued videos and saved upload progress stay protected while you review.</span></p>
      </> : step === "review" && review ? <OAuthReplacementReview limit={limit} review={review} /> : review ? <>
        <div className="grid gap-3 rounded-lg border border-line bg-subtle-surface p-4"><h3 className="m-0 flex items-center gap-2 text-sm font-semibold"><ShieldCheck aria-hidden="true" size={18} />Verify {review.channelName || "the connected channel"}</h3><p className="m-0 text-sm leading-relaxed text-muted">Use this same YouTube channel in Google’s account chooser. A different channel cannot replace this account’s connection.</p></div>
        {waiting ? <p className="m-0 flex items-start gap-2 text-sm leading-relaxed text-muted" role="status"><LoaderCircle aria-hidden="true" className="mt-0.5 shrink-0 animate-spin motion-reduce:animate-none" size={18} /><span>Waiting for Google approval and channel verification…</span></p> : null}
        {notice ? <p className="m-0 text-sm leading-relaxed text-muted" role="status">{notice}</p> : null}
        {waiting && openingFailed ? <button className={`${buttonClass} justify-self-start`} disabled={busy} onClick={() => void reopenGoogle()} type="button"><ArrowRight aria-hidden="true" size={16} />Open Google authorization again</button> : null}
        {waiting ? <button className={`${buttonClass} justify-self-start`} disabled={busy || refreshing} onClick={() => void refresh()} type="button"><RefreshCw aria-hidden="true" size={16} />{refreshing ? "Checking…" : "Refresh authorization"}</button> : null}
        <OAuthReplacementLimitSummary limit={limit} review={review} />
      </> : null}
      {error ? <p className="m-0 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">{error}</p> : null}
      {busy ? <p className="m-0 text-sm leading-relaxed text-muted" role="status">{step === "choose" ? "Checking the selected JSON…" : "Updating this authorization attempt…"}</p> : null}
    </div>
    <footer className="flex flex-wrap items-center gap-2 border-t border-line bg-subtle-surface p-4 sm:px-6">
      {step !== "choose" ? <button className={buttonClass} disabled={busy} onClick={() => void back()} type="button"><ArrowLeft aria-hidden="true" size={16} />{step === "reconnect" ? "Choose another JSON" : "Back"}</button> : null}
      <span aria-hidden="true" className="flex-1" />
      <button className={buttonClass} disabled={busy} onClick={() => void close()} type="button">{waiting ? "Cancel replacement" : "Cancel"}</button>
      {step === "choose" ? <button className={primaryButtonClass} disabled={busy || !channelMatches} onClick={() => void choose()} ref={chooseButton} type="button"><FileJson aria-hidden="true" size={16} />Choose JSON file</button> : step === "review" ? <button className={primaryButtonClass} disabled={busy || !review || !channelMatches} onClick={() => void reconnect()} type="button"><ArrowRight aria-hidden="true" size={16} />Continue to Google</button> : !waiting ? <button className={primaryButtonClass} disabled={busy || !review || status?.state === "cancelled" || !channelMatches} onClick={() => void reconnect()} type="button"><CheckCircle2 aria-hidden="true" size={16} />Reconnect again</button> : null}
    </footer>
  </dialog>;
}
