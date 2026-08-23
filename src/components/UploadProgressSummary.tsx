import type { UploadItem } from "../lib/types";

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
  const current = items.find((item) => item.status === "uploading");
  const remaining = items.filter((item) => ["queued", "dispatching", "uploading"].includes(item.status));
  const remainingBytes = remaining.reduce((sum, item) => sum + Math.max(0, item.totalBytes - item.confirmedBytes), 0);
  const remainingTotal = remaining.reduce((sum, item) => sum + item.totalBytes, 0);
  const rate = current?.transferBytesPerSecond;
  const totalEta = rate && rate > 0 ? remainingBytes / rate : undefined;

  if (!current && remaining.length === 0) return null;

  return (
    <section className="mb-4 grid grid-cols-2 gap-3.5 rounded-[0.62rem] border border-[#d8e5f8] bg-[#f6f9ff] px-3.5 py-3.5 max-sm:grid-cols-1" aria-live="polite" aria-label="Upload progress">
      <div>
        <div className="flex items-baseline justify-between gap-2 text-[0.75rem] font-bold text-[#38516f]">
          <span>Current video</span>
          <strong className="text-[0.72rem] text-[#1c4e91]">{current ? `${percent(current.confirmedBytes, current.totalBytes)}%` : "Waiting to start"}</strong>
        </div>
        <div aria-label={current ? `${percent(current.confirmedBytes, current.totalBytes)}% of current video uploaded` : "Current video upload waiting"} aria-valuemax={100} aria-valuemin={0} aria-valuenow={current ? percent(current.confirmedBytes, current.totalBytes) : 0} className="mt-2 h-2 overflow-hidden rounded-full bg-[#dce7f6]" role="progressbar">
          <span className="block h-full rounded-[inherit] bg-[#2463df] transition-[width] duration-250" style={{ width: `${current ? percent(current.confirmedBytes, current.totalBytes) : 0}%` }} />
        </div>
        <p className="mt-2 mb-0 text-[0.72rem] leading-snug text-[#64758a]">{current ? <><strong className="text-[#344a67]">{current.title}</strong> · ETA {formatEta(transferEta(current))}</> : "The next queued video will start when the native uploader is ready."}</p>
      </div>
      <div className="border-l border-[#d8e5f8] pl-3.5 max-sm:border-t max-sm:border-l-0 max-sm:pt-3.5 max-sm:pl-0">
        <div className="flex items-baseline justify-between gap-2 text-[0.75rem] font-bold text-[#38516f]">
          <span>All remaining videos</span>
          <strong className="text-[0.72rem] text-[#1c4e91]">{remaining.length} video{remaining.length === 1 ? "" : "s"} · {remainingTotal > 0 ? `${percent(remainingTotal - remainingBytes, remainingTotal)}%` : "0%"}</strong>
        </div>
        <div aria-label={`${remainingTotal > 0 ? percent(remainingTotal - remainingBytes, remainingTotal) : 0}% of all remaining videos uploaded`} aria-valuemax={100} aria-valuemin={0} aria-valuenow={remainingTotal > 0 ? percent(remainingTotal - remainingBytes, remainingTotal) : 0} className="mt-2 h-2 overflow-hidden rounded-full bg-[#dce7f6]" role="progressbar">
          <span className="block h-full rounded-[inherit] bg-[#39866a] transition-[width] duration-250" style={{ width: `${remainingTotal > 0 ? percent(remainingTotal - remainingBytes, remainingTotal) : 0}%` }} />
        </div>
        <p className="mt-2 mb-0 text-[0.72rem] leading-snug text-[#64758a]">ETA {formatEta(totalEta)}</p>
      </div>
    </section>
  );
}
