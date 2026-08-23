import {
  dedupeProgressLabel,
  dedupeProgressStep,
  dedupeProgressStepCount,
} from "../lib/dedupe-activity";
import type {
  DedupeActivityEntry,
  DedupeProgressPhase,
} from "../lib/dedupe-activity";

type DedupeActivityPanelProps = {
  activity: DedupeActivityEntry[];
  busy: boolean;
  phase: DedupeProgressPhase;
};

const progressToneClasses: Record<DedupeProgressPhase, { panel: string; track: string; copy: string }> = {
  idle: {
    panel: "border-[#d8e5f8] bg-[#f6f9ff]",
    track: "bg-brand",
    copy: "text-[#64758a]",
  },
  syncing: {
    panel: "border-[#d8e5f8] bg-[#f6f9ff]",
    track: "bg-brand",
    copy: "text-[#64758a]",
  },
  rebuilding: {
    panel: "border-[#d8e5f8] bg-[#f6f9ff]",
    track: "bg-brand",
    copy: "text-[#64758a]",
  },
  complete: {
    panel: "border-[#d8e5f8] bg-[#f6f9ff]",
    track: "bg-[#39866a]",
    copy: "text-[#64758a]",
  },
  error: {
    panel: "border-[#f0d0cc] bg-[#fff7f6]",
    track: "bg-[#c95146]",
    copy: "text-[#9c4038]",
  },
};

const activityToneClasses: Record<DedupeActivityEntry["state"], { row: string; dot: string }> = {
  running: { row: "border-[#e6eaf0] bg-[#fafbfc] text-[#4f5d73]", dot: "bg-[#7a8799]" },
  success: { row: "border-[#e6eaf0] bg-[#fafbfc] text-[#4f5d73]", dot: "bg-[#2d8960]" },
  error: { row: "border-[#f0d0cc] bg-[#fff7f6] text-[#9c4038]", dot: "bg-[#c95146]" },
};

export function DedupeActivityPanel({
  activity,
  busy,
  phase,
}: DedupeActivityPanelProps) {
  if (activity.length === 0) return null;
  const progressTone = progressToneClasses[phase];
  return (
    <section className="mt-4 border-t border-[#e7eaf0] pt-3.5" aria-labelledby="dedupe-activity-heading">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="mb-0.5 text-[0.67rem] font-bold tracking-[0.13em] text-[#68748a]">DEVICE-LOCAL ACTIVITY</p>
          <h3 className="m-0 text-[0.92rem] font-bold text-[#25314a]" id="dedupe-activity-heading">Dedupe activity</h3>
        </div>
        {busy && <span className="inline-flex items-center gap-1.5 whitespace-nowrap rounded-full bg-[#edf4ff] px-2 py-1.5 text-[0.7rem] font-bold text-[#2b63bc]"><span aria-hidden="true" className="size-1.5 animate-pulse rounded-full bg-current" />In progress</span>}
      </div>
      <div className={`mt-3 rounded-lg border p-3 ${progressTone.panel}`}>
        <div className="flex items-center justify-between text-[0.72rem] font-bold text-[#38516f]">
          <span>Phase progress</span>
          <strong className="text-[0.7rem] text-[#1c4e91]">
            {dedupeProgressStep(phase)} of {dedupeProgressStepCount}
          </strong>
        </div>
        <div
          aria-describedby="dedupe-progress-detail"
          aria-label={`Dedupe phase progress: ${dedupeProgressLabel(phase)}`}
          aria-valuemax={dedupeProgressStepCount}
          aria-valuemin={0}
          aria-valuenow={dedupeProgressStep(phase)}
          className="mt-2 overflow-hidden rounded-full bg-[#dce7f6] h-2"
          role="progressbar"
        >
          <span
            className={`block h-full rounded-full transition-[width] duration-200 ${progressTone.track}`}
            style={{
              width: `${(dedupeProgressStep(phase) / dedupeProgressStepCount) * 100}%`,
            }}
          />
        </div>
        <p className={`mt-1.5 text-[0.71rem] leading-snug ${progressTone.copy}`} id="dedupe-progress-detail">{dedupeProgressLabel(phase)}</p>
      </div>
      <ol
        aria-label="Dedupe activity log"
        aria-live="polite"
        className="m-0 mt-3 grid list-none gap-2 p-0"
      >
        {activity.map((entry) => (
          <li
            className={`flex items-start gap-2 rounded-lg border px-2.5 py-2 text-[0.76rem] leading-relaxed ${activityToneClasses[entry.state].row}`}
            key={entry.id}
          >
            <span aria-hidden="true" className={`mt-1.5 size-1.5 shrink-0 rounded-full ${activityToneClasses[entry.state].dot}`} />
            {entry.message}
          </li>
        ))}
      </ol>
    </section>
  );
}
