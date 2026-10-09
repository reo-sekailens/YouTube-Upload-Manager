import type { UploadItem, UploadStatus } from "./types";

export const queueStatusFilters = [
  { id: "all", label: "All" },
  { id: "active", label: "In progress" },
  { id: "attention", label: "Needs attention" },
  { id: "ready", label: "Ready to queue" },
  { id: "uploaded", label: "Uploaded" },
] as const;

export type QueueStatusFilter = (typeof queueStatusFilters)[number]["id"];

export function matchesQueueStatus(status: UploadStatus, filter: QueueStatusFilter) {
  switch (filter) {
    case "active":
      return ["importing", "queued", "dispatching", "uploading"].includes(status);
    case "attention":
      return status === "failed" || status === "needs_reconciliation";
    case "ready":
      return status === "draft";
    case "uploaded":
      return status === "uploaded";
    default:
      return true;
  }
}

export function filterQueueItems(
  items: UploadItem[],
  titleQuery: string,
  filter: QueueStatusFilter,
) {
  const query = titleQuery.trim().toLocaleLowerCase();
  if (!query && filter === "all") return items;
  return items.filter((item) =>
    matchesQueueStatus(item.status, filter) &&
    (!query || item.title.toLocaleLowerCase().includes(query)),
  );
}

export function formatQueueSize(bytes: number) {
  if (!Number.isFinite(bytes) || bytes < 0) return "Size unavailable";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

function safeBytes(value: number) {
  return Number.isFinite(value) ? Math.max(0, value) : 0;
}

export function summarizeQueueTransfers(items: UploadItem[]) {
  const active = items.filter((item) => item.status === "uploading");
  const remaining = items.filter((item) =>
    ["queued", "dispatching", "uploading"].includes(item.status),
  );
  const sumBytes = (values: UploadItem[]) => values.reduce((sum, item) => {
    const total = safeBytes(item.totalBytes);
    return {
      total: sum.total + total,
      confirmed: sum.confirmed + Math.min(total, safeBytes(item.confirmedBytes)),
    };
  }, { total: 0, confirmed: 0 });
  const activeBytes = sumBytes(active);
  const remainingBytes = sumBytes(remaining);
  // One unmeasured worker makes the combined rate incomplete. Wait for every
  // active transfer's native measurement before projecting a batch estimate.
  const hasMeasuredRate = active.length > 0 && active.every((item) =>
    item.transferBytesPerSecond !== undefined &&
    Number.isFinite(item.transferBytesPerSecond) &&
    item.transferBytesPerSecond > 0,
  );
  const combinedRate = hasMeasuredRate
    ? active.reduce((sum, item) => sum + (item.transferBytesPerSecond ?? 0), 0)
    : undefined;
  return {
    active,
    remaining,
    activeBytes,
    remainingBytes,
    etaSeconds: combinedRate
      ? (remainingBytes.total - remainingBytes.confirmed) / combinedRate
      : undefined,
  };
}
