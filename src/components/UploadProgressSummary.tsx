import type { UploadItem } from "../lib/types";
import { summarizeQueueTransfers } from "../lib/queue-presentation";

function percent(confirmedBytes: number, totalBytes: number) {
  return totalBytes > 0 ? Math.min(100, Math.max(0, Math.round((confirmedBytes / totalBytes) * 100))) : 0;
}

function formatEta(seconds: number | undefined) {
  if (seconds === undefined || !Number.isFinite(seconds) || seconds < 0) return "Calculating after the next confirmed upload chunk";
  const rounded = Math.ceil(seconds);
  const hours = Math.floor(rounded / 3600);
  const minutes = Math.floor((rounded % 3600) / 60);
  const remainingSeconds = rounded % 60;
  if (hours > 0) return `${hours}h ${minutes}m remaining`;
  if (minutes > 0) return `${minutes}m ${remainingSeconds}s remaining`;
  return `${remainingSeconds}s remaining`;
}

function transferEta(item: UploadItem) {
  const rate = item.transferBytesPerSecond;
  return rate && rate > 0 ? Math.max(0, (item.totalBytes - item.confirmedBytes) / rate) : undefined;
}

export function UploadProgressSummary({ items }: { items: UploadItem[] }) {
  const { active, remaining, activeBytes, remainingBytes, etaSeconds } = summarizeQueueTransfers(items);
  const current = active[0];
  const activePercent = percent(activeBytes.confirmed, activeBytes.total);
  const remainingPercent = percent(remainingBytes.confirmed, remainingBytes.total);

  if (!current && remaining.length === 0) return null;

  return (
    <section className="mb-4 grid grid-cols-2 gap-3.5 rounded-[0.62rem] border border-[#d8e5f8] bg-[#f6f9ff] px-3.5 py-3.5 max-sm:grid-cols-1" aria-label="Upload progress">
      <div className="min-w-0">
        <div className="flex flex-wrap items-baseline justify-between gap-2 text-[0.75rem] font-bold text-[#38516f]">
          <span>{active.length > 1 ? "Active transfers" : "Current video"}</span>
          <strong className="text-[0.72rem] text-[#1c4e91]">{current ? `${active.length > 1 ? `${active.length} videos · ` : ""}${activePercent}%` : "Waiting to start"}</strong>
        </div>
        <div aria-label={current ? `Upload progress for ${active.length > 1 ? "active transfers" : current.title}` : "Current video upload waiting"} aria-valuemax={100} aria-valuemin={0} aria-valuenow={activePercent} className="mt-2 h-2 overflow-hidden rounded-full bg-[#dce7f6]" role="progressbar">
          <span className="block h-full rounded-[inherit] bg-[#2463df] transition-[width] duration-250 motion-reduce:transition-none" style={{ width: `${activePercent}%` }} />
        </div>
        <p className="mt-2 mb-0 break-words text-[0.72rem] leading-relaxed text-[#38516f]">{active.length > 1 ? <>Uploading {active.length} videos in parallel. Each transfer is shown in the queue below.</> : current ? <><strong className="text-[#344a67]">{current.title}</strong> · ETA {formatEta(transferEta(current))}</> : "The next queued video will start when the native uploader is ready."}</p>
      </div>
      <div className="border-l border-[#d8e5f8] pl-3.5 max-sm:border-t max-sm:border-l-0 max-sm:pt-3.5 max-sm:pl-0">
        <div className="flex flex-wrap items-baseline justify-between gap-2 text-[0.75rem] font-bold text-[#38516f]">
          <span>All remaining videos</span>
          <strong className="text-[0.72rem] text-[#1c4e91]">{remaining.length} video{remaining.length === 1 ? "" : "s"} · {remainingPercent}%</strong>
        </div>
        <div aria-label="Upload progress for all remaining videos" aria-valuemax={100} aria-valuemin={0} aria-valuenow={remainingPercent} className="mt-2 h-2 overflow-hidden rounded-full bg-[#dce7f6]" role="progressbar">
          <span className="block h-full rounded-[inherit] bg-[#39866a] transition-[width] duration-250 motion-reduce:transition-none" style={{ width: `${remainingPercent}%` }} />
        </div>
        <p className="mt-2 mb-0 text-[0.72rem] leading-relaxed text-[#38516f]">Estimated time: {formatEta(etaSeconds)}</p>
        {etaSeconds !== undefined && <p className="mt-1 mb-0 text-[0.68rem] leading-relaxed text-[#38516f]">Based on the current combined transfer speed; upload capacity and YouTube limits can change this estimate.</p>}
      </div>
    </section>
  );
}
