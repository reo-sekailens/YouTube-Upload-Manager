import { useEffect, useId, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, FolderOpen, ShieldCheck, X } from "lucide-react";
import { setRemotePreviewBrowserSession } from "../lib/remote-preview";
import type { RemotePreviewBrowserSession, RemotePreviewConfiguration } from "../lib/remote-preview";
import { comparisonButtonClass } from "./DuplicateComparisonPlayers";

const controlClass = "min-h-10 min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2";
const primaryButtonClass = "inline-flex min-h-10 cursor-pointer items-center justify-center gap-2 rounded-md border border-brand bg-brand px-3 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";
const browsers = [
  { value: "chrome", label: "Chrome" },
  { value: "edge", label: "Edge" },
  { value: "brave", label: "Brave" },
  { value: "chromium", label: "Chromium" },
  { value: "firefox", label: "Firefox" },
] as const;
const browserNames: Record<RemotePreviewBrowserSession["browser"], string> = {
  chrome: "Chrome", edge: "Edge", brave: "Brave", chromium: "Chromium",
  firefox: "Firefox", opera: "Opera", vivaldi: "Vivaldi", safari: "Safari",
};

/** Match the native profile boundary without probing a browser or reading cookies. */
export function browserSessionDraftError(profile: string): string {
  const selected = profile.trim();
  if (!selected) return "Choose a browser profile name or folder before continuing.";
  if (new TextEncoder().encode(selected).length > 1024) return "The profile name or folder is too long.";
  if (/[\u0000-\u001f\u007f]/.test(selected) || selected.includes("::")) return "Use a profile name or folder without control characters or :: options.";
  return "";
}

export function BrowserSessionReview({ session, approved, busy, approvalId, onApprovalChange }: {
  session: RemotePreviewBrowserSession;
  approved: boolean;
  busy: boolean;
  approvalId: string;
  onApprovalChange: (approved: boolean) => void;
}) {
  return <>
    <dl className="mt-4 mb-0 grid gap-3 text-sm sm:grid-cols-2">
      <div><dt className="text-xs font-semibold text-muted">Browser</dt><dd className="mt-1 ml-0 font-semibold">{browserNames[session.browser]}</dd></div>
      <div className="min-w-0"><dt className="text-xs font-semibold text-muted">Profile</dt><dd className="mt-1 ml-0 wrap-anywhere font-semibold">{session.profile}</dd></div>
    </dl>
    <div className="mt-4 rounded-md border border-line bg-surface p-3 sm:p-4">
      <p className="m-0 text-sm font-semibold text-ink">Use the YouTube access already available in this profile</p>
      <p className="mt-2 mb-0 max-w-2xl text-sm leading-relaxed text-muted">yt-dlp reads the selected browser session when resolving a comparison. It may temporarily copy the browser cookie database while reading it. The app remembers this browser and profile choice for this YouTube channel; it does not save exported cookies or reuse the app’s Google login tokens.</p>
      <label className="mt-4 flex cursor-pointer items-start gap-3 text-sm leading-relaxed text-ink" htmlFor={approvalId}>
        <input checked={approved} className="mt-1 h-4 w-4 shrink-0 accent-brand" disabled={busy} id={approvalId} onChange={(event) => onApprovalChange(event.target.checked)} type="checkbox" />
        <span>Allow yt-dlp to read this browser session for comparisons on this channel.</span>
      </label>
    </div>
    <p className="mt-3 mb-0 max-w-2xl text-xs leading-relaxed text-muted">The browser account must be allowed to watch the private video. An expired session or browser protection can still prevent access; you may need to sign in again in that browser or close it and retry. The YouTube embed remains available as fallback.</p>
  </>;
}

export function BrowserSessionSetup({ initialSession, expectedChannelId, onSaved, onClose }: {
  initialSession?: RemotePreviewBrowserSession | null;
  expectedChannelId: string;
  onSaved: (configuration: RemotePreviewConfiguration) => void;
  onClose: () => void;
}) {
  const [step, setStep] = useState(1);
  const [browser, setBrowser] = useState<RemotePreviewBrowserSession["browser"]>(initialSession?.browser ?? "chrome");
  const [profile, setProfile] = useState(initialSession?.profile ?? "Default");
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const active = useRef(true);
  const pending = useRef(false);
  const heading = useRef<HTMLHeadingElement>(null);
  const approvalId = useId();
  const profileHelpId = useId();
  const draftError = browserSessionDraftError(profile);
  const session = { browser, profile: profile.trim() };
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  useEffect(() => { heading.current?.focus(); }, [step]);
  const chooseFolder = async () => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError("");
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ title: "Choose the signed-in browser profile folder", multiple: false, directory: true });
      if (active.current && typeof path === "string") setProfile(path);
    } catch { if (active.current) setError("The profile folder could not be selected. Enter its name or folder, or try again."); }
    finally { pending.current = false; if (active.current) setBusy(false); }
  };
  const save = async (disable = false) => {
    if (pending.current || (!disable && (draftError || !approved))) return;
    pending.current = true; setBusy(true); setError("");
    try {
      const configuration = await setRemotePreviewBrowserSession(disable ? null : session, expectedChannelId, !disable && approved);
      if (active.current) onSaved(configuration);
    } catch { if (active.current) setError(disable ? "The browser session could not be disabled. Try again." : "The browser session choice could not be saved. Check the profile and retry. The YouTube embed remains available."); }
    finally { pending.current = false; if (active.current) setBusy(false); }
  };
  const extraBrowser = !browsers.some((option) => option.value === browser);
  return <section aria-label="Set up browser session for comparisons" className="mb-5 rounded-lg border border-line bg-subtle-surface p-4 sm:p-5">
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0"><p className="mt-0 mb-2 text-xs font-bold tracking-wide text-muted uppercase">Step {step} of 2 · Browser session</p><h3 className="m-0 text-base font-semibold text-ink outline-none" ref={heading} tabIndex={-1}>{step === 1 ? "Choose your signed-in browser profile" : "Review browser session access"}</h3></div>
      <button aria-label="Cancel browser session setup" className={`${comparisonButtonClass} shrink-0 p-2`} disabled={busy} onClick={onClose} type="button"><X aria-hidden="true" size={18} /></button>
    </div>
    {step === 1 ? <>
      <p className="mt-2 mb-4 max-w-2xl text-sm leading-relaxed text-muted">Use a browser profile where you are already signed in to YouTube. Once enabled, yt-dlp reuses its current session for comparisons on this channel, including private videos that account can watch.</p>
      {initialSession && <p className="mt-0 mb-4 flex items-start gap-2 rounded-md border border-line bg-surface p-3 text-sm leading-relaxed text-ink" role="status"><ShieldCheck aria-hidden="true" className="mt-0.5 shrink-0" size={16} /><span>Enabled for this channel: {browserNames[initialSession.browser]} · <span className="wrap-anywhere">{initialSession.profile}</span></span></p>}
      <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]">
        <label className="grid content-start gap-1.5 text-sm font-semibold text-ink">Browser
          <select className={controlClass} disabled={busy} onChange={(event) => { const next = event.target.value as RemotePreviewBrowserSession["browser"]; setBrowser(next); setProfile(next === "firefox" ? "" : "Default"); setApproved(false); }} value={browser}>
            {browsers.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
            {extraBrowser && <option value={browser}>{browserNames[browser]}</option>}
          </select>
        </label>
        <div className="min-w-0"><label className="grid gap-1.5 text-sm font-semibold text-ink">Browser profile name or folder
          <input aria-describedby={profileHelpId} autoComplete="off" className={`${controlClass} w-full font-normal`} disabled={busy} maxLength={1024} onChange={(event) => { setProfile(event.target.value); setApproved(false); }} placeholder={browser === "firefox" ? "e.g. abc123.default-release" : "Default"} spellCheck={false} type="text" value={profile} />
        </label><p className="mt-2 mb-0 text-xs leading-relaxed text-muted" id={profileHelpId}>{browser === "firefox" ? "Firefox requires the exact profile name or its profile folder. Find it in Firefox’s about:profiles page." : "Use Default, a profile name such as Profile 1, or select its profile folder. The browser profile page shows its location."}</p><button className={`${comparisonButtonClass} mt-3`} disabled={busy} onClick={() => void chooseFolder()} type="button"><FolderOpen aria-hidden="true" size={16} />Choose profile folder</button></div>
      </div>
      {profile.trim() && draftError && <p className="mt-3 mb-0 text-sm text-warning-ink" role="status">{draftError}</p>}
    </> : <BrowserSessionReview approvalId={approvalId} approved={approved} busy={busy} onApprovalChange={setApproved} session={session} />}
    {error && <p className="mt-4 mb-0 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">{error}</p>}
    <div className="mt-5 flex flex-wrap items-center justify-between gap-3">
      <div className="flex flex-wrap items-center gap-2"><button className={comparisonButtonClass} disabled={busy} onClick={step === 1 ? onClose : () => { setStep(1); setApproved(false); }} type="button">{step === 2 && <ArrowLeft aria-hidden="true" size={16} />}{step === 1 ? "Cancel" : "Back"}</button>{initialSession && <button className={comparisonButtonClass} disabled={busy} onClick={() => void save(true)} type="button">Disable browser session</button>}</div>
      {step === 1 ? <button className={primaryButtonClass} disabled={busy || Boolean(draftError)} onClick={() => { setApproved(false); setStep(2); }} type="button">Review access<ArrowRight aria-hidden="true" size={16} /></button> : <button className={primaryButtonClass} disabled={busy || !approved || Boolean(draftError)} onClick={() => void save()} type="button">{busy ? "Saving…" : "Enable and retry comparison"}</button>}
    </div>
  </section>;
}
