import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { importAsset, isTauri, loadSnapshot, queueItem, reconcileQueue, startQueuedUploads, syncChannelInventory } from "./lib/local";
import type { DashboardSnapshot, UploadItem } from "./lib/types";
import type { ConnectionSettings } from "./lib/types";
import { ConnectionPanel } from "./components/ConnectionPanel";
import { DeletionReview } from "./components/DeletionReview";
import { DuplicateReview } from "./components/DuplicateReview";
import { QueueTable } from "./components/QueueTable";

const emptySnapshot: DashboardSnapshot = { items: [], duplicates: [] };

export default function App() {
  const [snapshot, setSnapshot] = useState<DashboardSnapshot>(emptySnapshot);
  const [notice, setNotice] = useState("Local queue is ready. No media leaves this device except during a YouTube upload.");
  const [busy, setBusy] = useState(false);

  const updateConnection = useCallback((settings: ConnectionSettings) => {
    setSnapshot((current) => ({ ...current, activeChannel: settings.activeChannel }));
  }, []);

  const refresh = async () => setSnapshot(await loadSnapshot());

  useEffect(() => {
    void refresh();
  }, []);

  const addVideo = async () => {
    if (!isTauri) {
      setNotice("Run this screen through Tauri to import a file into the managed local workspace.");
      return;
    }
    const selected = await open({ multiple: false, directory: false, filters: [{ name: "Video", extensions: ["mp4", "mov", "mkv", "webm", "avi"] }] });
    if (typeof selected !== "string") return;
    setBusy(true);
    try {
      const item = await importAsset(selected);
      setSnapshot((current) => ({ ...current, items: [item, ...current.items] }));
      setNotice(`${item.fileName} is safely imported on this device and ready for review.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "The file could not be imported.");
    } finally {
      setBusy(false);
    }
  };

  const queue = async (item: UploadItem) => {
    setBusy(true);
    try {
      const updated = await queueItem(item.id);
      setSnapshot((current) => ({ ...current, items: current.items.map((candidate) => candidate.id === updated.id ? updated : candidate) }));
      setNotice(`${updated.fileName} is saved in the local upload queue.`);
    } finally {
      setBusy(false);
    }
  };

  const reconcile = async () => {
    setBusy(true);
    try {
      const items = await reconcileQueue();
      setSnapshot((current) => ({ ...current, items }));
      setNotice("Saved uploads were reconciled. Provider sessions that cannot be proven safe require review.");
    } finally {
      setBusy(false);
    }
  };

  const startUploads = async () => {
    if (!snapshot.activeChannel) return;
    setBusy(true);
    try {
      const started = await startQueuedUploads();
      await refresh();
      setNotice(`${started} queued upload${started === 1 ? "" : "s"} started for ${snapshot.activeChannel}.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Queued uploads could not be started.");
    } finally {
      setBusy(false);
    }
  };

  const syncLibrary = async () => {
    if (!snapshot.activeChannel) return;
    setBusy(true);
    try {
      const synced = await syncChannelInventory();
      await refresh();
      setNotice(`${synced} YouTube video${synced === 1 ? "" : "s"} synced from ${snapshot.activeChannel}. Duplicate candidates were refreshed locally.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "The YouTube library could not be synced.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <img className="brand-mark" src="/favicon.svg" alt="" />
          <div>
            <p className="eyebrow">LOCAL-FIRST UPLOAD WORKSPACE</p>
            <h1>YouTube Mass Uploader</h1>
            <p className="subtle">{snapshot.activeChannel ? `Active channel: ${snapshot.activeChannel}` : "Your files stay on this device until you start an upload."}</p>
          </div>
        </div>
        <div className="actions">
          <button className="secondary" disabled={busy} onClick={() => void reconcile()}>Recover queue</button>
          {snapshot.activeChannel && <button className="secondary" disabled={busy} onClick={() => void syncLibrary()}>Sync library</button>}
          <button disabled={!snapshot.activeChannel || busy} onClick={() => void startUploads()}>Start uploads</button>
          <button className="import-button" disabled={busy} onClick={() => void addVideo()}>Import videos</button>
        </div>
      </header>

      <p className="notice" role="status"><span aria-hidden="true" />{notice}</p>
      {!isTauri && <p className="warning">Browser preview mode: managed local file import is available only in the signed Tauri app.</p>}

      <section className="queue-workspace" aria-labelledby="queue-heading">
        <div className="section-heading">
          <div><p className="eyebrow">PERSISTENT QUEUE</p><h2 id="queue-heading">Your upload queue</h2><p className="section-copy">Every import and upload state is saved locally, so interrupted work can continue where it stopped.</p></div>
          <span className="item-count">{snapshot.items.length} saved item{snapshot.items.length === 1 ? "" : "s"}</span>
        </div>
        <QueueTable items={snapshot.items} busy={busy} onQueue={(item) => void queue(item)} />
      </section>

      <div className="workspace-grid">
      <ConnectionPanel onConnectionChange={updateConnection} />
      <section className="panel duplicates-panel" aria-labelledby="duplicates-heading">
        <div className="section-heading"><div><p className="eyebrow">REVIEW REQUIRED</p><h2 id="duplicates-heading">Duplicate candidates</h2></div></div>
        <DuplicateReview candidates={snapshot.duplicates} />
      </section>
      </div>

      <section className="panel" aria-labelledby="deletion-heading">
        <div className="section-heading"><div><p className="eyebrow">EXPLICIT LOCAL REVIEW</p><h2 id="deletion-heading">Video removal requests</h2></div></div>
        <DeletionReview activeChannel={snapshot.activeChannel} busy={busy} onNotice={setNotice} />
      </section>
    </main>
  );
}
