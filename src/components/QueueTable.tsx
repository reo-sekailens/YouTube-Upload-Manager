import { useState } from "react";
import type { UploadItem, UploadVisibility } from "../lib/types";
import { StatusPill } from "./StatusPill";

interface QueueTableProps {
  items: UploadItem[];
  busy: boolean;
  onQueue: (item: UploadItem) => void;
  onVisibilityChange: (item: UploadItem, visibility: UploadVisibility) => void;
  onDeleteSourceAfterUploadChange: (item: UploadItem, enabled: boolean) => void;
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
  onVisibilityChange,
  onDeleteSourceAfterUploadChange,
}: QueueTableProps) {
  const [titleQuery, setTitleQuery] = useState("");

  if (items.length === 0) {
    return (
      <p className="queue-table__empty queue-table__empty--state">
        Your upload queue is empty.
      </p>
    );
  }

  const normalizedQuery = titleQuery.trim().toLocaleLowerCase();
  const matchingItems = normalizedQuery
    ? items.filter((item) =>
        item.title.toLocaleLowerCase().includes(normalizedQuery),
      )
    : items;

  return (
    <div className="queue-table-wrap queue-table-rail">
      <div className="queue-table__search">
        <label htmlFor="upload-queue-title-search">Search video titles</label>
        <div className="queue-table__search-control">
          <input
            id="upload-queue-title-search"
            onChange={(event) => setTitleQuery(event.target.value)}
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
            <th scope="col">
              <span className="sr-only">Action</span>
            </th>
          </tr>
        </thead>
        <tbody className="queue-table__body">
          {matchingItems.map((item) => {
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
              <tr className="queue-table__row" key={item.id}>
                <td className="queue-table__video" data-label="Video">
                  <div className="queue-table__video-copy">
                    <strong>{item.title}</strong>
                    <span>
                      {item.fileName} · {formatSize(item.sizeBytes)}
                    </span>
                  </div>
                </td>
                <td
                  className="queue-table__identity"
                  data-label="Local identity"
                >
                  <span className="queue-table__digest-label">SHA-256</span>
                  <code className="queue-table__digest">
                    {item.digest
                      ? `${item.digest.slice(0, 12)}…`
                      : "Calculating…"}
                  </code>
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
                    Delete original after confirmed upload
                  </label>
                  {item.sourceDeleteStatus ? (
                    <span className="queue-table__visibility-note">
                      Original source: {item.sourceDeleteStatus}
                    </span>
                  ) : null}
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
                  <StatusPill status={item.status} />
                </td>
                <td className="queue-table__action" data-label="Action">
                  <div className="queue-table__actions">
                    {item.status === "draft" ? (
                      <button
                        className="queue-button"
                        disabled={busy}
                        onClick={() => onQueue(item)}
                        type="button"
                      >
                        Add to queue
                      </button>
                    ) : (
                      <span className="queue-table__saved">Saved locally</span>
                    )}
                  </div>
                </td>
              </tr>
            );
          })}
          {matchingItems.length === 0 ? (
            <tr>
              <td className="queue-table__empty" colSpan={6}>
                No video titles match “{titleQuery.trim()}”.
              </td>
            </tr>
          ) : null}
        </tbody>
      </table>
    </div>
  );
}
