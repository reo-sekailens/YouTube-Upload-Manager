import { useRef, useState } from "react";
import { exportPortableArchive, importDesktopOAuthClient, importPortableArchive, isTauri } from "../lib/local";
import type { ConnectionSettings, PortableArchiveReceipt } from "../lib/types";
import { useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";

type TransferPanelProps = { onConnectionChange: (settings: ConnectionSettings) => void; onNotice: (message: string) => void };

function sizeLabel(bytes: number) { return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`; }

const secondaryActionClass = "rounded-md border border-[#cdd4df] bg-white px-2.5 py-2 text-[0.73rem] font-semibold text-[#34405a] transition-colors focus-visible:outline-3 focus-visible:outline-brand focus-visible:outline-offset-2 hover:border-[#aeb9c8] hover:bg-[#f3f5f8] disabled:cursor-not-allowed disabled:opacity-55";

export function TransferPanel({ onConnectionChange, onNotice }: TransferPanelProps) {
  const [busy, setBusy] = useState(false);
  const [receipt, setReceipt] = useState<PortableArchiveReceipt>();
  const [error, setError] = useState("");
  const running = useRef(false);
  // Device transfer can import the configuration that changes the connection revision itself.
  const fence = useWorkspaceRequestFence("device-transfer", false);
  const run = async (operation: (current: () => boolean) => Promise<void>) => {
    if (running.current) return;
    running.current = true;
    const current = fence.begin();
    setBusy(true); setError(""); setReceipt(undefined);
    try { await operation(current); }
    catch (cause) { if (current()) setError(workspaceErrorMessage(cause, "The transfer could not finish.")); }
    finally { running.current = false; if (current()) setBusy(false); }
  };
  const exportArchive = async () => {
    if (!isTauri) return onNotice("Run the signed Tauri app to export a portable archive.");
    await run(async (current) => {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: "youtube-upload-manager-dedupe.yumx.gz", filters: [{ name: "YouTube Upload Manager archive", extensions: ["yumx", "gz"] }] });
      if (!path || !current()) return;
      const next = await exportPortableArchive(path);
      if (current()) { setReceipt(next); onNotice(next.detail); }
    });
  };
  const importArchive = async () => {
    if (!isTauri) return onNotice("Run the signed Tauri app to import a portable archive.");
    await run(async (current) => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: false, multiple: false, pickerMode: "document", fileAccessMode: "scoped", filters: [{ name: "YouTube Upload Manager archive", extensions: ["yumx", "gz"] }] });
      if (typeof selected !== "string" || !current()) return;
      const next = await importPortableArchive(selected);
      if (current()) { setReceipt(next); onNotice(next.detail); }
    });
  };
  const importOAuthJson = async () => {
    if (!isTauri) return onNotice("Run the signed Tauri app to import a Desktop OAuth JSON file.");
    await run(async (current) => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: false, multiple: false, pickerMode: "document", fileAccessMode: "scoped", filters: [{ name: "Desktop OAuth JSON", extensions: ["json"] }] });
      if (typeof selected !== "string" || !current()) return;
      const settings = await importDesktopOAuthClient(selected);
      if (current()) { onConnectionChange(settings); onNotice("Desktop OAuth JSON is stored in protected device storage. Connect YouTube when ready."); }
    });
  };
  return <section className="rounded-[0.8rem] border border-line bg-surface p-5" aria-busy={busy} aria-labelledby="transfer-heading">
    <header><div><p className="m-0 text-[0.67rem] font-bold tracking-[0.13em] text-muted">DEVICE TRANSFER</p><h2 className="mt-1 text-[1.15rem] font-bold text-ink" id="transfer-heading">Export and import</h2><p className="m-0 mt-1 text-[0.82rem] leading-relaxed text-muted">Move compact duplicate evidence between installs without copying video files.</p></div></header>
    <div className="mt-4 grid gap-3 sm:grid-cols-2 max-sm:grid-cols-1">
      <section className="rounded-lg border border-[#e1e6ee] bg-[#fafbfc] p-3.5"><h3 className="m-0 text-[0.9rem] font-bold text-[#2d3f5d]">Duplicate metadata archive</h3><p className="m-0 mt-1.5 text-[0.75rem] leading-relaxed text-[#65758b]">Exports BLAKE3 hashes, item titles, and synchronized YouTube inventory with gzip compression. It excludes media, local paths, refresh tokens, OAuth client secrets, and resumable sessions.</p><div className="mt-3 flex flex-wrap gap-2 max-sm:flex-col"><button className={secondaryActionClass} disabled={busy} onClick={() => void exportArchive()} type="button">Export compact archive</button><button className={secondaryActionClass} disabled={busy} onClick={() => void importArchive()} type="button">Import archive</button></div></section>
      <section className="rounded-lg border border-[#e1e6ee] bg-[#fafbfc] p-3.5"><h3 className="m-0 text-[0.9rem] font-bold text-[#2d3f5d]">Desktop OAuth JSON</h3><p className="m-0 mt-1.5 text-[0.75rem] leading-relaxed text-[#65758b]">Import this device’s Google Desktop OAuth JSON separately. Refresh tokens are never exported; connect YouTube again on each install.</p><button className={`mt-3 ${secondaryActionClass}`} disabled={busy} onClick={() => void importOAuthJson()} type="button">Import OAuth JSON</button></section>
    </div>
    {busy && <p className="m-0 mt-3 text-[0.75rem] text-muted" role="status">Preparing or processing the selected transfer…</p>}
    {receipt && <p className="m-0 mt-3 rounded-md border border-[#cfead8] bg-[#edf8f1] px-2.5 py-2 text-[0.74rem] text-[#2d704e]" role="status">{receipt.uploadCount} hash records and {receipt.remoteVideoCount} YouTube records processed · {sizeLabel(receipt.bytes)} archive.</p>}
    {error && <p className="m-0 mt-3 text-[0.75rem] font-semibold text-danger" role="alert">{error}</p>}
  </section>;
}
import "./TransferPanel.lazy.css";
