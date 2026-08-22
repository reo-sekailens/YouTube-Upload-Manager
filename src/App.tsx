import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { importAsset, isTauri, loadSnapshot, queueItem, setItemVisibility, startQueuedUploads, syncChannelInventory } from "./lib/local";
import type { DashboardSnapshot, ManualUploadSettings, UploadItem, UploadVisibility } from "./lib/types";
import type { ConnectionSettings } from "./lib/types";
import { ConnectionPanel } from "./components/ConnectionPanel";
import { DeletionReview } from "./components/DeletionReview";
import { DuplicateReview } from "./components/DuplicateReview";
import { FolderMonitorPanel } from "./components/FolderMonitorPanel";
import { QueueTable } from "./components/QueueTable";
import { UploadProgressSummary } from "./components/UploadProgressSummary";
import { UploadIntakeReview } from "./components/UploadIntakeReview";
import { ManualUploadDefaultsPanel } from "./components/ManualUploadDefaultsPanel";
import { dedupeProgressLabel, dedupeProgressStep, dedupeProgressStepCount, recordDedupeActivity } from "./lib/dedupe-activity";
import type { DedupeActivityState, DedupeActivityEntry, DedupeProgressPhase } from "./lib/dedupe-activity";

const emptySnapshot: DashboardSnapshot = { items: [], duplicates: [] };
const supportedVideoExtensions = new Set(["3g2", "3gp", "avi", "flv", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "webm", "wmv"]);

function isSupportedVideoPath(path: string) {
  const extension = path.split(".").pop()?.toLowerCase();
  return extension !== undefined && supportedVideoExtensions.has(extension);
}

export default function App() {
  const [snapshot, setSnapshot] = useState<DashboardSnapshot>(emptySnapshot);
  const [notice, setNotice] = useState("Local queue is ready. No media leaves this device except during a YouTube upload.");
  const [busy, setBusy] = useState(false);
  const [dedupeBusy, setDedupeBusy] = useState(false);
  const [dedupeActivity, setDedupeActivity] = useState<DedupeActivityEntry[]>([]);
  const [dedupePhase, setDedupePhase] = useState<DedupeProgressPhase>("idle");
  const [dropActive, setDropActive] = useState(false);
  const [pendingImportPaths, setPendingImportPaths] = useState<string[]>();
  const dedupeActivityId = useRef(0);

  const updateConnection = useCallback((settings: ConnectionSettings) => {
    setSnapshot((current) => ({ ...current, activeChannel: settings.activeChannel }));
  }, []);

  const refresh = useCallback(async () => {
    const next = await loadSnapshot();
    setSnapshot(next);
    return next;
  }, []);

  const logDedupeActivity = useCallback((state: DedupeActivityState, message: string) => {
    dedupeActivityId.current += 1;
    setDedupeActivity((entries) => recordDedupeActivity(entries, {
      id: dedupeActivityId.current,
      state,
      message,
    }));
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const reviewImport = useCallback((paths: string[]) => {
    const videos = paths.filter(isSupportedVideoPath);
    if (videos.length === 0) {
      setNotice("Drop a supported video file: MP4, MOV, MKV, WebM, AVI, or another supported video format.");
      return;
    }
    setPendingImportPaths(videos);
  }, []);

  const importVideos = useCallback(async (paths: string[], settings: ManualUploadSettings) => {
    setBusy(true);
    const failures: string[] = [];
    const importedItems: UploadItem[] = [];
    try {
      for (const path of paths) {
        try {
          importedItems.push(await importAsset(path, settings));
        } catch (error) {
          failures.push(error instanceof Error ? error.message : "A video could not be imported.");
        }
      }
      if (importedItems.length === 0) {
        setNotice(`${failures.length} video${failures.length === 1 ? "" : "s"} could not be imported. ${failures[0] ?? ""}`.trim());
        return;
      }
      if (!snapshot.activeChannel) {
        await refresh();
        setNotice(`${importedItems.length} video${importedItems.length === 1 ? "" : "s"} safely imported to this device. Connect YouTube to start uploading them.`);
        return;
      }
      const queueFailures: string[] = [];
      for (const item of importedItems) {
        try {
          await queueItem(item.id);
        } catch (error) {
          queueFailures.push(error instanceof Error ? error.message : "A video could not be queued.");
        }
      }
      let startNotice = "";
      if (queueFailures.length === 0) {
        try {
          const started = await startQueuedUploads();
          startNotice = `${started} queued upload${started === 1 ? "" : "s"} started.`;
        } catch (error) {
          startNotice = error instanceof Error ? error.message : "The saved uploads could not be started.";
        }
      }
      await refresh();
      const importNotice = `${importedItems.length} video${importedItems.length === 1 ? "" : "s"} imported and queued locally.`;
      const failuresNotice = failures.length > 0 || queueFailures.length > 0
        ? ` ${failures.length + queueFailures.length} item${failures.length + queueFailures.length === 1 ? "" : "s"} need attention.`
        : "";
      setNotice(`${importNotice} ${startNotice}${failuresNotice}`.trim());
    } finally {
      setBusy(false);
    }
  }, [refresh, snapshot.activeChannel]);

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === "enter" || event.payload.type === "over") setDropActive(true);
      else if (event.payload.type === "leave") setDropActive(false);
      else if (event.payload.type === "drop") {
        setDropActive(false);
        reviewImport(event.payload.paths);
      }
    }).then((stop) => {
      if (disposed) stop(); else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [reviewImport]);

  useEffect(() => {
    if (!snapshot.items.some((item) => item.status === "uploading" || item.status === "dispatching")) return;
    const interval = window.setInterval(() => { void refresh(); }, 1000);
    return () => window.clearInterval(interval);
  }, [refresh, snapshot.items]);

  const addVideo = async () => {
    if (!isTauri) {
      setNotice("Run this screen through Tauri to import a file into the managed local workspace.");
      return;
    }
    const selected = await open({ multiple: true, directory: false, filters: [{ name: "Video", extensions: [...supportedVideoExtensions] }] });
    reviewImport(typeof selected === "string" ? [selected] : selected ?? []);
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

  const changeVisibility = async (item: UploadItem, visibility: UploadVisibility) => {
    setBusy(true);
    try {
      const updated = await setItemVisibility(item.id, visibility);
      setSnapshot((current) => ({ ...current, items: current.items.map((candidate) => candidate.id === updated.id ? updated : candidate) }));
      setNotice(`${updated.fileName} will upload as ${visibility}.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "The upload visibility could not be changed.");
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

  const runDedupe = async () => {
    const channel = snapshot.activeChannel;
    if (!channel) {
      setNotice("Connect a YouTube channel before running duplicate detection.");
      return;
    }
    setBusy(true);
    setDedupeBusy(true);
    setDedupeActivity([]);
    setDedupePhase("syncing");
    logDedupeActivity("running", `Started duplicate detection for ${channel}.`);
    logDedupeActivity("running", "Synchronizing this channel's uploaded-video inventory from YouTube…");
    setNotice(`Checking ${channel}'s uploaded-video titles for duplicates…`);
    try {
      const synced = await syncChannelInventory();
      logDedupeActivity("success", `Synced ${synced} uploaded video${synced === 1 ? "" : "s"} into this device's channel inventory.`);
      setDedupePhase("rebuilding");
      logDedupeActivity("running", "Rebuilding normalized exact-title and numbered-copy candidates locally…");
      const refreshed = await refresh();
      const candidateCount = refreshed.duplicates.length;
      logDedupeActivity("success", `Ready for review: ${candidateCount} duplicate candidate${candidateCount === 1 ? "" : "s"}. No videos were removed.`);
      setDedupePhase("complete");
      setNotice(`Dedupe complete for ${channel}: ${synced} uploaded video${synced === 1 ? "" : "s"} checked and ${candidateCount} candidate${candidateCount === 1 ? "" : "s"} ready for review.`);
    } catch (error) {
      const message = error instanceof Error ? error.message : "The YouTube library could not be synchronized.";
      setDedupePhase("error");
      logDedupeActivity("error", `Dedupe stopped: ${message}`);
      setNotice(`Dedupe could not run: ${message}`);
    } finally {
      setDedupeBusy(false);
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
            <h1>YouTube Upload Manager</h1>
            <p className="subtle">{snapshot.activeChannel ? `Active channel: ${snapshot.activeChannel}` : "Your files stay on this device until you start an upload."}</p>
          </div>
        </div>
        <div className="actions">
          <button disabled={!snapshot.activeChannel || busy} onClick={() => void startUploads()}>Start uploads</button>
          <button className="import-button" disabled={busy} onClick={() => void addVideo()}>Import videos</button>
        </div>
      </header>

      <p className="notice" role="status"><span aria-hidden="true" />{notice}</p>
      {pendingImportPaths && <UploadIntakeReview paths={pendingImportPaths} onCancel={() => setPendingImportPaths(undefined)} onConfirm={(settings) => { const paths = pendingImportPaths; setPendingImportPaths(undefined); void importVideos(paths, settings); }} />}
      {!isTauri && <p className="warning">Browser preview mode: managed local file import is available only in the signed Tauri app.</p>}

      <section className="queue-workspace" aria-labelledby="queue-heading">
        <div className="section-heading">
          <div><p className="eyebrow">PERSISTENT QUEUE</p><h2 id="queue-heading">Your upload queue</h2><p className="section-copy">Every import and upload state is saved locally, so interrupted work can continue where it stopped.</p></div>
          <span className="item-count">{snapshot.items.length} saved item{snapshot.items.length === 1 ? "" : "s"}</span>
        </div>
        <UploadProgressSummary items={snapshot.items} />
        <ManualUploadDefaultsPanel />
        <div className={`queue-dropzone${dropActive ? " queue-dropzone--active" : ""}`}>
          <div className="queue-dropzone__copy">
            <strong>Drag and drop videos here</strong>
            <p>They are copied into this device’s managed workspace before any upload. You can also choose multiple files.</p>
          </div>
          <button className="secondary-action" disabled={busy} onClick={() => void addVideo()} type="button">Choose videos</button>
        </div>
        <QueueTable items={snapshot.items} busy={busy} onQueue={(item) => void queue(item)} onVisibilityChange={(item, visibility) => void changeVisibility(item, visibility)} />
      </section>

      <FolderMonitorPanel activeChannel={snapshot.activeChannel} onNotice={setNotice} onQueueRefresh={async () => { await refresh(); }} />

      <div className="workspace-grid">
      <ConnectionPanel onConnectionChange={updateConnection} />
      <section className="panel duplicates-panel" aria-labelledby="duplicates-heading">
        <div className="section-heading duplicate-heading">
          <div>
            <p className="eyebrow">REVIEW REQUIRED</p>
            <h2 id="duplicates-heading">Duplicate candidates</h2>
            <p className="section-copy">
              {snapshot.activeChannel
                ? `Check ${snapshot.activeChannel}'s uploaded-video titles for exact matches and numbered copies.`
                : "Connect a YouTube channel to check its uploaded-video titles for duplicates."}
            </p>
          </div>
          <button
            className="secondary-action dedupe-action"
            disabled={!snapshot.activeChannel || busy}
            aria-busy={dedupeBusy}
            onClick={() => void runDedupe()}
          >
            {dedupeBusy ? "Running dedupe…" : "Run dedupe"}
          </button>
        </div>
        <DuplicateReview candidates={snapshot.duplicates} />
        {dedupeActivity.length > 0 && (
          <section className="dedupe-activity" aria-labelledby="dedupe-activity-heading">
            <div className="dedupe-activity__heading">
              <div>
                <p className="eyebrow">DEVICE-LOCAL ACTIVITY</p>
                <h3 id="dedupe-activity-heading">Dedupe activity</h3>
              </div>
              {dedupeBusy && <span className="dedupe-activity__running">In progress</span>}
            </div>
            <div className={`dedupe-progress dedupe-progress--${dedupePhase}`}>
              <div className="dedupe-progress__heading">
                <span>Phase progress</span>
                <strong>{dedupeProgressStep(dedupePhase)} of {dedupeProgressStepCount}</strong>
              </div>
              <div
                aria-describedby="dedupe-progress-detail"
                aria-label={`Dedupe phase progress: ${dedupeProgressLabel(dedupePhase)}`}
                aria-valuemax={dedupeProgressStepCount}
                aria-valuemin={0}
                aria-valuenow={dedupeProgressStep(dedupePhase)}
                className="dedupe-progress__track"
                role="progressbar"
              >
                <span style={{ width: `${(dedupeProgressStep(dedupePhase) / dedupeProgressStepCount) * 100}%` }} />
              </div>
              <p id="dedupe-progress-detail">{dedupeProgressLabel(dedupePhase)}</p>
            </div>
            <ol className="dedupe-activity__list" aria-live="polite" aria-label="Dedupe activity log">
              {dedupeActivity.map((entry) => (
                <li className={`dedupe-activity__entry dedupe-activity__entry--${entry.state}`} key={entry.id}>
                  <span aria-hidden="true" />
                  {entry.message}
                </li>
              ))}
            </ol>
          </section>
        )}
      </section>
      </div>

      <section className="panel" aria-labelledby="deletion-heading">
        <div className="section-heading"><div><p className="eyebrow">EXPLICIT LOCAL REVIEW</p><h2 id="deletion-heading">Video removal requests</h2></div></div>
        <DeletionReview activeChannel={snapshot.activeChannel} busy={busy} onNotice={setNotice} />
      </section>

      <footer className="app-disclaimer">
        YouTube Upload Manager is an independent project and is not affiliated with, endorsed by, sponsored by, or provided by Google or YouTube. Google and YouTube are trademarks of Google LLC; all other names, logos, and trademarks belong to their respective owners.
      </footer>
    </main>
  );
}
