import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyUploadPlanAssets, assetBindings, assetReviewCurrent, assetWorkspaceMatches, defaultAssetMatches, discardVideoAssetReview, loadUploadPlanAssetWorkspace, loadVideoAssetWorkspace, prepareUploadPlanAssets, prepareVideoAssets, repairVideoAssetJobs, saveVideoAssetJobs, type VideoAssetReview } from "./video-assets";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), desktop: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.desktop }));
export const review: VideoAssetReview = { reviewId: "review-a", channelId: "channel-a", expiresAt: new Date(Date.now() + 900_000).toISOString(), limitsCheckedAt: "2026-10-13", items: [{ itemId: "item-a", title: "Video A", fileName: "camera.mp4", videoId: "video-a", status: "uploaded" }, { itemId: "item-b", title: "Video B", fileName: "camera.mp4", status: "queued" }], assets: [{ assetId: "image-a", fileName: "camera.png", kind: "thumbnail", sizeBytes: 1000, valid: true, dimensions: "1280 × 720", suggestedItemIds: ["item-a", "item-b"], warnings: [] }, { assetId: "caption-a", fileName: "camera.srt", kind: "caption", sizeBytes: 100, valid: true, cueCount: 2, suggestedItemIds: ["item-a"], warnings: [] }] };
describe("native video attachment bridge", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.desktop.mockReturnValue(true); });
  it("carries channel, item selection, original paths and final declarations through native commands", async () => {
    await loadVideoAssetWorkspace("channel-a", ["item-a"]); await prepareVideoAssets("channel-a", ["item-a"], ["D:/original/camera.srt"], true);
    const bindings = [{ assetId: "caption-a", itemId: "item-a", language: "en", name: "English", draft: false }]; await saveVideoAssetJobs("channel-a", "review-a", bindings); await repairVideoAssetJobs("channel-a", ["job-a"]); await discardVideoAssetReview("channel-a", "review-a");
    expect(mocks.invoke.mock.calls).toEqual([["load_video_asset_workspace", { expectedChannelId: "channel-a", itemIds: ["item-a"] }], ["prepare_video_assets", { expectedChannelId: "channel-a", itemIds: ["item-a"], paths: ["D:/original/camera.srt"], autoMatch: true }], ["save_video_asset_jobs", { expectedChannelId: "channel-a", reviewId: "review-a", bindings }], ["repair_video_asset_jobs", { expectedChannelId: "channel-a", jobIds: ["job-a"] }], ["discard_video_asset_review", { expectedChannelId: "channel-a", reviewId: "review-a" }]]);
  });
  it("never auto-selects an ambiguous basename or supplies a caption draft declaration", () => {
    const matches = defaultAssetMatches(review); expect(matches["image-a"].itemIds).toEqual([]); expect(matches["caption-a"].itemIds).toEqual(["item-a"]); expect(matches["caption-a"].draft).toBeUndefined();
    expect(() => assetBindings(review, matches)).toThrow("language, name and draft");
    matches["caption-a"] = { itemIds: ["item-a"], language: "en", name: "English", draft: false }; matches["image-a"].itemIds = ["item-a", "item-b"];
    expect(assetBindings(review, matches)).toEqual([{ assetId: "image-a", itemId: "item-a" }, { assetId: "image-a", itemId: "item-b" }, { assetId: "caption-a", itemId: "item-a", language: "en", name: "English", draft: false }]);
  });
  it("reviews prospective attachments by immutable plan ID without sending a saved-job command", async () => {
    await loadUploadPlanAssetWorkspace("channel-a", "plan-a");
    await prepareUploadPlanAssets("channel-a", "plan-a", ["D:/original/camera.srt"], false);
    const bindings = [{ assetId: "caption-a", itemId: "prospective-0", language: "en", name: "English", draft: false }];
    await applyUploadPlanAssets("channel-a", "plan-a", "review-a", bindings);
    expect(mocks.invoke.mock.calls).toEqual([["load_upload_plan_asset_workspace", { expectedChannelId: "channel-a", planId: "plan-a" }], ["prepare_upload_plan_assets", { expectedChannelId: "channel-a", planId: "plan-a", paths: ["D:/original/camera.srt"], autoMatch: false }], ["apply_upload_plan_assets", { expectedChannelId: "channel-a", planId: "plan-a", reviewId: "review-a", bindings }]]);
  });
  it("rejects stale approvals, foreign targets, invalid assets and duplicate declarations", () => {
    const matches = { "image-a": { itemIds: ["item-a"], language: "", name: "" } };
    expect(assetReviewCurrent({ ...review, expiresAt: "invalid" })).toBe(false);
    expect(() => assetBindings({ ...review, expiresAt: "2020-01-01T00:00:00Z" }, matches)).toThrow("expired");
    expect(() => assetBindings(review, { "image-a": { ...matches["image-a"], itemIds: ["foreign"] } })).toThrow("reviewed videos");
    expect(() => assetBindings({ ...review, assets: [{ ...review.assets[0], valid: false }] }, matches)).toThrow("invalid attachment");
    const duplicate = { ...review, assets: [review.assets[0], { ...review.assets[0], assetId: "image-b" }] };
    expect(() => assetBindings(duplicate, { ...matches, "image-b": matches["image-a"] })).toThrow("one thumbnail");
  });
  it("requires explicit caption metadata and rejects foreign workspace receipts", () => {
    for (const declaration of [{ language: "", name: "English", draft: true }, { language: "en", name: "", draft: false }, { language: "en", name: "English", draft: undefined }, { language: "en", name: "\u0000secret", draft: true }]) expect(() => assetBindings(review, { "caption-a": { itemIds: ["item-a"], ...declaration } })).toThrow();
    expect(assetWorkspaceMatches({ channelId: "channel-a", items: [review.items[0]], jobs: [] }, "channel-a", ["item-a"])).toBe(true);
    expect(assetWorkspaceMatches({ channelId: "channel-b", items: [review.items[0]], jobs: [] }, "channel-a", ["item-a"])).toBe(false);
  });
  it("rejects privileged asset work from an ordinary browser", async () => {
    mocks.desktop.mockReturnValue(false); await expect(loadVideoAssetWorkspace("channel-a", ["item-a"])).rejects.toThrow("desktop app"); await expect(prepareVideoAssets("channel-a", ["item-a"], ["/asset.png"], false)).rejects.toThrow("desktop app"); await expect(saveVideoAssetJobs("channel-a", "review-a", [])).rejects.toThrow("desktop app"); await expect(repairVideoAssetJobs("channel-a", ["job-a"])).rejects.toThrow("desktop app"); await discardVideoAssetReview("channel-a", "review-a"); expect(mocks.invoke).not.toHaveBeenCalled();
    await expect(loadUploadPlanAssetWorkspace("channel-a", "plan-a")).rejects.toThrow("desktop app"); await expect(prepareUploadPlanAssets("channel-a", "plan-a", ["/asset.png"], false)).rejects.toThrow("desktop app"); await expect(applyUploadPlanAssets("channel-a", "plan-a", "review-a", [])).rejects.toThrow("desktop app"); expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
