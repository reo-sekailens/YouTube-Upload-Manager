import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { filterQueueItems, formatQueueSize, matchesQueueStatus, summarizeQueueTransfers } from "../lib/queue-presentation";
import type { UploadItem, UploadStatus } from "../lib/types";
import { QueueTable, uploadRetryGroups } from "./QueueTable";
import QueueToolbar from "./QueueToolbar";
import { UploadProgressSummary } from "./UploadProgressSummary";

function item(overrides: Partial<UploadItem> = {}): UploadItem {
  return {
    id: "item",
    title: "Cebu video",
    fileName: "cebu.mp4",
    sizeBytes: 50 * 1024 * 1024,
    status: "queued",
    confirmedBytes: 0,
    totalBytes: 100,
    visibility: "private",
    madeForKids: false,
    updatedAt: "2026-10-09T00:00:00Z",
    ...overrides,
  };
}

function renderQueue(items: UploadItem[], busy = false, matching = false) {
  return renderToStaticMarkup(<QueueTable
    busy={busy}
    items={items}
    onCancel={() => undefined}
    onDeleteSourceAfterUploadChange={() => undefined}
    onDeleteUploadedSource={() => undefined}
    onQueue={() => undefined}
    onRetryMatching={matching ? () => undefined : undefined}
    onVisibilityChange={() => undefined}
  />);
}

describe("queue navigation and upload feedback", () => {
  it("displays nonzero megabyte sizes and uses the correct larger unit", () => {
    expect(formatQueueSize(50 * 1024 * 1024)).toBe("50 MB");
    expect(formatQueueSize(2 * 1024 ** 3)).toBe("2.0 GB");
    expect(formatQueueSize(512)).toBe("512 B");
    expect(formatQueueSize(0)).toBe("0 B");
    expect(formatQueueSize(Number.NaN)).toBe("Size unavailable");
    expect(renderQueue([item()])).toContain("cebu.mp4 · 50 MB");
  });

  it("classifies every lifecycle state without treating failed or ambiguous work as active", () => {
    const statuses: UploadStatus[] = ["draft", "importing", "queued", "dispatching", "uploading", "needs_reconciliation", "uploaded", "failed", "cancelled"];
    expect(statuses.filter((status) => matchesQueueStatus(status, "active"))).toEqual(["importing", "queued", "dispatching", "uploading"]);
    expect(statuses.filter((status) => matchesQueueStatus(status, "attention"))).toEqual(["needs_reconciliation", "failed"]);
    expect(statuses.filter((status) => matchesQueueStatus(status, "ready"))).toEqual(["draft"]);
    expect(statuses.filter((status) => matchesQueueStatus(status, "uploaded"))).toEqual(["uploaded"]);
    expect(statuses.filter((status) => matchesQueueStatus(status, "all"))).toEqual(statuses);
  });

  it("combines the local title query with status and preserves input order and records", () => {
    const items = [
      item({ id: "failed-cebu", status: "failed" }),
      item({ id: "queued-cebu" }),
      item({ id: "review-manila", title: "Manila", status: "needs_reconciliation" }),
    ];
    expect(filterQueueItems(items, "  CEBU ", "attention").map((value) => value.id)).toEqual(["failed-cebu"]);
    expect(filterQueueItems(items, "", "attention").map((value) => value.id)).toEqual(["failed-cebu", "review-manila"]);
    expect(filterQueueItems(items, "absent", "all")).toEqual([]);
    expect(filterQueueItems(items, "", "all")).toEqual(items);
    expect(items.map((value) => value.id)).toEqual(["failed-cebu", "queued-cebu", "review-manila"]);
  });

  it("offers failed retry through the existing queue action but keeps ambiguous outcomes out of retry", () => {
    const failed = renderQueue([item({ status: "failed" })]);
    expect(failed).toContain('aria-label="Retry upload: Cebu video"');
    expect(failed).toContain('aria-label="Remove from queue: Cebu video"');
    expect(renderQueue([item({ status: "needs_reconciliation" })])).not.toContain("Retry upload");
    expect(renderQueue([item({ status: "failed" })], true)).toMatch(/aria-label="Retry upload: Cebu video"[^>]*disabled=""/);
  });

  it("places a visible retry beside the transfer error and groups only the same failure", () => {
    const detail = "YouTube rejected the upload setup request.";
    const rows = [item({ id: "first", status: "failed", detail }), item({ id: "second", status: "failed", detail }), item({ id: "different", status: "failed", detail: "A different error." }), item({ id: "confirmed", status: "failed", detail, videoId: "video-a" }), item({ id: "uncertain", status: "needs_reconciliation", detail: "A transfer outcome is uncertain." })];
    const markup = renderQueue(rows, false, true);
    expect(markup).toContain("Retry all with same issue (2)"); expect(markup).not.toContain("Retry all with same issue (3)");
    expect(markup.indexOf("Retry upload: Cebu video")).toBeGreaterThan(markup.indexOf('data-label="Transfer"'));
    expect(markup.match(/aria-label="Retry all with same issue \(2\)"/g)).toHaveLength(1);
    expect(markup.indexOf("Bulk retry matching upload failures")).toBeGreaterThan(markup.indexOf("Filter queue by upload status"));
    expect(markup.indexOf("Bulk retry matching upload failures")).toBeLessThan(markup.indexOf('aria-label="Saved upload queue"'));
    expect(renderQueue([rows[0]], false, true)).not.toContain("Retry all with same issue");
    expect(renderQueue(rows)).not.toContain("Retry all with same issue");
    expect(renderQueue(rows, true, true)).toMatch(/aria-label="Retry all with same issue \(2\)"[^>]*disabled=""/);
    expect(renderQueue([item({ status: "needs_reconciliation", detail })])).toContain('aria-label="Retry upload: Cebu video"');
    expect(renderQueue([item({ status: "needs_reconciliation", detail, confirmedBytes: 1 })])).not.toContain("Retry upload");
  });

  it("offers one bulk action with an issue selector when different failure groups are present", () => {
    const rows = [
      item({ id: "first-a", title: "Camera A", status: "failed", retryFailureKey: "setup:400:invalidTitle" }),
      item({ id: "first-b", title: "Camera B", status: "failed", retryFailureKey: "setup:400:invalidTitle" }),
      item({ id: "second-a", title: "Hall A", status: "failed", retryFailureKey: "setup:400:invalidCategory" }),
      item({ id: "second-b", title: "Hall B", status: "failed", retryFailureKey: "setup:400:invalidCategory" }),
    ];
    const markup = renderQueue(rows, false, true);
    expect(markup).toContain("Upload issue to retry"); expect(markup).toContain("Camera A · 2 uploads with this issue"); expect(markup).toContain("Hall A · 2 uploads with this issue");
    expect(markup.match(/aria-label="Retry all with same issue \(2\)"/g)).toHaveLength(1);
  });

  it("uses filters to offer relevant issues while preserving whole-group counts and a visible anchor", () => {
    const detail = "YouTube rejected the upload setup request.";
    const rows = [item({ id: "hidden", title: "Hidden video", status: "failed", detail }), item({ id: "visible", title: "Visible video", status: "failed", detail }), item({ id: "single", title: "Different", status: "failed", retryFailureKey: "setup:400:invalidTitle" })];
    const filtered = filterQueueItems(rows, "visible", "attention");
    expect(uploadRetryGroups(rows, filtered)).toEqual([{ key: "setup:legacy_unknown", count: 2, anchor: rows[1] }]);
    expect(uploadRetryGroups(rows, filterQueueItems(rows, "", "uploaded"))).toEqual([]);
    expect(uploadRetryGroups(rows, [rows[2]])).toEqual([]);
    expect(uploadRetryGroups(rows, [])).toEqual([]);
  });

  it("offers upload guidance and hides unusable supporting controls when the queue is empty", () => {
    const empty = renderQueue([]);
    expect(empty).toContain("Your upload queue is empty.");
    expect(empty).toContain("Choose videos above");
    expect(empty).not.toContain("Filter queue by upload status");
    const toolbar = renderToStaticMarkup(<QueueToolbar busy={false} count={0} onClear={() => undefined} />);
    expect(toolbar).not.toContain("Clear queue");
  });

  it("shows all active workers and estimates the remaining queue from their combined measured speed", () => {
    const items = [
      item({ id: "first", status: "uploading", totalBytes: 100, confirmedBytes: 40, transferBytesPerSecond: 10 }),
      item({ id: "second", status: "uploading", totalBytes: 300, confirmedBytes: 60, transferBytesPerSecond: 30 }),
      item({ id: "waiting", totalBytes: 100 }),
      item({ id: "completed", status: "uploaded", totalBytes: 200, confirmedBytes: 200 }),
      item({ id: "failed", status: "failed", totalBytes: 200 }),
    ];
    const summary = summarizeQueueTransfers(items);
    expect(summary.active).toHaveLength(2);
    expect(summary.remaining).toHaveLength(3);
    expect(summary.activeBytes).toEqual({ total: 400, confirmed: 100 });
    expect(summary.remainingBytes).toEqual({ total: 500, confirmed: 100 });
    expect(summary.etaSeconds).toBe(10);
    const markup = renderToStaticMarkup(<UploadProgressSummary items={items} />);
    expect(markup).toContain("2 videos · 25%");
    expect(markup).toContain("3 videos · 20%");
    expect(markup).toContain("10s remaining");
    expect(markup).toContain("current combined transfer speed");
  });

  it("withholds a misleading combined ETA until every active worker has a usable rate", () => {
    for (const rate of [undefined, 0, Number.NaN, Number.POSITIVE_INFINITY]) {
      const summary = summarizeQueueTransfers([
        item({ status: "uploading", transferBytesPerSecond: 10 }),
        item({ id: "unmeasured", status: "uploading", transferBytesPerSecond: rate }),
      ]);
      expect(summary.etaSeconds).toBeUndefined();
    }
    expect(summarizeQueueTransfers([item()]).etaSeconds).toBeUndefined();
  });

  it("bounds confirmed progress per video so one stale overshoot cannot conceal another transfer", () => {
    const summary = summarizeQueueTransfers([
      item({ status: "uploading", confirmedBytes: 200, transferBytesPerSecond: 10 }),
      item({ id: "waiting", totalBytes: 100 }),
    ]);
    expect(summary.remainingBytes).toEqual({ total: 200, confirmed: 100 });
    expect(summary.etaSeconds).toBe(10);
  });
});
