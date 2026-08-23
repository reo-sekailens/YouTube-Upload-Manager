import type { UploadStatus } from "../lib/types";

const labels: Record<UploadStatus, string> = {
  draft: "Ready to queue",
  queued: "Queued",
  dispatching: "Starting upload",
  importing: "Importing",
  uploading: "Uploading",
  needs_reconciliation: "Needs review",
  uploaded: "Uploaded",
  failed: "Failed",
  cancelled: "Cancelled",
};

const toneClasses: Record<UploadStatus, string> = {
  draft: "bg-slate-100 text-slate-600",
  queued: "bg-slate-100 text-slate-600",
  dispatching: "bg-slate-100 text-slate-600",
  importing: "bg-blue-50 text-blue-700",
  uploading: "bg-blue-50 text-blue-700",
  needs_reconciliation: "bg-red-50 text-red-700",
  uploaded: "bg-emerald-50 text-emerald-700",
  failed: "bg-red-50 text-red-700",
  cancelled: "bg-stone-100 text-stone-600",
};

export function StatusPill({ status, label }: { status: UploadStatus; label?: string }) {
  const resolvedLabel = label ?? labels[status];

  return (
    <span
      aria-label={`Upload status: ${resolvedLabel}`}
      className={`inline-flex items-center gap-1.5 whitespace-nowrap rounded-full px-2 py-1 text-[0.7rem] leading-none font-semibold ${toneClasses[status]}`}
    >
      <span aria-hidden="true" className="size-1.5 rounded-full bg-current" />
      <span>{resolvedLabel}</span>
    </span>
  );
}
