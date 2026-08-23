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
      ? items.filter((item) =>
          item.title.toLocaleLowerCase().includes(normalizedQuery),
        )
      : items;
  }, [deferredTitleQuery, items]);
  const visibleItems = useMemo(
    () => windowItems(matchingItems, page),
    [matchingItems, page],
  );

  if (items.length === 0) {
    return (
      <p className="queue-table__empty queue-table__empty--state">
        Your upload queue is empty.
      </p>
    );
  }

  return (
    <div
      aria-busy={titleQuery !== deferredTitleQuery}
      className="queue-table-wrap queue-table-rail"
    >
      {pendingSourceDelete && (
        <div className="queue-table__cleanup-confirmation" role="dialog" aria-modal="true" aria-labelledby="source-cleanup-heading">
          <h3 id="source-cleanup-heading">Delete the original file?</h3>
          <p>YouTube has confirmed this upload. The managed app copy and the YouTube video will remain.</p>
          <p>Type <strong>{pendingSourceDelete.fileName}</strong> to permanently delete only its original external file.</p>
          <input
            aria-label="Exact original filename"
            onChange={(event) => setSourceDeleteConfirmation(event.target.value)}
            placeholder={pendingSourceDelete.fileName}
            value={sourceDeleteConfirmation}
          />
          <div className="queue-table__actions">
            <button className="secondary-action" onClick={() => { setPendingSourceDelete(undefined); setSourceDeleteConfirmation(""); }} type="button">Keep original</button>
            <button
              className="danger-button"
              disabled={busy || sourceDeleteConfirmation.trim() !== pendingSourceDelete.fileName}
              onClick={() => {
                onDeleteUploadedSource(pendingSourceDelete, sourceDeleteConfirmation);
                setPendingSourceDelete(undefined);
                setSourceDeleteConfirmation("");
              }}
              type="button"
            >
              Delete original
            </button>
          </div>
        </div>
      )}
      <div className="queue-table__search">
        <label htmlFor="upload-queue-title-search">Search video titles</label>
        <div className="queue-table__search-control">
          <input
            aria-busy={titleQuery !== deferredTitleQuery}
            id="upload-queue-title-search"
            onChange={(event) => {
              setTitleQuery(event.target.value);
              setPage(1);
            }}
            placeholder="Search titles"
            type="search"
            value={titleQuery}
          />
          {titleQuery ? (
            <button
              className="queue-button"
              onClick={() => setTitleQuery("")}
              type="button"
            >
              Clear search
            </button>
          ) : null}
        </div>
      </div>
      <table className="queue-table queue-table--rail">
        <thead>
          <tr>
            <th scope="col">Video</th>
            <th scope="col">Visibility</th>
            <th scope="col">Local identity</th>
            <th scope="col">Transfer</th>
            <th scope="col">Status</th>
          </tr>
        </thead>
        <tbody className="queue-table__body">
          {visibleItems.items.map((item) => {
            const progress = item.totalBytes
              ? Math.min(
                  100,
                  Math.max(
                    0,
                    Math.round((item.confirmedBytes / item.totalBytes) * 100),
                  ),
                )
              : 0;

            return (
              <tr className="queue-table__row" data-queue-record key={item.id}>
                <td className="queue-table__video" data-label="Video">
                  <div className="queue-table__video-copy">
                    <strong>{item.title}</strong>
                    <span>
                      {item.fileName} · {formatSize(item.sizeBytes)}
                    </span>
                  </div>
                </td>
                <td className="queue-table__visibility" data-label="Visibility">
                  <label className="visibility-select">
                    <span className="sr-only">Visibility for {item.title}</span>
                    <select
                      disabled={
                        busy || !["draft", "failed"].includes(item.status)
                      }
                      onChange={(event) =>
                        onVisibilityChange(
                          item,
                          event.target.value as UploadVisibility,
                        )
                      }
                      value={item.visibility}
                    >
                      <option value="private">Private</option>
                      <option value="unlisted">Unlisted</option>
                      <option value="public">Public</option>
                    </select>
                  </label>
                  <span className="queue-table__visibility-note">
                    {item.status === "draft" || item.status === "failed"
                      ? "Set before queueing"
                      : "Locked for this upload"}
                  </span>
                  <span className="queue-table__visibility-note">
                    {item.madeForKids ? "Made for kids" : "Not made for kids"}
                    {item.playlistTitle
                      ? ` · ${item.playlistTitle}`
                      : " · No playlist"}
                  </span>
                  <label className="queue-table__source-cleanup">
                    <input
                      checked={item.deleteSourceAfterUpload}
                      disabled={
                        busy || !["draft", "failed"].includes(item.status)
                      }
                      onChange={(event) =>
                        onDeleteSourceAfterUploadChange(item, event.target.checked)
                      }
                      type="checkbox"
                    />{" "}
                    Automatically delete original after YouTube confirms upload
                  </label>
                  {item.sourceDeleteStatus ? (
                    <span className="queue-table__visibility-note">
                      Original source: {item.sourceDeleteStatus}
                    </span>
                  ) : null}
                </td>
                <td
                  className="queue-table__identity"
                  data-label="Local identity"
                >
                  <span className="queue-table__digest-label">BLAKE3 identity</span>
                  <code className="queue-table__digest">
                    {item.digest
                      ? `${item.digest.slice(0, 12)}…`
                      : "Verifying in background"}
                  </code>
                </td>
                <td className="queue-table__transfer" data-label="Transfer">
                  <div className="queue-progress">
                    <div
                      aria-label={`${progress}% uploaded`}
                      aria-valuemax={100}
                      aria-valuemin={0}
                      aria-valuenow={progress}
                      className="queue-progress__track"
                      role="progressbar"
                    >
                      <div
                        className="queue-progress__value"
                        style={{ width: `${progress}%` }}
                      />
                    </div>
                    <span>{progress}%</span>
                  </div>
                  {item.status === "uploading" ? (
                    <span className="queue-table__eta">{formatEta(item)}</span>
                  ) : null}
                  {item.detail ? (
                    <span className="queue-table__detail">{item.detail}</span>
                  ) : null}
                </td>
                <td className="queue-table__status" data-label="Status">
                  <div className="queue-table__status-stack">
                    <StatusPill status={item.status} />
                    <div className="queue-table__actions">
                      {item.status === "draft" ? (
                        <>
                          <button
                            className="queue-button"
                            disabled={busy}
                            onClick={() => onQueue(item)}
                            type="button"
                          >
                            Add to queue
                          </button>
                          <button
                            className="danger-button queue-button"
                            disabled={busy}
                            onClick={() => onCancel(item)}
                            type="button"
                          >
                            Remove
                          </button>
                        </>
                      ) : (
                        item.status === "uploaded" && !item.deleteSourceAfterUpload && !item.sourceDeleteStatus ? (
                          <button
                            className="danger-button queue-button"
                            disabled={busy}
                            onClick={() => setPendingSourceDelete(item)}
                            type="button"
                          >
                            Delete original…
                          </button>
                        ) : ["queued", "dispatching", "uploading", "needs_reconciliation", "failed", "importing"].includes(item.status) ? (
                          <button
                            className="danger-button queue-button"
                            disabled={busy}
                            onClick={() => onCancel(item)}
                            type="button"
                          >
                            {item.status === "uploading" ? "Cancel upload" : "Remove"}
                          </button>
                        ) : <span className="queue-table__saved">Receipt saved on this device</span>
                      )}
                    </div>
                  </div>
                </td>
              </tr>
            );
          })}
          {matchingItems.length === 0 ? (
            <tr>
              <td className="queue-table__empty" colSpan={5}>
                No video titles match “{titleQuery.trim()}”.
              </td>
            </tr>
          ) : null}
        </tbody>
      </table>
      <PaginationControls
        end={visibleItems.end}
        label="Upload queue"
        onPageChange={setPage}
        page={visibleItems.page}
        pageCount={visibleItems.pageCount}
        start={visibleItems.start}
        total={visibleItems.total}
      />
    </div>
  );
}
