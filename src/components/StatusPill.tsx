import type { UploadStatus } from "../lib/types";

const labels: Record<UploadStatus, string> = {
  draft: "Ready to queue",
  queued: "Queued",
  importing: "Importing",
  uploading: "Uploading",
  needs_reconciliation: "Needs review",
  uploaded: "Uploaded",
  failed: "Failed",
  cancelled: "Cancelled",
};

export function StatusPill({ status, label }: { status: UploadStatus; label?: string }) {
  const resolvedLabel = label ?? labels[status];

  return (
    <span aria-label={`Upload status: ${resolvedLabel}`} className={`status-pill status-pill--${status.replaceAll("_", "-")}`}>
      <span aria-hidden="true" className="status-pill__dot" />
      <span className="status-pill__label">{resolvedLabel}</span>
    </span>
  );
}
