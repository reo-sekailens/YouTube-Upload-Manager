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

export function DedupeActivityPanel({
  activity,
  busy,
  phase,
}: DedupeActivityPanelProps) {
  if (activity.length === 0) return null;
  return (
    <section className="dedupe-activity" aria-labelledby="dedupe-activity-heading">
      <div className="dedupe-activity__heading">
        <div>
          <p className="eyebrow">DEVICE-LOCAL ACTIVITY</p>
          <h3 id="dedupe-activity-heading">Dedupe activity</h3>
        </div>
        {busy && <span className="dedupe-activity__running">In progress</span>}
      </div>
      <div className={`dedupe-progress dedupe-progress--${phase}`}>
        <div className="dedupe-progress__heading">
          <span>Phase progress</span>
          <strong>
            {dedupeProgressStep(phase)} of {dedupeProgressStepCount}
          </strong>
        </div>
        <div
          aria-describedby="dedupe-progress-detail"
          aria-label={`Dedupe phase progress: ${dedupeProgressLabel(phase)}`}
          aria-valuemax={dedupeProgressStepCount}
          aria-valuemin={0}
          aria-valuenow={dedupeProgressStep(phase)}
          className="dedupe-progress__track"
          role="progressbar"
        >
          <span
            style={{
              width: `${(dedupeProgressStep(phase) / dedupeProgressStepCount) * 100}%`,
            }}
          />
        </div>
        <p id="dedupe-progress-detail">{dedupeProgressLabel(phase)}</p>
      </div>
      <ol
        aria-label="Dedupe activity log"
        aria-live="polite"
        className="dedupe-activity__list"
      >
        {activity.map((entry) => (
          <li
            className={`dedupe-activity__entry dedupe-activity__entry--${entry.state}`}
            key={entry.id}
          >
            <span aria-hidden="true" />
            {entry.message}
          </li>
        ))}
      </ol>
    </section>
  );
}
