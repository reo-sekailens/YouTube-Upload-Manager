import type { DuplicateCandidate } from "../lib/types";

export function DuplicateReview({ candidates }: { candidates: DuplicateCandidate[] }) {
  if (candidates.length === 0) {
    return <p className="duplicate-review__empty duplicate-review__empty--state">No duplicate candidates found in this account.</p>;
  }

  return (
    <div className="duplicate-review duplicate-review--rail" role="list">
      {candidates.map((candidate) => {
        const isExact = candidate.confidence === "exact_local";
        const decision = candidate.decision?.replaceAll("_", " ") ?? "Unreviewed";

        return (
          <section className="duplicate-group duplicate-group--rail" key={candidate.id} aria-label={`Duplicate candidate: ${candidate.evidence}`} role="listitem">
            <header className="duplicate-group__header">
              <div className="duplicate-group__summary">
                <span className={`match-badge match-badge--${isExact ? "exact" : "possible"}`}>
                  {isExact ? "Exact local match" : "Review required"}
                </span>
                <p>{candidate.evidence}</p>
              </div>
              <span className="duplicate-group__count">{decision}</span>
            </header>
            <div className="duplicate-group__items">
              <article className="duplicate-card duplicate-card--rail">
                <div className="duplicate-card__thumbnail" aria-hidden="true"><span>↔</span></div>
                <div className="duplicate-card__body">
                  <span className="duplicate-card__source">Local vs YouTube library</span>
                  <h3>{candidate.leftTitle}</h3>
                  <p>{candidate.rightTitle}</p>
                </div>
                <aside className="duplicate-card__actions" aria-label="Duplicate candidate safeguards">
                  <span className="duplicate-card__review">Deletion requires a separate explicit confirmation.</span>
                </aside>
              </article>
            </div>
          </section>
        );
      })}
    </div>
  );
}
