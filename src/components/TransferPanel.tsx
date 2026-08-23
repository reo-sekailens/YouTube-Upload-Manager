import { useState } from "react";
import { exportPortableArchive, importDesktopOAuthClient, importPortableArchive, isTauri } from "../lib/local";
import type { ConnectionSettings, PortableArchiveReceipt } from "../lib/types";

type TransferPanelProps = { onConnectionChange: (settings: ConnectionSettings) => void; onNotice: (message: string) => void };

function sizeLabel(bytes: number) { return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`; }

const secondaryActionClass = "rounded-md border border-[#cdd4df] bg-white px-2.5 py-2 text-[0.73rem] font-semibold text-[#34405a] transition-colors hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-55";

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
  return <section className="rounded-[0.8rem] border border-line bg-surface p-5" aria-labelledby="transfer-heading">
    <header><div><p className="m-0 text-[0.67rem] font-bold tracking-[0.13em] text-muted">DEVICE TRANSFER</p><h2 className="mt-1 text-[1.15rem] font-bold text-ink" id="transfer-heading">Export and import</h2><p className="m-0 mt-1 text-[0.82rem] leading-relaxed text-muted">Move compact duplicate evidence between installs without copying video files.</p></div></header>
    <div className="mt-4 grid gap-3 sm:grid-cols-2 max-sm:grid-cols-1">
      <section className="rounded-lg border border-[#e1e6ee] bg-[#fafbfc] p-3.5"><h3 className="m-0 text-[0.9rem] font-bold text-[#2d3f5d]">Duplicate metadata archive</h3><p className="m-0 mt-1.5 text-[0.75rem] leading-relaxed text-[#65758b]">Exports BLAKE3 hashes, item titles, and synchronized YouTube inventory with gzip compression. It excludes media, local paths, refresh tokens, OAuth client secrets, and resumable sessions.</p><div className="mt-3 flex flex-wrap gap-2 max-sm:flex-col"><button className={secondaryActionClass} disabled={busy} onClick={() => void exportArchive()} type="button">Export compact archive</button><button className={secondaryActionClass} disabled={busy} onClick={() => void importArchive()} type="button">Import archive</button></div></section>
      <section className="rounded-lg border border-[#e1e6ee] bg-[#fafbfc] p-3.5"><h3 className="m-0 text-[0.9rem] font-bold text-[#2d3f5d]">Desktop OAuth JSON</h3><p className="m-0 mt-1.5 text-[0.75rem] leading-relaxed text-[#65758b]">Import this device’s Google Desktop OAuth JSON separately. Refresh tokens are never exported; connect YouTube again on each install.</p><button className={`mt-3 ${secondaryActionClass}`} disabled={busy} onClick={() => void importOAuthJson()} type="button">Import OAuth JSON</button></section>
    </div>
    {receipt && <p className="m-0 mt-3 rounded-md border border-[#cfead8] bg-[#edf8f1] px-2.5 py-2 text-[0.74rem] text-[#2d704e]" role="status">{receipt.uploadCount} hash records and {receipt.remoteVideoCount} YouTube records processed · {sizeLabel(receipt.bytes)} archive.</p>}
    {error && <p className="m-0 mt-3 text-[0.75rem] font-semibold text-danger" role="alert">{error}</p>}
  </section>;
}
