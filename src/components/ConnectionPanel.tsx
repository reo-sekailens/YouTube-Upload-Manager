import { useEffect, useState } from "react";
import { beginYoutubeConnection, cancelYoutubeConnection, disconnectYoutube, importDesktopOAuthClient, isTauri, loadConnectionSettings } from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import { useRetainedWorkspaceState } from "../lib/retained-workspace-state";
import { subscribeLocalStateChanges } from "../lib/state-events";
import type { ConnectionSettings } from "../lib/types";

const disconnected: ConnectionSettings = { connected: false };

type ConnectionPanelProps = {
  onConnectionChange?: (settings: ConnectionSettings) => void;
};

export function ConnectionPanel({ onConnectionChange }: ConnectionPanelProps) {
  const [settings, setSettings] = useState<ConnectionSettings>(disconnected);
  const [notice, setNotice] = useRetainedWorkspaceState("account.notice", "");
  const [connecting, setConnecting] = useRetainedWorkspaceState(
    "account.connecting",
    false,
  );
  const [connectionAttemptId, setConnectionAttemptId] =
    useRetainedWorkspaceState<string | undefined>(
      "account.connection-attempt",
      undefined,
    );
  const [disconnecting, setDisconnecting] = useState(false);

  useEffect(() => {
    let active = true;
    void loadConnectionSettings()
      .then((loaded) => {
        if (!active) return;
        setSettings(loaded);
        onConnectionChange?.(loaded);
      })
      .catch(() => {
        if (active) setNotice("Connection settings could not be loaded from this device.");
      });
    return () => { active = false; };
  }, [onConnectionChange]);

  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;
    if (!isTauri) return;
    void subscribeLocalStateChanges((batch) => {
      if (!batch.changes.some((change) => change.surface === "connection"))
        return;
      void loadConnectionSettings()
        .then((loaded) => {
          if (!active) return;
          setSettings(loaded);
          onConnectionChange?.(loaded);
          if (loaded.detail !== "Waiting for Google authorization in your browser.") {
            setNotice(loaded.detail ?? "Google authorization finished.");
            setConnecting(false);
            setConnectionAttemptId(undefined);
          }
        })
        .catch(() => undefined);
    })
      .then((stop) => {
        if (active) unsubscribe = stop;
        else stop();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [onConnectionChange, setConnecting, setConnectionAttemptId, setNotice]);

  useEffect(() => {
    if (!connecting || !isTauri) return;
    let active = true;
    let timeout: number | undefined;
    const delays = [250, 500, 1_000, 2_000, 4_000, 8_000, 16_000];
    const check = async (attempt: number) => {
      try {
        const loaded = await loadConnectionSettings();
        if (!active) return;
        if (loaded.detail !== "Waiting for Google authorization in your browser.") {
          setSettings(loaded);
          onConnectionChange?.(loaded);
          setNotice(loaded.detail ?? "Google authorization finished.");
          setConnecting(false);
          setConnectionAttemptId(undefined);
          return;
        }
      } catch {
        // Event delivery remains primary; this fallback is intentionally bounded.
      }
      if (active && attempt + 1 < delays.length)
        timeout = window.setTimeout(
          () => void check(attempt + 1),
          delays[attempt + 1],
        );
    };
    timeout = window.setTimeout(() => void check(0), delays[0]);
    return () => {
      active = false;
      if (timeout !== undefined) window.clearTimeout(timeout);
    };
  }, [
    connecting,
    onConnectionChange,
    setConnecting,
    setConnectionAttemptId,
    setNotice,
  ]);

  const connect = async () => {
    if (!isTauri || !settings.oauthConfigured) return;
    setConnecting(true);
    try {
      const { authorizationUrl, attemptId } = await beginYoutubeConnection();
      setConnectionAttemptId(attemptId);
      void openAndCopyGoogleAuthorization(authorizationUrl).then((copied) => {
        setNotice(copied
          ? "Google authorization was opened and its link was copied. Return to this app when consent is complete."
          : "Google authorization was opened, but clipboard access was unavailable.");
      }).catch((error: unknown) => {
        void cancelYoutubeConnection(attemptId).catch(() => undefined);
        setConnecting(false);
        setConnectionAttemptId(undefined);
        setNotice(error instanceof Error ? error.message : "Google authorization could not be opened.");
      });
    } catch (error) {
      setConnecting(false);
      setConnectionAttemptId(undefined);
      setNotice(error instanceof Error ? error.message : "YouTube authorization could not be started.");
    }
  };

  const cancelConnection = async () => {
    if (!isTauri || !connectionAttemptId) return;
    try {
      const saved = await cancelYoutubeConnection(connectionAttemptId);
      setSettings(saved);
      onConnectionChange?.(saved);
      setNotice("Google connection cancelled. You can continue using the app without a connected channel.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "The pending Google connection could not be cancelled.");
    } finally {
      setConnecting(false);
      setConnectionAttemptId(undefined);
    }
  };

  const importDesktopClient = async () => {
    if (!isTauri) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Google Desktop OAuth JSON", extensions: ["json"] }],
    });
    if (typeof selected !== "string") return;
    setConnecting(true);
    try {
      const saved = await importDesktopOAuthClient(selected);
      setSettings(saved);
      onConnectionChange?.(saved);
      setNotice("Desktop OAuth JSON imported. Its client secret is held in OS-protected storage; connect YouTube when ready.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "The Desktop OAuth JSON could not be imported.");
    } finally {
      setConnecting(false);
    }
  };

  const disconnect = async () => {
    if (!isTauri || !settings.connected) return;
    setDisconnecting(true);
    try {
      const saved = await disconnectYoutube();
      setSettings(saved);
      onConnectionChange?.(saved);
      setNotice("YouTube was disconnected on this device. Locally protected authorization credentials were removed; no YouTube videos changed.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "YouTube could not be disconnected from this device.");
    } finally {
      setDisconnecting(false);
    }
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
      {notice && <p className="mt-2 text-[0.73rem] leading-[1.5] text-[#315389]" role="status">{notice}</p>}
      {connecting && <p className="mt-2 text-[0.73rem] leading-[1.5] text-[#7b8799]">Google is waiting in your browser. You can keep using other parts of the app, or cancel this connection attempt.</p>}
      <footer className="mt-4 flex flex-wrap items-center gap-3 border-t border-[#e7eaf0] pt-4 max-compact:flex-col max-compact:items-stretch max-compact:[&_button]:w-full">
        {!settings.connected && <button className="cursor-pointer rounded-md border border-brand bg-brand px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-white transition-[background,border-color,box-shadow] duration-150 hover:border-brand-strong hover:bg-brand-strong disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || connecting} onClick={() => void connect()} type="button">
          {connecting ? "Opening Google…" : "Connect YouTube"}
        </button>}
        {!settings.connected && <button className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || connecting} onClick={() => void importDesktopClient()} type="button">Import Desktop OAuth JSON</button>}
        {!settings.connected && connecting && <button className="cursor-pointer rounded-md border border-[#cdd4df] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-[#34405a] transition-[background,border-color,box-shadow] duration-150 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-50" onClick={() => void cancelConnection()} type="button">Cancel connection</button>}
        {settings.connected && (
          <button className="cursor-pointer rounded-md border border-[#e8c7c5] bg-white px-3 py-2.5 text-[0.79rem] leading-[1.2] font-semibold text-danger transition-[background,border-color,box-shadow] duration-150 hover:border-[#d59d99] hover:bg-[#fff5f4] disabled:cursor-not-allowed disabled:opacity-50" disabled={!isTauri || disconnecting || connecting} onClick={() => void disconnect()} type="button">
            {disconnecting ? "Disconnecting…" : "Disconnect YouTube"}
          </button>
        )}
        {!isTauri && <p className="m-0 text-[0.72rem] text-[#7b8799]">Open the signed desktop app to connect YouTube.</p>}
        {settings.connected && <p className="m-0 text-[0.72rem] text-[#7b8799]">Disconnecting removes this device’s local authorization only. It never deletes a YouTube video.</p>}
      </footer>
    </section>
  );
}
