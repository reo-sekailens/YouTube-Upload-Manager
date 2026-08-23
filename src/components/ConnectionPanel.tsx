import { useEffect, useState } from "react";
import { beginYoutubeConnection, cancelYoutubeConnection, disconnectYoutube, importDesktopOAuthClient, isTauri, loadConnectionSettings } from "../lib/local";
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
      const url = new URL(authorizationUrl);
      if (url.protocol !== "https:") throw new Error("The authorization request must use HTTPS.");
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      void openUrl(url.toString()).catch((error: unknown) => {
        void cancelYoutubeConnection(attemptId).catch(() => undefined);
        setConnecting(false);
        setConnectionAttemptId(undefined);
        setNotice(error instanceof Error ? error.message : "Google authorization could not be opened.");
      });
      setNotice("Google authorization was opened in your browser. Return to this app when consent is complete.");
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
    <section className="connection-panel panel" aria-labelledby="connection-heading">
      <header className="section-heading connection-panel__heading">
        <div>
          <p className="eyebrow">ACCOUNT CONNECTION</p>
          <h2 id="connection-heading">Your YouTube connection</h2>
        </div>
        <span className={`connection-status${settings.connected ? " connection-status--connected" : ""}`}>
          <span aria-hidden="true" />
          {status}
        </span>
      </header>

      <div className="connection-panel__summary">
        {settings.connected && settings.activeChannel ? (
          <p className="connection-panel__channel">
            <span>Active channel</span>
            <strong>{settings.activeChannel}</strong>
          </p>
        ) : settings.connected ? (
          <p className="connection-panel__description">
            Authorization is connected on this device. The channel name appears
            after the account check completes.
          </p>
        ) : (
          <p className="connection-panel__description">
            Connect your Google account when ready. No account is connected
            yet.
          </p>
        )}
      </div>

      <p className="connection-form__help">
        Create your own Google Cloud project, enable the YouTube Data API,
        create a Desktop OAuth client, then import its downloaded JSON here.
        OAuth access and refresh tokens remain in OS-protected storage and
        never reach this screen.
      </p>
      {!settings.connected && (
        <p className="connection-form__help">
          The JSON is parsed locally. Its client secret, when included, is stored only in the operating system credential store.
        </p>
      )}
      {notice && <p className="connection-form__notice" role="status">{notice}</p>}
      {connecting && <p className="connection-form__help">Google is waiting in your browser. You can keep using other parts of the app, or cancel this connection attempt.</p>}
      <footer className="connection-panel__actions">
        {!settings.connected && <button disabled={!isTauri || connecting} onClick={() => void connect()} type="button">
          {connecting ? "Opening Google…" : "Connect YouTube"}
        </button>}
        {!settings.connected && <button className="secondary-button" disabled={!isTauri || connecting} onClick={() => void importDesktopClient()} type="button">Import Desktop OAuth JSON</button>}
        {!settings.connected && connecting && <button className="secondary-button" onClick={() => void cancelConnection()} type="button">Cancel connection</button>}
        {settings.connected && (
          <button className="danger-button" disabled={!isTauri || disconnecting || connecting} onClick={() => void disconnect()} type="button">
            {disconnecting ? "Disconnecting…" : "Disconnect YouTube"}
          </button>
        )}
        {!isTauri && <p>Open the signed desktop app to connect YouTube.</p>}
        {settings.connected && <p>Disconnecting removes this device’s local authorization only. It never deletes a YouTube video.</p>}
      </footer>
    </section>
  );
}
