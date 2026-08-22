import { useEffect, useMemo, useState } from "react";
import type { UploadTitleDuplicate, UploadTitleDuplicateDecision } from "../lib/types";

type UploadTitleDuplicateReviewProps = {
  candidates: UploadTitleDuplicate[];
  busy?: boolean;
  onResolve: (itemIds: string[], decision: UploadTitleDuplicateDecision) => void | Promise<void>;
};

/**
 * Holds matching titles at a deliberate decision point. The active item advances
 * one at a time unless the operator elects to apply the same decision to all.
 */
export function UploadTitleDuplicateReview({ candidates, busy = false, onResolve }: UploadTitleDuplicateReviewProps) {
  const [activeItemId, setActiveItemId] = useState<string | undefined>(candidates[0]?.itemId);
  const [decision, setDecision] = useState<UploadTitleDuplicateDecision>("skip");
  const [applyToAll, setApplyToAll] = useState(false);

  useEffect(() => {
    setActiveItemId((current) => candidates.some((candidate) => candidate.itemId === current) ? current : candidates[0]?.itemId);
  }, [candidates]);

  const activeIndex = Math.max(0, candidates.findIndex((candidate) => candidate.itemId === activeItemId));
  const activeCandidate = candidates[activeIndex];
  const targetIds = useMemo(
    () => applyToAll ? candidates.map((candidate) => candidate.itemId) : activeCandidate ? [activeCandidate.itemId] : [],
    [activeCandidate, applyToAll, candidates],
  );

  if (!activeCandidate) return null;

  const submit = () => {
    if (targetIds.length === 0 || busy) return;
    void onResolve(targetIds, decision);
  };
  const isIgnore = decision === "ignore";
  const matchScopeLabel = activeCandidate.matchScope === "youtube"
    ? "YouTube library"
    : activeCandidate.matchScope === "local_queue"
      ? "Local batch or queue"
      : "YouTube library and local queue";
  const submitLabel = busy
    ? "Saving…"
    : applyToAll
      ? isIgnore ? "Upload all anyway" : "Skip all duplicates"
      : candidates.length > 1 ? "Review next" : "Finish review";

  return (
    <section aria-labelledby="upload-title-duplicate-heading" className="panel upload-title-duplicate-review" role="region">
      <header className="section-heading upload-title-duplicate-review__heading">
        <div>
          <p className="eyebrow">UPLOAD CHECK REQUIRED</p>
          <h2 id="upload-title-duplicate-heading">A possible duplicate needs review</h2>
          <p className="section-copy">Native light dedupe found a normalized filename/title match before upload. This file has not been uploaded.</p>
        </div>
        {candidates.length > 1 && <span className="item-count">{activeIndex + 1} of {candidates.length}</span>}
      </header>

      <div className="upload-title-duplicate-review__match">
        <span className="match-badge match-badge--possible">Light duplicate match</span>
        <h3>{activeCandidate.title}</h3>
        <p>Matching title{activeCandidate.matchedTitles.length === 1 ? "" : "s"} in {matchScopeLabel}:</p>
        <ul>{activeCandidate.matchedTitles.map((title, index) => <li key={`${title}-${index}`}>{title}</li>)}</ul>
      </div>

      <fieldset className="upload-title-duplicate-review__choices" disabled={busy}>
        <legend>What should happen to this file?</legend>
        <label><input checked={isIgnore} name="upload-title-duplicate-decision" onChange={() => setDecision("ignore")} type="radio" value="ignore" /> <span><strong>Upload anyway</strong> Keep this new file and add it to the upload queue.</span></label>
        <label><input checked={!isIgnore} name="upload-title-duplicate-decision" onChange={() => setDecision("skip")} type="radio" value="skip" /> <span><strong>Skip duplicate</strong> Keep the local source file unchanged and do not upload it.</span></label>
      </fieldset>

      {candidates.length > 1 && <label className="upload-title-duplicate-review__apply"><input checked={applyToAll} disabled={busy} onChange={(event) => setApplyToAll(event.target.checked)} type="checkbox" /> Apply this choice to all {candidates.length} affected items</label>}

      <footer className="upload-title-duplicate-review__actions">
        <button className={applyToAll && !isIgnore ? "secondary-action" : "queue-button"} disabled={busy} onClick={submit} type="button">{submitLabel}</button>
      </footer>
    </section>
  );
}
