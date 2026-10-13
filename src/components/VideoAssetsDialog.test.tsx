import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VideoAssetBindingReview, VideoAssetsDialog, applyPlanAssetsForChannel, loadAssetsForChannel, loadPlanAssetsForChannel, prepareAssetsForChannel, preparePlanAssetsForChannel, saveAssetsForChannel } from "./VideoAssetsDialog";
import type { VideoAssetReview } from "../lib/video-assets";
import type { UploadPlanPage } from "../lib/upload-plan";
const mocks = vi.hoisted(() => ({ connection: vi.fn(), load: vi.fn(), prepare: vi.fn(), save: vi.fn(), discard: vi.fn(), loadPlan: vi.fn(), preparePlan: vi.fn(), applyPlan: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection, beginDeletionAuthorization: vi.fn(), enableDeletionSudoMode: vi.fn() }));
vi.mock("../lib/video-assets", async (original) => ({ ...await original<object>(), loadVideoAssetWorkspace: mocks.load, prepareVideoAssets: mocks.prepare, saveVideoAssetJobs: mocks.save, discardVideoAssetReview: mocks.discard, loadUploadPlanAssetWorkspace: mocks.loadPlan, prepareUploadPlanAssets: mocks.preparePlan, applyUploadPlanAssets: mocks.applyPlan }));
const review: VideoAssetReview = { reviewId: "review-a", channelId: "channel-a", expiresAt: new Date(Date.now() + 900_000).toISOString(), limitsCheckedAt: "2026-10-13", items: [{ itemId: "item-a", title: "<Saved video>", fileName: "video.mp4", videoId: "video-a", status: "uploaded" }], assets: [{ assetId: "caption-a", fileName: "video.srt", kind: "caption", sizeBytes: 200, valid: true, cueCount: 2, suggestedItemIds: ["item-a"], warnings: [] }] };
const matches = { "caption-a": { itemIds: ["item-a"], language: "en", name: "English", draft: false } };
describe("stepped video asset approval", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.prepare.mockResolvedValue(review); mocks.load.mockResolvedValue({ channelId: "channel-a", items: review.items, jobs: [] }); mocks.save.mockResolvedValue({ channelId: "channel-a", jobIds: ["job-a"] }); mocks.discard.mockResolvedValue(undefined); });
  it("starts at Choose assets and creates no jobs or implicit permissions", () => { const markup = renderToStaticMarkup(<VideoAssetsDialog expectedChannelId="channel-a" itemIds={["item-a"]} onClose={() => undefined} />); expect(markup).toContain('aria-label="Video attachment steps"'); expect(markup).toContain("Choose assets"); expect(markup).toContain("Match to videos"); expect(markup).toContain("Review"); expect(markup).toContain("confirmed upload receipts stay intact"); expect(mocks.prepare).not.toHaveBeenCalled(); expect(mocks.save).not.toHaveBeenCalled(); expect(mocks.connection).not.toHaveBeenCalled(); });
  it("reviews exact caption choices and immutable video identity without HTML execution", () => { const markup = renderToStaticMarkup(<VideoAssetBindingReview review={review} bindings={[{ assetId: "caption-a", itemId: "item-a", language: "en", name: "English", draft: false }]} />); for (const text of ["video-a", "video.srt", "2 timed cues", "en", "English", "Visible caption", "never replaced or deleted automatically", "&lt;Saved video&gt;"]) expect(markup).toContain(text); expect(markup).not.toContain("<Saved video>"); });
  it("loads and saves only the reviewed channel and target set", async () => { expect(await loadAssetsForChannel("channel-a", ["item-a"], () => true)).toMatchObject({ channelId: "channel-a" }); await expect(saveAssetsForChannel("channel-a", review, matches, false, () => true)).rejects.toThrow("approve"); expect(mocks.save).not.toHaveBeenCalled(); expect(await saveAssetsForChannel("channel-a", review, matches, true, () => true)).toEqual({ channelId: "channel-a", jobIds: ["job-a"] }); expect(mocks.save).toHaveBeenCalledExactlyOnceWith("channel-a", "review-a", [{ assetId: "caption-a", itemId: "item-a", language: "en", name: "English", draft: false }]); });
  it("never starts original checks when the connection changed", async () => { mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" }); await expect(prepareAssetsForChannel("channel-a", ["item-a"], ["/private/video.srt"], true, () => true)).rejects.toThrow("connected channel changed"); expect(mocks.prepare).not.toHaveBeenCalled(); });
  it("revokes a late native review after cancellation and rejects foreign receipts", async () => { let resolve!: (value: VideoAssetReview) => void; mocks.prepare.mockReturnValueOnce(new Promise<VideoAssetReview>((done) => { resolve = done; })); let current = true; const request = prepareAssetsForChannel("channel-a", ["item-a"], ["/video.srt"], false, () => current); await vi.waitFor(() => expect(mocks.prepare).toHaveBeenCalledOnce()); current = false; resolve(review); expect(await request).toBeUndefined(); expect(mocks.discard).toHaveBeenCalledWith("channel-a", "review-a"); mocks.save.mockResolvedValueOnce({ channelId: "channel-b", jobIds: ["job-a"] }); await expect(saveAssetsForChannel("channel-a", review, matches, true, () => true)).rejects.toThrow("receipt did not match"); });
});

describe("prospective batch-plan attachments", () => {
  const prospective: VideoAssetReview = { ...review, reviewId: "plan-review-a", items: [{ itemId: "prospective-a", title: "<Planned video>", fileName: "video.mp4", status: "planned" }], assets: [{ ...review.assets[0], suggestedItemIds: ["prospective-a"] }] };
  const choices = { "caption-a": { ...matches["caption-a"], itemIds: ["prospective-a"] } };
  const sealed: UploadPlanPage = { id: "sealed-plan-a", channelId: "channel-a", channelName: "Channel A", createdAt: "2026-10-13", expiresAt: review.expiresAt, evidenceMode: "cached", totalFiles: 1, readyFiles: 1, duplicateFiles: 0, blockedFiles: 0, offset: 0, limit: 25, files: [], warnings: [] };
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.loadPlan.mockResolvedValue({ channelId: "channel-a", items: prospective.items, jobs: [] }); mocks.preparePlan.mockResolvedValue(prospective); mocks.applyPlan.mockResolvedValue(sealed); mocks.discard.mockResolvedValue(undefined); });
  it("starts a separate stepped plan review without loading saved uploads or creating jobs", () => {
    const markup = renderToStaticMarkup(<VideoAssetsDialog expectedChannelId="channel-a" prospectivePlanId="plan-a" onPlanApplied={() => undefined} onClose={() => undefined} />);
    expect(markup).toContain("These attachments belong to this batch plan"); expect(markup).toContain("Loading planned videos");
    for (const label of ["Choose assets", "Match to videos", "Review"]) expect(markup).toContain(label);
    expect(mocks.load).not.toHaveBeenCalled(); expect(mocks.save).not.toHaveBeenCalled(); expect(mocks.connection).not.toHaveBeenCalled();
  });
  it("resolves opaque prospective targets and returns a new sealed plan after explicit approval", async () => {
    const workspace = await loadPlanAssetsForChannel("channel-a", "plan-a", () => true); expect(workspace?.items).toEqual(prospective.items);
    expect(await preparePlanAssetsForChannel("channel-a", "plan-a", ["prospective-a"], ["/video.srt"], true, () => true)).toEqual(prospective);
    await expect(applyPlanAssetsForChannel("channel-a", "plan-a", prospective, choices, false, () => true)).rejects.toThrow("approve"); expect(mocks.applyPlan).not.toHaveBeenCalled();
    expect(await applyPlanAssetsForChannel("channel-a", "plan-a", prospective, choices, true, () => true)).toEqual(sealed);
    expect(mocks.applyPlan).toHaveBeenCalledExactlyOnceWith("channel-a", "plan-a", "plan-review-a", [{ assetId: "caption-a", itemId: "prospective-a", language: "en", name: "English", draft: false }]);
    expect(mocks.load).not.toHaveBeenCalled(); expect(mocks.prepare).not.toHaveBeenCalled(); expect(mocks.save).not.toHaveBeenCalled();
  });
  it("distinguishes planned originals from saved upload receipts in final review", () => {
    const markup = renderToStaticMarkup(<VideoAssetBindingReview prospective review={prospective} bindings={[{ assetId: "caption-a", itemId: "prospective-a", language: "en", name: "English", draft: false }]} />);
    expect(markup).toContain("Planned original: video.mp4"); expect(markup).toContain("&lt;Planned video&gt;"); expect(markup).not.toContain("Saved upload:"); expect(markup).not.toContain("YouTube video:");
  });
  it("rejects a saved receipt, repeated identity or foreign workspace in prospective mode", async () => {
    for (const workspace of [{ channelId: "channel-b", items: prospective.items, jobs: [] }, { channelId: "channel-a", items: review.items, jobs: [] }, { channelId: "channel-a", items: [prospective.items[0], prospective.items[0]], jobs: [] }, { channelId: "channel-a", items: prospective.items, jobs: [{ jobId: "existing" }] }]) {
      mocks.loadPlan.mockResolvedValueOnce(workspace); await expect(loadPlanAssetsForChannel("channel-a", "plan-a", () => true)).rejects.toThrow("prospective attachment workspace");
    }
  });
  it("revokes expired or different plan-target reviews before approval", async () => {
    for (const invalid of [{ ...prospective, expiresAt: "2000-01-01T00:00:00Z" }, { ...prospective, items: [{ ...prospective.items[0], itemId: "other-plan-target" }] }, { ...prospective, items: review.items }]) {
      mocks.preparePlan.mockResolvedValueOnce(invalid); await expect(preparePlanAssetsForChannel("channel-a", "plan-a", ["prospective-a"], ["/video.srt"], false, () => true)).rejects.toThrow("prospective asset review");
    }
    expect(mocks.discard).toHaveBeenCalledTimes(3); expect(mocks.applyPlan).not.toHaveBeenCalled();
  });
  it("blocks native work after channel changes and ignores cancelled preparation", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" }); await expect(preparePlanAssetsForChannel("channel-a", "plan-a", ["prospective-a"], ["/video.srt"], false, () => true)).rejects.toThrow("connected channel changed"); expect(mocks.preparePlan).not.toHaveBeenCalled();
    let resolve!: (value: VideoAssetReview) => void; let current = true; mocks.preparePlan.mockReturnValueOnce(new Promise<VideoAssetReview>((done) => { resolve = done; }));
    const request = preparePlanAssetsForChannel("channel-a", "plan-a", ["prospective-a"], ["/video.srt"], false, () => current); await vi.waitFor(() => expect(mocks.preparePlan).toHaveBeenCalledOnce()); current = false; resolve(prospective);
    expect(await request).toBeUndefined(); expect(mocks.discard).toHaveBeenCalledWith("channel-a", "plan-review-a");
  });
  it("requires a fresh channel-bound sealed receipt covering the entire reviewed batch", async () => {
    for (const invalid of [{ ...sealed, id: "plan-a" }, { ...sealed, channelId: "channel-b" }, { ...sealed, totalFiles: 2 }, { ...sealed, expiresAt: "2000-01-01T00:00:00Z" }]) {
      mocks.applyPlan.mockResolvedValueOnce(invalid); await expect(applyPlanAssetsForChannel("channel-a", "plan-a", prospective, choices, true, () => true)).rejects.toThrow("plan receipt");
    }
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-a" }).mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" }); await expect(applyPlanAssetsForChannel("channel-a", "plan-a", prospective, choices, true, () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.save).not.toHaveBeenCalled();
  });
});
