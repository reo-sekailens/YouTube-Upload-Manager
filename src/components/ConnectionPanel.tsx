import { useCallback, useEffect, useRef, useState } from "react";
import { beginYoutubeConnection, cancelYoutubeConnection, disconnectYoutube, importDesktopOAuthClient, isTauri, loadConnectionSettings } from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import { useRetainedWorkspaceState } from "../lib/retained-workspace-state";
import { subscribeLocalStateChanges } from "../lib/state-events";
import { useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import type { ConnectionSettings } from "../lib/types";

const disconnected: ConnectionSettings = { connected: false };
const waitingForGoogle = "Waiting for Google authorization in your browser.";
type ConnectionPanelProps = { onConnectionChange?: (settings: ConnectionSettings) => void };

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
      if (!current() || mutation.current) return;
      acceptSettings(loaded);
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
      setNotice("YouTube was disconnected on this device. Locally protected authorization credentials were removed; no YouTube videos changed.");
    } catch (cause) {
      setError(workspaceErrorMessage(cause, "YouTube could not be disconnected from this device."));
    } finally { mutation.current = false; setDisconnecting(false); }
  };

  const status = settings.connected
    ? settings.activeChannel ? `Connected to ${settings.activeChannel}` : "YouTube is connected"
    : "YouTube is not connected";

  return (
    <section className="m-0 w-full rounded-xl border border-line bg-surface p-5" aria-labelledby="connection-heading">
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
          <p className="mb-4 flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg border border-[#dcece3] bg-[#f8fbf9] px-3 py-2.5 text-[0.83rem] leading-[1.5] text-[#5f6c80] [&>span]:text-[0.72rem] [&>span]:font-semibold [&>span]:uppercase [&>span]:tracking-[0.04em] [&>span]:text-muted [&>strong]:overflow-wrap-anywhere [&>strong]:text-[0.87rem] [&>strong]:text-[#1d5e40]">
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
      <footer className="mt-4 flex flex-wrap items-center gap-3 border-t border-[#e7eaf0] pt-4 max-compact:flex-col max-compact:items-stretch max-compact:[&_button]:w-full">
        {!settings.connected && <button className="cursor-pointer rounded-md border border-brand bg-brand px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-white transition-[background,border-color,box-shadow] duration-150 hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || loading || !settings.oauthConfigured || connecting || importing || disconnecting} onClick={() => void connect()} type="button">
          {connecting ? connectionAttemptId ? "Waiting for Google…" : "Opening Google…" : "Connect YouTube"}
        </button>}
        {!settings.connected && <button className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || loading || connecting || importing || disconnecting} onClick={() => void importDesktopClient()} type="button">Import Desktop OAuth JSON</button>}
        {!settings.connected && connecting && <button className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" disabled={!connectionAttemptId || cancelling} onClick={() => void cancelConnection()} type="button">{cancelling ? "Cancelling…" : "Cancel connection"}</button>}
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
  );
}
import "./ConnectionPanel.lazy.css";
