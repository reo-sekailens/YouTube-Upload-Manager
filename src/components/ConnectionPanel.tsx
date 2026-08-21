import { FormEvent, useEffect, useId, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { beginYoutubeConnection, disconnectYoutube, isTauri, loadConnectionSettings, saveOAuthClientId } from "../lib/local";
import type { ConnectionSettings } from "../lib/types";

const disconnected: ConnectionSettings = { connected: false };

type ConnectionPanelProps = {
  onConnectionChange?: (settings: ConnectionSettings) => void;
};

export function ConnectionPanel({ onConnectionChange }: ConnectionPanelProps) {
  const clientIdId = useId();
  const [settings, setSettings] = useState<ConnectionSettings>(disconnected);
  const [clientId, setClientId] = useState("");
  const [notice, setNotice] = useState("");
  const [saving, setSaving] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);

  useEffect(() => {
    let active = true;
    void loadConnectionSettings()
      .then((loaded) => {
        if (!active) return;
        setSettings(loaded);
        setClientId(loaded.clientId ?? "");
        onConnectionChange?.(loaded);
      })
      .catch(() => {
        if (active) setNotice("Connection settings could not be loaded from this device.");
      });
    return () => { active = false; };
  }, [onConnectionChange]);

  const save = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalizedClientId = clientId.trim();
    if (!normalizedClientId) {
      setNotice("Enter the Google OAuth client ID before saving.");
      return;
    }
    if (!normalizedClientId.endsWith(".apps.googleusercontent.com")) {
      setNotice("Enter a Google installed-app OAuth client ID ending in .apps.googleusercontent.com.");
      return;
    }
    if (!isTauri) {
      setNotice("Browser preview cannot save local connection settings. Open the signed Tauri app.");
      return;
    }

    setSaving(true);
    try {
      const saved = await saveOAuthClientId(normalizedClientId);
      setSettings(saved);
      setClientId(saved.clientId ?? normalizedClientId);
      onConnectionChange?.(saved);
      setNotice("Google OAuth client ID saved on this device. Connect YouTube when authorization is available.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "The local OAuth client setting could not be saved.");
    } finally {
      setSaving(false);
    }
  };

  const connect = async () => {
    if (!isTauri || !settings.clientId) return;
    setConnecting(true);
    try {
      const { authorizationUrl } = await beginYoutubeConnection();
      const url = new URL(authorizationUrl);
      if (url.protocol !== "https:") throw new Error("The authorization request must use HTTPS.");
      await openUrl(url.toString());
      setNotice("Google authorization was opened in your browser. Return to this app when consent is complete.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "YouTube authorization could not be started.");
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
      setClientId(saved.clientId ?? "");
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
            Active channel
            <strong>{settings.activeChannel}</strong>
          </p>
        ) : settings.connected ? (
          <p className="connection-panel__description">
            Authorization is connected on this device. The channel name appears
            after the account check completes.
          </p>
        ) : (
          <p className="connection-panel__description">
            Configure this device before authorizing a YouTube channel. No
            account is connected yet.
          </p>
        )}
      </div>

      <form className="connection-form" onSubmit={(event) => void save(event)}>
        <div className="connection-form__field">
          <label htmlFor={clientIdId}>Google OAuth client ID</label>
          <div className="connection-form__controls">
            <input
              id={clientIdId}
              autoCapitalize="none"
              autoComplete="off"
              inputMode="text"
              onChange={(event) => setClientId(event.target.value)}
              placeholder="1234567890-abc.apps.googleusercontent.com"
              spellCheck={false}
              value={clientId}
            />
            <button className="secondary" disabled={saving} type="submit">
              {saving ? "Saving…" : "Save locally"}
            </button>
          </div>
        </div>
        <p className="connection-form__help">
          This public client ID stays in this app’s local settings. OAuth access
          and refresh tokens remain in OS-protected storage and never reach this
          screen.
        </p>
        {notice && <p className="connection-form__notice" role="status">{notice}</p>}
      </form>
      <footer className="connection-panel__actions">
        {!settings.connected && <button disabled={!isTauri || !settings.clientId || saving || connecting} onClick={() => void connect()} type="button">
          {connecting ? "Opening Google…" : "Connect YouTube"}
        </button>}
        {settings.connected && (
          <button className="danger-button" disabled={!isTauri || disconnecting || connecting} onClick={() => void disconnect()} type="button">
            {disconnecting ? "Disconnecting…" : "Disconnect YouTube"}
          </button>
        )}
        {!settings.clientId && <p>Save a client ID on this device before starting authorization.</p>}
        {settings.connected && <p>Disconnecting removes this device’s local authorization only. It never deletes a YouTube video.</p>}
      </footer>
    </section>
  );
}
