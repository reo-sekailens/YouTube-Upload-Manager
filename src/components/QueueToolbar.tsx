import { FileText, Fingerprint, FolderOpen, ListX } from "lucide-react";

type QueueToolbarProps = {
  busy: boolean;
  count: number;
  onClear: () => void;
};

const retentionItems = [
  [FileText, "Records kept", "Local queue records stay saved on this device"],
  [Fingerprint, "BLAKE3 kept", "BLAKE3 hashes stay saved for duplicate evidence"],
  [FolderOpen, "Originals kept", "Original files remain in their chosen locations"],
] as const;

export default function QueueToolbar({ busy, count, onClear }: QueueToolbarProps) {
  return (
    <div className="ui-action-row items-center gap-3">
      <span className="ui-queue-count">{count} saved item{count === 1 ? "" : "s"}</span>
      <div aria-label="Clear queue retention" className="flex flex-wrap items-center gap-1.5" role="group">
        {retentionItems.map(([Icon, label, title]) => (
          <span className="inline-flex items-center gap-1 rounded-full bg-[#f1f5f9] px-2 py-1 text-[0.68rem] font-semibold text-[#4f6078]" key={label} title={title}>
            <Icon aria-hidden="true" size={16} />
            {label}
          </span>
        ))}
      </div>
      <button aria-label="Clear queue" className="ui-button-secondary inline-flex size-9 items-center justify-center p-0" disabled={busy} onClick={onClear} title="Clear queue" type="button">
        <ListX aria-hidden="true" size={16} />
      </button>
    </div>
  );
}
