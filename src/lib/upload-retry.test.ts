import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadUploadRetryReview, retryUploadReview, uploadRetryIssueKey } from "./upload-retry";
import type { UploadItem } from "./types";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), native: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.native }));
function item(overrides: Partial<UploadItem> & { retryFailureKey?: string } = {}): UploadItem & { retryFailureKey?: string } {
  return { id: "item-a", title: "Video A", fileName: "a.mp4", sizeBytes: 100, status: "failed", confirmedBytes: 0, totalBytes: 100, visibility: "private", madeForKids: false, updatedAt: "2026-10-13T00:00:00Z", ...overrides };
}
describe("native upload retry bridge", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.native.mockReturnValue(true); });
  it("loads a channel-scoped native review for the exact anchor and matching choice", async () => {
    await loadUploadRetryReview("channel-a", "item-a", true);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("load_upload_retry_review", { expectedChannelId: "channel-a", anchorItemId: "item-a", allMatching: true });
  });
  it("retries only approved existing IDs without paths, metadata or a new import", async () => {
    await retryUploadReview("channel-a", "review-a", ["item-a", "item-b"]);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("retry_upload_review", { expectedChannelId: "channel-a", reviewId: "review-a", itemIds: ["item-a", "item-b"] });
  });
  it("requires a real desktop runtime", () => {
    mocks.native.mockReturnValue(false);
    expect(() => loadUploadRetryReview("channel-a", "item-a", false)).toThrow("desktop app");
    expect(() => retryUploadReview("channel-a", "review-a", ["item-a"])).toThrow("desktop app");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
  it("groups structured matching failures and only the exact zero-byte legacy setup rejection", () => {
    expect(uploadRetryIssueKey(item({ retryFailureKey: "upload_setup:400:invalidTitle" }))).toBe("upload_setup:400:invalidTitle");
    expect(uploadRetryIssueKey(item({ detail: "YouTube rejected the upload setup request." }))).toBe("setup:legacy_unknown");
    expect(uploadRetryIssueKey(item({ status: "needs_reconciliation", detail: "YouTube rejected the upload setup request." }))).toBe("setup:legacy_unknown");
    expect(uploadRetryIssueKey(item({ status: "needs_reconciliation", retryFailureKey: "setup:400:invalidTitle" }))).toBe("setup:400:invalidTitle");
    for (const value of [item({ status: "needs_reconciliation", retryFailureKey: "transfer:500:unknown" }), item({ status: "needs_reconciliation", retryFailureKey: "setup:400:invalidTitle", confirmedBytes: 1 })]) expect(uploadRetryIssueKey(value)).toBeUndefined();
    for (const value of [item({ detail: "Another failure" }), item({ detail: "YouTube rejected the upload setup request.", confirmedBytes: 1 }), item({ detail: "YouTube rejected the upload setup request.", videoId: "video-a" }), item({ status: "uploading", retryFailureKey: "upload_setup:400:invalidTitle" })]) expect(uploadRetryIssueKey(value)).toBeUndefined();
  });
});
