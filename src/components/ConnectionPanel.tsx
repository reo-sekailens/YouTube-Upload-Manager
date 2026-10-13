import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { beginYoutubeConnection, cancelYoutubeConnection, disconnectYoutube, importDesktopOAuthClient, isTauri, loadConnectionSettings, loadUploadLimitStatus } from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import { useRetainedWorkspaceState } from "../lib/retained-workspace-state";
import { subscribeLocalStateChanges } from "../lib/state-events";
import { useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import type { ConnectionSettings, UploadLimitStatus } from "../lib/types";

const OAuthClientReplaceDialog = lazy(() => import("./OAuthClientReplaceDialog").then((module) => ({ default: module.OAuthClientReplaceDialog })));
const ChannelProfilesDialog = lazy(() => import("./ChannelProfilesDialog").then((module) => ({ default: module.ChannelProfilesDialog })));

function limitResumeLabel(value?: string): string {
  if (!value) return "the saved retry time";
  const date = new Date(value);
  return Number.isFinite(date.valueOf()) ? date.toLocaleString() : "the saved retry time";
}

const disconnected: ConnectionSettings = { connected: false };
const waitingForGoogle = "Waiting for Google authorization in your browser.";
type ConnectionPanelProps = { onConnectionChange?: (settings: ConnectionSettings) => void };
type ProfileDialogSession = { channelId?: string; channelName?: string };

export function profileDialogScope(settings: ConnectionSettings): ProfileDialogSession {
  return { channelId: settings.activeChannelId ?? undefined, channelName: settings.activeChannel ?? undefined };
}

/** A reviewed native switch may intentionally change channel; the dialog session is its UI fence. */
export function deliverReviewedProfileSettings(saved: ConnectionSettings, current: () => boolean, accept: (settings: ConnectionSettings) => void) {
  if (!current()) return false;
  accept(saved);
  return true;
}

export function ConnectionPanel({ onConnectionChange }: ConnectionPanelProps) {
  const [settings, setSettings] = useState<ConnectionSettings>(disconnected);
  const [notice, setNotice] = useRetainedWorkspaceState("account.notice", "");
  const [connecting, setConnecting] = useRetainedWorkspaceState("account.connecting", false);
  const [connectionAttemptId, setConnectionAttemptId] = useRetainedWorkspaceState<string | undefined>("account.connection-attempt", undefined);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [importing, setImporting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [limit, setLimit] = useState<UploadLimitStatus>({ kind: "none" });
  const [replacementChannelId, setReplacementChannelId] = useState<string>();
  const [profiles, setProfiles] = useState<ProfileDialogSession>();
  const profileSession = useRef<ProfileDialogSession | undefined>(undefined);
  const panel = useRef<HTMLElement>(null);
  const replacementTrigger = useRef<HTMLButtonElement>(null);
  const profilesTrigger = useRef<HTMLButtonElement>(null);
  const wasReplacing = useRef(false);
  const wasManagingProfiles = useRef(false);
  const mutation = useRef(false);
  const attempt = useRef(connectionAttemptId);
  attempt.current = connectionAttemptId;
  const readFence = useWorkspaceRequestFence("device-connection");

  const acceptSettings = useCallback((loaded: ConnectionSettings) => {
    setSettings(loaded);
    setLoading(false);
    onConnectionChange?.(loaded);
  }, [onConnectionChange]);

  const refresh = useCallback(async (finishAttempt = false) => {
    if (mutation.current) return;
    const current = readFence.begin();
    try {
      const loaded = await loadConnectionSettings();
      let loadedLimit: UploadLimitStatus = { kind: "none" };
      let limitError = "";
      try { loadedLimit = await loadUploadLimitStatus(loaded.activeChannelId); }
      catch (cause) { limitError = workspaceErrorMessage(cause, "Upload limits could not be checked. Refresh connection to try again."); }
      if (!current() || mutation.current) return;
      acceptSettings(loaded);
      setLimit(loadedLimit);
      if (limitError) setError(limitError);
      setLoading(false);
      if (finishAttempt && attempt.current && loaded.detail !== waitingForGoogle) {
        setNotice(loaded.detail ?? "Google authorization finished.");
        setConnecting(false);
        attempt.current = undefined;
        setConnectionAttemptId(undefined);
      }
    } catch (cause) {
      if (current()) {
        setLoading(false);
        setError(workspaceErrorMessage(cause, "Connection settings could not be loaded from this device."));
      }
    }
  }, [acceptSettings, readFence, setConnecting, setConnectionAttemptId, setNotice]);

  useEffect(() => { void refresh(true); }, [refresh]);

  useEffect(() => {
    if (panel.current) panel.current.inert = replacementChannelId !== undefined || profiles !== undefined;
    if (replacementChannelId === undefined && wasReplacing.current) replacementTrigger.current?.focus();
    if (profiles === undefined && wasManagingProfiles.current) profilesTrigger.current?.focus();
    wasReplacing.current = replacementChannelId !== undefined;
    wasManagingProfiles.current = profiles !== undefined;
    return () => { if (panel.current) panel.current.inert = false; };
  }, [profiles, replacementChannelId]);

  useEffect(() => {
    if (profileSession.current && profileSession.current.channelId !== (settings.activeChannelId ?? undefined)) {
      profileSession.current = undefined;
      setProfiles(undefined);
    }
  }, [settings.activeChannelId]);

  useEffect(() => () => { profileSession.current = undefined; }, []);

  useEffect(() => {
    if (!isTauri) return;
    let active = true;
    let unsubscribe: (() => void) | undefined;
    void subscribeLocalStateChanges((batch) => {
      if (active && batch.changes.some((change) => change.surface === "connection")) void refresh(true);
    }).then((stop) => { if (active) unsubscribe = stop; else stop(); }).catch((cause) => {
      if (active) setError(workspaceErrorMessage(cause, "Connection updates could not be subscribed. Use Refresh connection to check the current status."));
    });
    return () => { active = false; unsubscribe?.(); };
  }, [refresh]);

  useEffect(() => {
    if (!connecting || !connectionAttemptId || !isTauri) return;
    let active = true;
    let timeout: number | undefined;
    const delays = [250, 500, 1_000, 2_000, 4_000, 8_000, 16_000];
    const check = async (index: number) => {
      if (!active || attempt.current !== connectionAttemptId) return;
      await refresh(true);
      if (active && attempt.current === connectionAttemptId && index + 1 < delays.length)
        timeout = window.setTimeout(() => void check(index + 1), delays[index + 1]);
    };
    timeout = window.setTimeout(() => void check(0), delays[0]);
    return () => { active = false; if (timeout !== undefined) window.clearTimeout(timeout); };
  }, [connecting, connectionAttemptId, refresh]);

  const connect = async () => {
    if (!isTauri || loading || !settings.oauthConfigured || connecting || mutation.current) return;
    mutation.current = true; readFence.invalidate(); setError(""); setConnecting(true);
    try {
      const { authorizationUrl, attemptId } = await beginYoutubeConnection();
      attempt.current = attemptId;
      setConnectionAttemptId(attemptId);
      mutation.current = false;
      try {
        const copied = await openAndCopyGoogleAuthorization(authorizationUrl);
        if (attempt.current === attemptId) setNotice(copied
          ? "Google authorization was opened and its link was copied. Return to this app when consent is complete."
          : "Google authorization was opened, but clipboard access was unavailable.");
      } catch (cause) {
        if (attempt.current !== attemptId) return;
        setError(workspaceErrorMessage(cause, "Google authorization could not be opened."));
        try {
          const saved = await cancelYoutubeConnection(attemptId);
          readFence.invalidate(); acceptSettings(saved);
          setConnecting(false); attempt.current = undefined; setConnectionAttemptId(undefined);
        } catch (cancelError) {
          setError(workspaceErrorMessage(cancelError, "Google could not open and the pending connection could not be cancelled. Use Cancel connection to try again."));
        }
      }
    } catch (cause) {
      setConnecting(false); attempt.current = undefined; setConnectionAttemptId(undefined);
      setError(workspaceErrorMessage(cause, "YouTube authorization could not be started."));
    } finally { mutation.current = false; }
  };

  const cancelConnection = async () => {
    if (!isTauri || !attempt.current || mutation.current) return;
    const attemptId = attempt.current;
    mutation.current = true; readFence.invalidate(); setCancelling(true); setError("");
    try {
      const saved = await cancelYoutubeConnection(attemptId);
      acceptSettings(saved);
      setNotice("Google connection cancelled. You can continue using the app without a connected channel.");
      setConnecting(false); attempt.current = undefined; setConnectionAttemptId(undefined);
    } catch (cause) {
      setError(workspaceErrorMessage(cause, "The pending Google connection could not be cancelled. Try cancelling again."));
    } finally { mutation.current = false; setCancelling(false); }
  };

  const importDesktopClient = async () => {
    if (!isTauri || loading || connecting || mutation.current) return;
    mutation.current = true; readFence.invalidate(); setImporting(true); setError("");
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, directory: false, filters: [{ name: "Google Desktop OAuth JSON", extensions: ["json"] }] });
      if (typeof selected !== "string") return;
      const saved = await importDesktopOAuthClient(selected);
      acceptSettings(saved);
      setNotice("Desktop OAuth JSON imported. Its client secret is held in OS-protected storage; connect YouTube when ready.");
    } catch (cause) {
      setError(workspaceErrorMessage(cause, "The Desktop OAuth JSON could not be imported."));
    } finally { mutation.current = false; setImporting(false); }
  };

  const disconnect = async () => {
    if (!isTauri || !settings.connected || connecting || mutation.current) return;
    mutation.current = true; readFence.invalidate(); setDisconnecting(true); setError("");
    try {
      const saved = await disconnectYoutube();
      acceptSettings(saved);
      setNotice("YouTube disconnected. Your saved channel profile and uploads remain on this device.");
    } catch (cause) {
      setError(workspaceErrorMessage(cause, "YouTube could not be disconnected from this device."));
    } finally { mutation.current = false; setDisconnecting(false); }
  };

  const status = settings.connected
    ? settings.activeChannel ? `Connected to ${settings.activeChannel}` : "YouTube is connected"
    : "YouTube is not connected";

  const closeProfiles = (session: ProfileDialogSession) => {
    if (profileSession.current !== session) return;
    profileSession.current = undefined;
    setProfiles(undefined);
    void refresh();
  };
  const profileChanged = (session: ProfileDialogSession, saved?: ConnectionSettings) => {
    if (profileSession.current !== session) return;
    readFence.invalidate();
    if (saved) {
      deliverReviewedProfileSettings(saved, () => profileSession.current === session, acceptSettings);
      setLimit({ kind: "none" });
    }
    profileSession.current = undefined;
    setProfiles(undefined);
    setError("");
    setNotice(saved ? saved.detail || (saved.connected
      ? `Selected channel: ${saved.activeChannel || "the reviewed channel"}. Existing uploads and recovery evidence retain their original channel.`
      : "Saved channel profile removed. Its queue and recovery evidence remain on this device.")
      : "Verified channel profile saved. Select it through a separate switch review when ready.");
    void refresh();
  };

  return (
    <>
    <section ref={panel} className="m-0 w-full rounded-xl border border-line bg-surface p-5" aria-labelledby="connection-heading">
      <header className="mb-3 flex items-start justify-between gap-4 border-b border-[#e7eaf0] pb-4 max-compact:flex-col max-compact:items-stretch">
        <div>
          <p className="mb-2 text-[0.67rem] font-bold uppercase tracking-[0.1em] leading-[1.2] text-muted">ACCOUNT CONNECTION</p>
          <h2 id="connection-heading" className="m-0 text-[1.2rem] font-bold tracking-[-0.035em] text-ink">Your YouTube connection</h2>
        </div>
        <span className={settings.connected ? "inline-flex items-center gap-1.5 text-right text-[0.73rem] font-semibold text-[#18724c]" : "inline-flex items-center gap-1.5 text-right text-[0.73rem] font-semibold text-[#876a20]"}>
          <span aria-hidden="true" className="size-1.5 rounded-full bg-current" />
          {status}
        </span>
      </header>

      <div className="pt-0.5">
        {settings.connected && settings.activeChannel ? (
          <p className="mb-4 flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg border border-[#dcece3] bg-[#f8fbf9] px-3 py-2.5 text-[0.83rem] leading-[1.5] text-[#5f6c80] [&>span]:text-[0.72rem] [&>span]:font-semibold [&>span]:uppercase [&>span]:tracking-[0.04em] [&>span]:text-muted [&>strong]:overflow-wrap-anywhere [&>strong]:text-[0.87rem] [&>strong]:text-success">
            <span>Active channel</span>
            <strong>{settings.activeChannel}</strong>
          </p>
        ) : settings.connected ? (
          <p className="mb-4 text-[0.83rem] leading-[1.5] text-[#5f6c80]">
            Authorization is connected on this device. The channel name appears
            after the account check completes.
          </p>
        ) : (
          <p className="mb-4 text-[0.83rem] leading-[1.5] text-[#5f6c80]">
            Connect your Google account when ready. No account is connected
            yet.
          </p>
        )}
      </div>

      <p className="mt-2 text-[0.73rem] leading-[1.5] text-[#7b8799]">
        Create your own Google Cloud project, enable the YouTube Data API,
        create a Desktop OAuth client, then import its downloaded JSON here.
        OAuth access and refresh tokens remain in OS-protected storage and
        never reach this screen.
      </p>
      {!settings.connected && (
        <p className="mt-2 text-[0.73rem] leading-[1.5] text-[#7b8799]">
          The JSON is parsed locally. Its client secret, when included, is stored only in the operating system credential store.
        </p>
      )}
      {error && <p className="mt-2 text-[0.73rem] font-semibold leading-[1.5] text-danger" role="alert">{error}</p>}
      {loading && <p className="ui-subtle" role="status">Loading this device’s connection…</p>}
      {importing && <p className="ui-subtle" role="status">Choose a Desktop OAuth JSON file to import…</p>}
      {notice && <p className="mt-2 text-[0.73rem] leading-[1.5] text-[#315389]" role="status">{notice}</p>}
      {connecting && connectionAttemptId && <p className="mt-2 text-[0.73rem] leading-[1.5] text-[#7b8799]">Google is waiting in your browser. You can keep using other parts of the app, or cancel this connection attempt.</p>}
      {settings.connected && limit.kind !== "none" && (
        <div className="mt-4 rounded-lg border border-line bg-subtle-surface p-4" role="status">
          <p className="m-0 text-sm font-semibold text-ink">{limit.kind === "api_quota" ? "Google Cloud API quota reached" : "YouTube channel upload limit reached"}</p>
          <p className="mt-2 mb-0 text-sm leading-relaxed text-muted">{limit.kind === "api_quota"
            ? `Your videos remain queued. The current Cloud project is paused until ${limitResumeLabel(limit.resumesAt)}. You can review a replacement OAuth JSON below; YouTube still decides whether requests are allowed.`
            : `Your videos remain queued until ${limitResumeLabel(limit.resumesAt)}. This limit belongs to the channel; replacing the OAuth JSON does not reset it.`}</p>
          {limit.kind === "api_quota" && limit.channelResumesAt && <p className="mt-2 mb-0 text-sm leading-relaxed text-muted">The channel also has an upload pause until {limitResumeLabel(limit.channelResumesAt)}. That pause remains after replacing the JSON.</p>}
        </div>
      )}
      <footer className="mt-4 flex flex-wrap items-center gap-3 border-t border-[#e7eaf0] pt-4 max-compact:flex-col max-compact:items-stretch max-compact:[&_button]:w-full">
        <button ref={profilesTrigger} className="cursor-pointer rounded-md border border-line bg-surface px-3 py-2.5 text-[0.79rem] font-semibold text-ink hover:bg-control-hover focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || loading || importing || disconnecting || connecting || cancelling || replacementChannelId !== undefined || profiles !== undefined} onClick={() => {
          const session = profileDialogScope(settings);
          profileSession.current = session;
          setProfiles(session);
        }} type="button">Saved channel profiles</button>
        {!settings.connected && <button className="cursor-pointer rounded-md border border-brand bg-brand px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-white transition-[background,border-color,box-shadow] duration-150 hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || loading || !settings.oauthConfigured || connecting || importing || disconnecting} onClick={() => void connect()} type="button">
          {connecting ? connectionAttemptId ? "Waiting for Google…" : "Opening Google…" : "Connect YouTube"}
        </button>}
        {!settings.connected && <button className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || loading || connecting || importing || disconnecting} onClick={() => void importDesktopClient()} type="button">Import Desktop OAuth JSON</button>}
        {!settings.connected && connecting && <button className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!connectionAttemptId || cancelling} onClick={() => void cancelConnection()} type="button">{cancelling ? "Cancelling…" : "Cancel connection"}</button>}
        {settings.connected && (
          <button ref={replacementTrigger} className="cursor-pointer rounded-md border border-line bg-surface px-3 py-2.5 text-[0.79rem] font-semibold text-ink hover:bg-control-hover focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || loading || !settings.activeChannelId || disconnecting || connecting || importing} onClick={() => setReplacementChannelId(settings.activeChannelId)} type="button">Replace OAuth JSON</button>
        )}
        {settings.connected && (
          <button className="cursor-pointer rounded-md border border-[#e8c7c5] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-danger transition-[background,border-color,box-shadow] duration-150 hover:border-[#d59d99] hover:bg-[#fff5f4] focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || loading || disconnecting || connecting || importing} onClick={() => void disconnect()} type="button">
            {disconnecting ? "Disconnecting…" : "Disconnect YouTube"}
          </button>
        )}
        <button className="cursor-pointer rounded-md border border-line bg-surface px-3 py-2.5 text-[0.79rem] font-semibold text-ink focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={loading || importing || disconnecting || cancelling || (connecting && !connectionAttemptId)} onClick={() => { setError(""); void refresh(true); }} type="button">Refresh connection</button>
        {!settings.connected && !settings.oauthConfigured && !loading && <p className="m-0 text-[0.72rem] text-muted">Import your Desktop OAuth JSON to enable Connect YouTube.</p>}
        {!isTauri && <p className="m-0 text-[0.72rem] text-[#7b8799]">Open the signed desktop app to connect YouTube.</p>}
        {settings.connected && <p className="m-0 text-[0.72rem] text-[#7b8799]">Disconnecting removes this device’s local authorization only. It never deletes a YouTube video.</p>}
      </footer>
    </section>
    {replacementChannelId && <Suspense fallback={<p role="status">Opening OAuth JSON replacement…</p>}><OAuthClientReplaceDialog expectedChannelId={replacementChannelId} channelName={settings.activeChannel} limit={limit} onReplaced={(saved) => {
      acceptSettings(saved);
      setNotice("OAuth JSON replaced after Google verified the same channel. Your queued videos and upload checkpoints are retained.");
      setReplacementChannelId(undefined);
      void refresh();
    }} onClose={() => { setReplacementChannelId(undefined); void refresh(); }} /></Suspense>}
    {profiles && <Suspense fallback={<p role="status">Opening saved channel profiles…</p>}><ChannelProfilesDialog expectedChannelId={profiles.channelId} activeChannel={profiles.channelName} onChanged={(saved) => profileChanged(profiles, saved)} onClose={() => closeProfiles(profiles)} /></Suspense>}
    </>
  );
}
import "./ConnectionPanel.lazy.css";
export default ConnectionPanel;
