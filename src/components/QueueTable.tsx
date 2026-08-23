import { useDeferredValue, useMemo, useState } from "react";
import type { UploadItem, UploadVisibility } from "../lib/types";
import { windowItems } from "../lib/list-windowing";
import { useRetainedWorkspaceState } from "../lib/retained-workspace-state";
import { PaginationControls } from "./PaginationControls";
import { StatusPill } from "./StatusPill";

interface QueueTableProps {
  items: UploadItem[];
  busy: boolean;
  onQueue: (item: UploadItem) => void;
  onCancel: (item: UploadItem) => void;
  onVisibilityChange: (item: UploadItem, visibility: UploadVisibility) => void;
  onDeleteSourceAfterUploadChange: (item: UploadItem, enabled: boolean) => void;
  onDeleteUploadedSource: (item: UploadItem, confirmation: string) => void;
}

const secondaryButtonClass =
  "cursor-pointer rounded-md border border-[#cdd4df] bg-white px-2.5 py-1.5 text-[0.73rem] font-[680] text-[#344a67] transition-colors hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-55";
const dangerButtonClass =
  "cursor-pointer rounded-md border border-[#e5c2c0] bg-white px-2.5 py-1.5 text-[0.73rem] font-[680] text-[#a4413b] transition-colors hover:border-[#d89d98] hover:bg-[#fff5f4] focus-visible:outline-3 focus-visible:outline-[#c44f463d] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-55";
const dataCellClass =
  "bg-[#fbfcff] px-3 py-3 text-[0.81rem] align-middle text-[#4d5a70] border-y border-[#dfe6f2] first:rounded-l-lg first:border-l last:rounded-r-lg last:border-r max-sm:block max-sm:w-full max-sm:border-0 max-sm:bg-transparent max-sm:px-0 max-sm:py-1 max-sm:before:mb-1 max-sm:before:inline-block max-sm:before:text-[0.62rem] max-sm:before:font-bold max-sm:before:tracking-[0.07em] max-sm:before:text-[#7b8799] max-sm:before:uppercase max-sm:before:content-[attr(data-label)]";

function formatSize(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(bytes >= 10 * 1024 * 1024 * 1024 ? 0 : 1)} GB`;
}

function formatEta(item: UploadItem) {
  if (!item.transferBytesPerSecond || item.transferBytesPerSecond <= 0)
    return "ETA calculating";
  const seconds = Math.ceil(
    Math.max(0, item.totalBytes - item.confirmedBytes) /
      item.transferBytesPerSecond,
  );
  if (seconds >= 3600)
    return `ETA ${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  if (seconds >= 60) return `ETA ${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  return `ETA ${seconds}s`;
}

function sourceDeleteStatusLabel(status: NonNullable<UploadItem["sourceDeleteStatus"]>) {
  switch (status) {
    case "pending":
    case "waiting_for_youtube_processing":
      return "Kept until YouTube processing succeeds";
    case "processing_verified":
      return "Processing verified; safe cleanup is retrying";
    case "retained_youtube_processing_failed":
      return "Kept: YouTube processing failed or was abandoned";
    case "retained_youtube_upload_failed":
      return "Kept: YouTube reported an upload failure";
    case "retained_youtube_upload_rejected":
      return "Kept: YouTube rejected the upload";
    case "deleted":
      return "Deleted after YouTube processing succeeded";
    default:
      return "Kept for safety";
  }
}

export function QueueTable({
  items,
  busy,
  onQueue,
  onCancel,
  onVisibilityChange,
  onDeleteSourceAfterUploadChange,
  onDeleteUploadedSource,
}: QueueTableProps) {
  const [titleQuery, setTitleQuery] = useRetainedWorkspaceState(
    "batch.queue-title-query",
    "",
  );
  const deferredTitleQuery = useDeferredValue(titleQuery);
  const [page, setPage] = useRetainedWorkspaceState("batch.queue-page", 1);
  const [pendingSourceDelete, setPendingSourceDelete] = useState<UploadItem>();
  const [sourceDeleteConfirmation, setSourceDeleteConfirmation] = useState("");

  const matchingItems = useMemo(() => {
    const normalizedQuery = deferredTitleQuery.trim().toLocaleLowerCase();
    return normalizedQuery
      ? items.filter((item) => item.title.toLocaleLowerCase().includes(normalizedQuery))
      : items;
  }, [deferredTitleQuery, items]);
  const visibleItems = useMemo(
    () => windowItems(matchingItems, page),
    [matchingItems, page],
  );

  if (items.length === 0) {
    return <p className="m-0 pt-6 pb-1 text-[0.84rem] leading-relaxed text-[#758196]">Your upload queue is empty.</p>;
  }

  return (
    <div
      aria-busy={titleQuery !== deferredTitleQuery}
      className="overflow-x-auto"
      data-queue-table
    >
      {pendingSourceDelete && (
        <div className="mb-3 grid max-w-xl gap-2.5 rounded-lg border border-[#edc7c3] bg-white p-3.5 shadow-[0_12px_32px_rgba(35,49,72,0.16)]" role="dialog" aria-modal="true" aria-labelledby="source-cleanup-heading">
          <h3 className="m-0 text-[0.95rem] text-[#8d3932]" id="source-cleanup-heading">Delete the original file?</h3>
          <p className="m-0 text-[0.75rem] leading-relaxed text-[#5f6c80]">YouTube processing must be verified before the app can delete this file. The managed app copy and the YouTube video will remain.</p>
          <p className="m-0 text-[0.75rem] leading-relaxed text-[#5f6c80]">Type <strong>{pendingSourceDelete.fileName}</strong> to permanently delete only its original external file.</p>
          <input
            className="rounded-md border border-[#ccd6e4] px-2.5 py-2"
            aria-label="Exact original filename"
            onChange={(event) => setSourceDeleteConfirmation(event.target.value)}
            placeholder={pendingSourceDelete.fileName}
            value={sourceDeleteConfirmation}
          />
          <div className="flex flex-wrap justify-end gap-2">
            <button className={secondaryButtonClass} onClick={() => { setPendingSourceDelete(undefined); setSourceDeleteConfirmation(""); }} type="button">Keep original</button>
            <button className={dangerButtonClass} disabled={busy || sourceDeleteConfirmation.trim() !== pendingSourceDelete.fileName} onClick={() => { onDeleteUploadedSource(pendingSourceDelete, sourceDeleteConfirmation); setPendingSourceDelete(undefined); setSourceDeleteConfirmation(""); }} type="button">Delete original</button>
          </div>
        </div>
      )}
      <div className="mb-3 grid gap-1.5 text-[0.74rem] font-bold text-[#465775]">
        <label htmlFor="upload-queue-title-search">Search video titles</label>
        <div className="flex flex-wrap items-center gap-2">
          <input
            className="min-w-[16rem] max-w-xl flex-1 rounded-md border border-[#cbd5e3] bg-white px-2.5 py-2 font-medium text-[#27344a] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e824]"
            aria-busy={titleQuery !== deferredTitleQuery}
            id="upload-queue-title-search"
            onChange={(event) => { setTitleQuery(event.target.value); setPage(1); }}
            placeholder="Search titles"
            type="search"
            value={titleQuery}
          />
          {titleQuery ? <button className={secondaryButtonClass} onClick={() => setTitleQuery("")} type="button">Clear search</button> : null}
        </div>
      </div>
      <table className="w-full min-w-[860px] border-separate border-spacing-y-2.5 max-sm:min-w-0 max-sm:[&_tbody]:block max-sm:[&_tr]:block max-sm:[&_tr]:w-full max-sm:[&_tr]:border-b max-sm:[&_tr]:border-[#e4e8ee] max-sm:[&_tr]:py-3 max-sm:[&_thead]:hidden" >
        <thead>
          <tr>
            <th className="w-1/4 border-b border-[#dfe4eb] px-2.5 pb-2 text-left text-[0.65rem] font-[740] tracking-[0.08em] text-[#7a8799] uppercase" scope="col">Video</th>
            <th className="w-[22%] border-b border-[#dfe4eb] px-2.5 pb-2 text-left text-[0.65rem] font-[740] tracking-[0.08em] text-[#7a8799] uppercase" scope="col">Visibility</th>
            <th className="w-[15%] border-b border-[#dfe4eb] px-2.5 pb-2 text-left text-[0.65rem] font-[740] tracking-[0.08em] text-[#7a8799] uppercase" scope="col">Local identity</th>
            <th className="w-[24%] border-b border-[#dfe4eb] px-2.5 pb-2 text-left text-[0.65rem] font-[740] tracking-[0.08em] text-[#7a8799] uppercase" scope="col">Transfer</th>
            <th className="w-[14%] border-b border-[#dfe4eb] px-2.5 pb-2 text-left text-[0.65rem] font-[740] tracking-[0.08em] text-[#7a8799] uppercase" scope="col">Status</th>
          </tr>
        </thead>
        <tbody>
          {visibleItems.items.map((item) => {
            const progress = item.totalBytes
              ? Math.min(100, Math.max(0, Math.round((item.confirmedBytes / item.totalBytes) * 100)))
              : 0;
            return (
              <tr className="max-sm:block max-sm:w-full" data-queue-record key={item.id}>
                <td className={dataCellClass} data-label="Video">
                  <div>
                    <strong className="mb-0.5 block overflow-wrap-anywhere font-[680] text-[#28354d]">{item.title}</strong>
                    <span className="block text-[0.72rem] text-[#7b8799]">{item.fileName} · {formatSize(item.sizeBytes)}</span>
                  </div>
                </td>
                <td className={dataCellClass} data-label="Visibility">
                  <label>
                    <span className="sr-only">Visibility for {item.title}</span>
                    <select className="rounded-md border border-[#ccd6e4] bg-white py-1.5 pr-7 pl-2 text-[0.74rem] font-[650] text-[#344a67] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e824]" disabled={busy || !["draft", "failed"].includes(item.status)} onChange={(event) => onVisibilityChange(item, event.target.value as UploadVisibility)} value={item.visibility}>
                      <option value="private">Private</option><option value="unlisted">Unlisted</option><option value="public">Public</option>
                    </select>
                  </label>
                  <span className="mt-1 block text-[0.67rem] text-[#7b8799]">{item.status === "draft" || item.status === "failed" ? "Set before queueing" : "Locked for this upload"}</span>
                  <span className="mt-1 block text-[0.67rem] text-[#7b8799]">{item.madeForKids ? "Made for kids" : "Not made for kids"}{item.playlistTitle ? ` · ${item.playlistTitle}` : " · No playlist"}</span>
                  <label className="mt-2 flex items-start gap-1.5 text-[0.68rem] leading-snug text-[#4f6078] has-[:disabled]:opacity-55">
                    <input className="mt-0.5 shrink-0 accent-[#2463df]" checked={item.deleteSourceAfterUpload} disabled={busy || !["draft", "failed"].includes(item.status)} onChange={(event) => onDeleteSourceAfterUploadChange(item, event.target.checked)} type="checkbox" />
                    Automatically delete original after YouTube processing succeeds
                  </label>
                  {item.sourceDeleteStatus ? <span className="mt-1 block text-[0.67rem] text-[#7b8799]">Original source: {sourceDeleteStatusLabel(item.sourceDeleteStatus)}</span> : null}
                </td>
                <td className={`${dataCellClass} min-w-[8.7rem]`} data-label="Local identity">
                  <span className="block text-[0.64rem] font-[760] tracking-[0.055em] text-[#637796] uppercase">BLAKE3 identity</span>
                  <code className="mt-1 inline-block max-w-full overflow-hidden text-ellipsis whitespace-nowrap text-[0.72rem] text-[#344f7a]">{item.digest ? `${item.digest.slice(0, 12)}…` : "Verifying in background"}</code>
                </td>
                <td className={dataCellClass} data-label="Transfer">
                  <div className="flex min-w-36 items-center gap-2">
                    <div aria-label={`${progress}% uploaded`} aria-valuemax={100} aria-valuemin={0} aria-valuenow={progress} className="h-1.5 w-25 overflow-hidden rounded-full bg-[#e6eaf0]" role="progressbar">
                      <div className="h-full rounded-[inherit] bg-[#2463df]" style={{ width: `${progress}%` }} />
                    </div>
                    <span className="block text-[0.72rem] text-[#66748a]">{progress}%</span>
                  </div>
                  {item.status === "uploading" ? <span className="mt-1 block text-[0.72rem] text-[#66748a]">{formatEta(item)}</span> : null}
                  {item.detail ? <span className="mt-1 block max-w-xs text-[0.72rem] text-[#7b8799]">{item.detail}</span> : null}
                </td>
                <td className={dataCellClass} data-label="Status">
                  <div className="flex flex-col items-start gap-2">
                    <StatusPill status={item.status} />
                    <div className="flex flex-wrap justify-end gap-1.5 max-sm:mt-1 max-sm:justify-start">
                      {item.status === "draft" ? <><button className={secondaryButtonClass} disabled={busy} onClick={() => onQueue(item)} type="button">Add to queue</button><button className={dangerButtonClass} disabled={busy} onClick={() => onCancel(item)} type="button">Remove</button></> : item.status === "uploaded" && !item.deleteSourceAfterUpload && !item.sourceDeleteStatus ? <button className={dangerButtonClass} disabled={busy} onClick={() => setPendingSourceDelete(item)} type="button">Delete original…</button> : ["queued", "dispatching", "uploading", "needs_reconciliation", "failed", "importing"].includes(item.status) ? <button className={dangerButtonClass} disabled={busy} onClick={() => onCancel(item)} type="button">{item.status === "uploading" ? "Cancel upload" : "Remove"}</button> : <span className="text-[0.72rem] leading-snug text-[#7b8799]">Receipt saved on this device</span>}
                    </div>
                  </div>
                </td>
              </tr>
            );
          })}
          {matchingItems.length === 0 ? <tr className="max-sm:block"><td className="pt-1 pb-0 text-[0.84rem] leading-relaxed text-[#758196] max-sm:block" colSpan={5}>No video titles match “{titleQuery.trim()}”.</td></tr> : null}
        </tbody>
      </table>
      <PaginationControls end={visibleItems.end} label="Upload queue" onPageChange={setPage} page={visibleItems.page} pageCount={visibleItems.pageCount} start={visibleItems.start} total={visibleItems.total} />
    </div>
  );
}
