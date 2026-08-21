import type { UploadItem } from "../lib/types";
import { StatusPill } from "./StatusPill";

interface QueueTableProps {
  items: UploadItem[];
  busy: boolean;
  onQueue: (item: UploadItem) => void;
}

function formatSize(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(bytes >= 10 * 1024 * 1024 * 1024 ? 0 : 1)} GB`;
}

export function QueueTable({ items, busy, onQueue }: QueueTableProps) {
  if (items.length === 0) {
    return <p className="queue-table__empty queue-table__empty--state">Your upload queue is empty.</p>;
  }

  return (
    <div className="queue-table-wrap queue-table-rail">
      <table className="queue-table queue-table--rail">
        <thead>
          <tr>
            <th scope="col">Video</th>
            <th scope="col">Local identity</th>
            <th scope="col">Transfer</th>
            <th scope="col">Status</th>
            <th scope="col"><span className="sr-only">Action</span></th>
          </tr>
        </thead>
        <tbody className="queue-table__body">
          {items.map((item) => {
            const progress = item.totalBytes
              ? Math.min(100, Math.max(0, Math.round((item.confirmedBytes / item.totalBytes) * 100)))
              : 0;

            return (
              <tr className="queue-table__row" key={item.id}>
                <td className="queue-table__video" data-label="Video">
                  <div className="queue-table__video-copy">
                    <strong>{item.title}</strong>
                    <span>{item.fileName} · {formatSize(item.sizeBytes)}</span>
                  </div>
                </td>
                <td className="queue-table__identity" data-label="Local identity">
                  <span className="queue-table__digest-label">SHA-256</span>
                  <code className="queue-table__digest">{item.digest ? `${item.digest.slice(0, 12)}…` : "Calculating…"}</code>
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
                      <div className="queue-progress__value" style={{ width: `${progress}%` }} />
                    </div>
                    <span>{progress}%</span>
                  </div>
                  {item.detail ? <span className="queue-table__detail">{item.detail}</span> : null}
                </td>
                <td className="queue-table__status" data-label="Status">
                  <StatusPill status={item.status} />
                </td>
                <td className="queue-table__action" data-label="Action">
                  <div className="queue-table__actions">
                    {item.status === "draft" ? (
                      <button className="queue-button" disabled={busy} onClick={() => onQueue(item)} type="button">Add to queue</button>
                    ) : (
                      <span className="queue-table__saved">Saved locally</span>
                    )}
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
