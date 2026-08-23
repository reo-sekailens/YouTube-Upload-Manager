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
    <section aria-labelledby="upload-title-duplicate-heading" className="mt-4 rounded-xl border border-[#dce1e8] bg-white p-5" role="region">
      <header className="mb-4 flex items-start justify-between gap-4 border-b border-[#e7eaf0] pb-4">
        <div>
          <p className="mb-2 text-[0.67rem] font-bold tracking-[0.1em] text-[#68748a] uppercase">UPLOAD CHECK REQUIRED</p>
          <h2 className="m-0 text-[1.15rem] font-bold tracking-[-0.035em] text-[#172033]" id="upload-title-duplicate-heading">A possible duplicate needs review</h2>
          <p className="mt-2 mb-0 max-w-3xl text-[0.79rem] leading-relaxed text-[#64758a]">Native light dedupe found a normalized filename/title match before upload. This file has not been uploaded.</p>
        </div>
        {candidates.length > 1 && <span className="shrink-0 rounded-full bg-[#edf3ff] px-2 py-1 text-[0.7rem] font-bold text-[#2b63bc]">{activeIndex + 1} of {candidates.length}</span>}
      </header>

      <div className="rounded-lg border border-[#f1dc9b] bg-[#fff9eb] px-4 py-3.5">
        <span className="inline-flex rounded-full bg-[#fff2dd] px-2 py-1 text-[0.68rem] font-bold text-[#885a14]">Light duplicate match</span>
        <h3 className="my-2 wrap-anywhere text-[0.98rem] text-[#3d3420]">{activeCandidate.title}</h3>
        <p className="mt-1 mb-0 text-[0.78rem] leading-relaxed text-[#6c5a27]">Matching title{activeCandidate.matchedTitles.length === 1 ? "" : "s"} in {matchScopeLabel}:</p>
        <ul className="mt-1 mb-0 pl-5 text-[0.78rem] leading-relaxed text-[#6c5a27]">{activeCandidate.matchedTitles.map((title, index) => <li key={`${title}-${index}`}>{title}</li>)}</ul>
      </div>

      <fieldset className="mt-4 mb-3 grid gap-2.5 border-0 p-0" disabled={busy}>
        <legend className="mb-2 text-[0.8rem] font-bold text-[#34405a]">What should happen to this file?</legend>
        <label className="flex items-start gap-2.5 rounded-lg border border-[#e1e6ee] bg-[#fafbfc] px-3 py-2.5 text-[0.78rem] leading-snug text-[#4d5b72]"><input className="mt-0.5 accent-[#2463df]" checked={isIgnore} name="upload-title-duplicate-decision" onChange={() => setDecision("ignore")} type="radio" value="ignore" /> <span><strong className="mb-0.5 block text-[#27344a]">Upload anyway</strong> Keep this new file and add it to the upload queue.</span></label>
        <label className="flex items-start gap-2.5 rounded-lg border border-[#e1e6ee] bg-[#fafbfc] px-3 py-2.5 text-[0.78rem] leading-snug text-[#4d5b72]"><input className="mt-0.5 accent-[#2463df]" checked={!isIgnore} name="upload-title-duplicate-decision" onChange={() => setDecision("skip")} type="radio" value="skip" /> <span><strong className="mb-0.5 block text-[#27344a]">Skip duplicate</strong> Keep the local source file unchanged and do not upload it.</span></label>
      </fieldset>

      {candidates.length > 1 && <label className="flex items-start gap-2.5 rounded-lg border border-[#d7e4ff] bg-[#f0f5ff] px-3 py-2.5 text-[0.78rem] leading-snug text-[#4d5b72]"><input className="mt-0.5 accent-[#2463df]" checked={applyToAll} disabled={busy} onChange={(event) => setApplyToAll(event.target.checked)} type="checkbox" /> Apply this choice to all {candidates.length} affected items</label>}

      <footer className="mt-4 flex flex-wrap justify-end gap-2 max-sm:flex-col-reverse">
        <button className={applyToAll && !isIgnore ? "rounded-md border border-[#cbd3df] bg-white px-3 py-2 text-[0.79rem] font-[680] text-[#34405a] hover:bg-[#f0f3f7] disabled:cursor-not-allowed disabled:opacity-50 max-sm:w-full" : "rounded-md border border-[#2463df] bg-[#2463df] px-3 py-2 text-[0.79rem] font-[680] text-white hover:border-[#1b54c6] hover:bg-[#1b54c6] disabled:cursor-not-allowed disabled:opacity-50 max-sm:w-full"} disabled={busy} onClick={submit} type="button">{submitLabel}</button>
      </footer>
    </section>
  );
}
