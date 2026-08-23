import { useState } from "react";
import { exportPortableArchive, importDesktopOAuthClient, importPortableArchive, isTauri } from "../lib/local";
import type { ConnectionSettings, PortableArchiveReceipt } from "../lib/types";

type TransferPanelProps = { onConnectionChange: (settings: ConnectionSettings) => void; onNotice: (message: string) => void };

function sizeLabel(bytes: number) { return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`; }

export function TransferPanel({ onConnectionChange, onNotice }: TransferPanelProps) {
  const [busy, setBusy] = useState(false);
  const [receipt, setReceipt] = useState<PortableArchiveReceipt>();
  const [error, setError] = useState("");
  const run = async (operation: () => Promise<PortableArchiveReceipt>) => {
    setBusy(true); setError("");
    try { const next = await operation(); setReceipt(next); onNotice(next.detail); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "The transfer could not finish."); }
    finally { setBusy(false); }
  };
  const exportArchive = async () => {
    if (!isTauri) return onNotice("Run the signed Tauri app to export a portable archive.");
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: "youtube-upload-manager-dedupe.yumx.gz", filters: [{ name: "YouTube Upload Manager archive", extensions: ["yumx", "gz"] }] });
    if (path) await run(() => exportPortableArchive(path));
  };
  const importArchive = async () => {
    if (!isTauri) return onNotice("Run the signed Tauri app to import a portable archive.");
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({ directory: false, multiple: false, pickerMode: "document", fileAccessMode: "scoped", filters: [{ name: "YouTube Upload Manager archive", extensions: ["yumx", "gz"] }] });
    if (typeof selected === "string") await run(() => importPortableArchive(selected));
  };
  const importOAuthJson = async () => {
    if (!isTauri) return onNotice("Run the signed Tauri app to import a Desktop OAuth JSON file.");
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({ directory: false, multiple: false, pickerMode: "document", fileAccessMode: "scoped", filters: [{ name: "Desktop OAuth JSON", extensions: ["json"] }] });
    if (typeof selected !== "string") return;
    setBusy(true); setError("");
    try { const settings = await importDesktopOAuthClient(selected); onConnectionChange(settings); onNotice("Desktop OAuth JSON is stored in protected device storage. Connect YouTube when ready."); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "The OAuth JSON could not be imported."); }
    finally { setBusy(false); }
  };
  return <section className="panel transfer-panel" aria-labelledby="transfer-heading">
    <header className="section-heading"><div><p className="eyebrow">DEVICE TRANSFER</p><h2 id="transfer-heading">Export and import</h2><p className="section-copy">Move compact duplicate evidence between installs without copying video files.</p></div></header>
    <div className="transfer-panel__grid">
      <section><h3>Duplicate metadata archive</h3><p>Exports BLAKE3 hashes, item titles, and synchronized YouTube inventory with gzip compression. It excludes media, local paths, refresh tokens, OAuth client secrets, and resumable sessions.</p><div><button className="secondary-action" disabled={busy} onClick={() => void exportArchive()} type="button">Export compact archive</button><button className="secondary-action" disabled={busy} onClick={() => void importArchive()} type="button">Import archive</button></div></section>
      <section><h3>Desktop OAuth JSON</h3><p>Import this device’s Google Desktop OAuth JSON separately. Refresh tokens are never exported; connect YouTube again on each install.</p><button className="secondary-action" disabled={busy} onClick={() => void importOAuthJson()} type="button">Import OAuth JSON</button></section>
    </div>
    {receipt && <p className="transfer-panel__receipt" role="status">{receipt.uploadCount} hash records and {receipt.remoteVideoCount} YouTube records processed · {sizeLabel(receipt.bytes)} archive.</p>}
    {error && <p className="transfer-panel__error" role="alert">{error}</p>}
  </section>;
}
