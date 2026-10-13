import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { cancelPerceptualForChannel, loadPerceptualForChannel, PerceptualCandidateCard, PerceptualCandidateList, PerceptualReviewDialog, startPerceptualForChannel, validPerceptualSelection } from "./PerceptualReviewDialog";
import type { PerceptualCandidate, PerceptualReview } from "../lib/perceptual-review";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), start: vi.fn(), load: vi.fn(), cancel: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection }));
vi.mock("../lib/perceptual-review", () => ({ startPerceptualReview: mocks.start, loadPerceptualReview: mocks.load, cancelPerceptualReview: mocks.cancel }));

const candidate: PerceptualCandidate = {
  candidateId: "pair-a", leftItemId: "item-a", rightItemId: "item-b", leftTitle: "<Original footage>", rightTitle: "Re-encoded footage",
  leftDurationSeconds: 123, rightDurationSeconds: 124, durationDifferencePercent: 0.81, alignedSamples: 6, totalSamples: 8,
  medianDistance: 3, maxDistance: 7, evidence: "possible", sourceEvidenceCurrent: true, warnings: ["Shared introductions can look similar."],
  samples: [{ position: 0.25, leftTimeSeconds: 30.75, rightTimeSeconds: 31, distance: 2, informative: true }, { position: 0.5, leftTimeSeconds: 61.5, rightTimeSeconds: 62, distance: 7, informative: false }],
};
const review: PerceptualReview = {
  reviewId: "review-a", channelId: "channel-a", state: "running", algorithmVersion: "local-frame-dhash-v1", decoderAvailable: true, processed: 0, total: 2,
  sources: [{ itemId: "item-a", title: "Original", fileName: "original.mp4", state: "pending", detail: "Waiting for local sampling", sampleCount: 0, informativeSamples: 0 }, { itemId: "item-b", title: "Re-encoded", fileName: "encoded.mp4", state: "pending", detail: "Waiting for local sampling", sampleCount: 0, informativeSamples: 0 }],
  candidates: [], warnings: ["Local samples cannot establish remote content identity."], startedAt: "2026-10-13T00:00:00Z",
};
const complete: PerceptualReview = { ...review, state: "complete", processed: 2, candidates: [candidate], completedAt: "2026-10-13T00:01:00Z", sources: review.sources.map((source) => ({ ...source, state: "analyzed", sampleCount: 8, informativeSamples: 6, durationSeconds: 123, detail: "Original signature verified before and after sampling" })) };
const items = [{ id: "item-a", title: "Original", fileName: "original.mp4" }, { id: "item-b", title: "Re-encoded", fileName: "encoded.mp4" }];
function deferred<T>() { let resolve!: (value: T) => void; return { promise: new Promise<T>((done) => { resolve = done; }), resolve: (value: T) => resolve(value) }; }

describe("stepped local footage review", () => {
  beforeEach(() => {
    vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" });
    mocks.start.mockResolvedValue(review); mocks.load.mockResolvedValue(complete); mocks.cancel.mockResolvedValue({ ...review, state: "cancelled" });
  });

  it("opens in opt-in selection and starts no analysis or provider read", () => {
    const markup = renderToStaticMarkup(<PerceptualReviewDialog items={items} expectedChannelId="channel-a" channelName="Event channel" onClose={() => undefined} />);
    for (const text of ['aria-modal="true"', 'aria-label="Local footage review steps"', "Select originals", "Analyze", "Review candidates", "Event channel", "1–16", "Next: Analyze", "No YouTube video is downloaded"]) expect(markup).toContain(text);
    expect(markup).not.toContain("Start local analysis"); expect(mocks.start).not.toHaveBeenCalled(); expect(mocks.connection).not.toHaveBeenCalled();
  });

  it("shows a useful empty state without dead search or selection controls", () => {
    const markup = renderToStaticMarkup(<PerceptualReviewDialog items={[]} expectedChannelId="channel-a" onClose={() => undefined} />);
    expect(markup).toContain("No saved original videos"); expect(markup).toContain("normal upload review"); expect(markup).not.toContain("Find a saved original");
  });

  it("accepts one original for retained fingerprint comparison and bounds distinct selections", () => {
    expect(validPerceptualSelection(["item-a"])).toBe(true); expect(validPerceptualSelection(Array.from({ length: 16 }, (_, index) => `item-${index}`))).toBe(true);
    for (const ids of [[], [""], [" "], ["item-a", "item-a"], Array.from({ length: 17 }, (_, index) => `item-${index}`)]) expect(validPerceptualSelection(ids)).toBe(false);
  });

  it("starts normal explicitly approved same-channel analysis without changing upload state", async () => {
    expect(await startPerceptualForChannel("channel-a", ["item-a", "item-b"], true, () => true)).toEqual(review);
    expect(mocks.start).toHaveBeenCalledExactlyOnceWith("channel-a", ["item-a", "item-b"], true); expect(mocks.connection).toHaveBeenCalledTimes(2); expect(mocks.cancel).not.toHaveBeenCalled();
  });

  it("rejects unapproved or duplicated IDs before native work begins", async () => {
    await expect(startPerceptualForChannel("channel-a", ["item-a"], false, () => true)).rejects.toThrow("approve");
    await expect(startPerceptualForChannel("channel-a", ["item-a", "item-a"], true, () => true)).rejects.toThrow("1–16");
    expect(mocks.start).not.toHaveBeenCalled(); expect(mocks.connection).not.toHaveBeenCalled();
  });

  it("rejects a changed channel before starting filesystem analysis", async () => {
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-b" });
    await expect(startPerceptualForChannel("channel-a", ["item-a", "item-b"], true, () => true)).rejects.toThrow("connected channel changed"); expect(mocks.start).not.toHaveBeenCalled();
  });

  it("cancels a late-created analysis after the modal closes", async () => {
    const native = deferred<PerceptualReview>(); mocks.start.mockReturnValue(native.promise); let current = true;
    const operation = startPerceptualForChannel("channel-a", ["item-a", "item-b"], true, () => current);
    await vi.waitFor(() => expect(mocks.start).toHaveBeenCalledOnce()); current = false; native.resolve(review);
    expect(await operation).toBeUndefined(); expect(mocks.cancel).toHaveBeenCalledExactlyOnceWith("channel-a", "review-a");
  });

  it("cancels created analysis if the connection changes while native work is starting", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-a" }).mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(startPerceptualForChannel("channel-a", ["item-a", "item-b"], true, () => true)).rejects.toThrow("connected channel changed"); expect(mocks.cancel).toHaveBeenCalledOnce();
  });

  it("rejects a result with foreign selected originals or unrelated candidates", async () => {
    for (const result of [{ ...review, channelId: "channel-b" }, { ...review, sources: [review.sources[0], { ...review.sources[1], itemId: "foreign" }] }, { ...complete, candidates: [{ ...candidate, leftItemId: "foreign-a", rightItemId: "foreign-b" }] }, { ...review, processed: 3 }]) {
      mocks.start.mockResolvedValueOnce(result);
      await expect(startPerceptualForChannel("channel-a", ["item-a", "item-b"], true, () => true)).rejects.toThrow("no longer matches");
    }
    expect(mocks.cancel).toHaveBeenCalledTimes(4);
  });

  it("loads saved results for the exact review and preserves retained same-channel candidates", async () => {
    const retained = { ...complete, candidates: [{ ...candidate, rightItemId: "retained-item", sourceEvidenceCurrent: false, retainedEvidenceAsOf: "2026-10-12T00:00:00Z" }] };
    mocks.load.mockResolvedValue(retained);
    expect(await loadPerceptualForChannel("channel-a", "review-a", ["item-a", "item-b"], () => true)).toEqual(retained); expect(mocks.load).toHaveBeenCalledExactlyOnceWith("channel-a", "review-a");
    mocks.load.mockResolvedValueOnce({ ...retained, reviewId: "other-review" });
    await expect(loadPerceptualForChannel("channel-a", "review-a", ["item-a", "item-b"], () => true)).rejects.toThrow("no longer matches");
  });

  it("suppresses a late loaded result and exposes native failures without fabricating candidates", async () => {
    const native = deferred<PerceptualReview>(); mocks.load.mockReturnValue(native.promise); let current = true;
    const operation = loadPerceptualForChannel("channel-a", "review-a", ["item-a", "item-b"], () => current);
    await vi.waitFor(() => expect(mocks.load).toHaveBeenCalledOnce()); current = false; native.resolve(complete); expect(await operation).toBeUndefined();
    mocks.load.mockRejectedValueOnce("Original changed during sampling"); await expect(loadPerceptualForChannel("channel-a", "review-a", ["item-a", "item-b"], () => true)).rejects.toBe("Original changed during sampling");
  });

  it("cancels scoped native work without requiring the old channel to remain connected", async () => {
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-b" });
    expect(await cancelPerceptualForChannel("channel-a", "review-a", ["item-a", "item-b"], () => true)).toEqual({ ...review, state: "cancelled" });
    expect(mocks.cancel).toHaveBeenCalledExactlyOnceWith("channel-a", "review-a"); expect(mocks.connection).not.toHaveBeenCalled();
  });

  it("explains sampled alignment, duration and distance without probability or byte-hash claims", () => {
    const markup = renderToStaticMarkup(<PerceptualCandidateCard candidate={candidate} algorithmVersion="local-frame-dhash-v1" onCompare={() => undefined} />);
    for (const text of ["Possible content match", "2:03 / 2:04", "0.81% difference", "6 of 8", "Median 3", "maximum 7", "not a probability", "Both originals verified", "full byte-hash match", "remote content", "Alignment and sample evidence", "25%", "Limited scene", "Shared introductions", "Compare originals", "local-frame-dhash-v1"]) expect(markup).toContain(text);
    expect(markup).toContain("&lt;Original footage&gt;"); expect(markup).not.toContain("<Original footage>"); expect(markup).not.toContain("Delete"); expect(markup).not.toContain("Suppress");
  });

  it("clearly distinguishes retained fingerprints and limited scenes from currently verified originals", () => {
    const markup = renderToStaticMarkup(<PerceptualCandidateCard candidate={{ ...candidate, evidence: "limited", sourceEvidenceCurrent: false, retainedEvidenceAsOf: "2026-10-12T00:00:00Z" }} algorithmVersion="v1" />);
    expect(markup).toContain("Limited evidence"); expect(markup).toContain("current original not verified"); expect(markup).toContain("Fingerprint recorded 2026-10-12"); expect(markup).not.toContain("Both originals verified");
  });

  it("bounds candidate and alignment rendering and retains truthful empty results", () => {
    const large = { ...complete, candidates: Array.from({ length: 25 }, (_, index) => ({ ...candidate, candidateId: `pair-${index}`, leftTitle: `Candidate ${index}`, samples: Array.from({ length: 32 }, (_, sample) => ({ ...candidate.samples[0], position: sample / 100 })) })) };
    const markup = renderToStaticMarkup(<PerceptualCandidateList review={large} page={1} onPage={() => undefined} />);
    expect(markup).toContain("Candidate 10"); expect(markup).toContain("Candidate 19"); expect(markup).not.toContain("Candidate 0<"); expect(markup).not.toContain("Candidate 20"); expect(markup).toContain("Page 2 of 3"); expect(markup).not.toContain("31%");
    const empty = renderToStaticMarkup(<PerceptualCandidateList review={review} page={0} onPage={() => undefined} />);
    expect(empty).toContain("No candidate matches were found"); expect(empty).toContain("cannot prove"); expect(empty).not.toContain("Candidate pages");
  });
});
