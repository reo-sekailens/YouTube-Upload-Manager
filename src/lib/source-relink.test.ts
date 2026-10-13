import { beforeEach, describe, expect, it, vi } from "vitest";
import { cancelSourceRelink, commitSourceRelink, loadSourceRelinkReview, prepareSourceRelink, verifySourceRelink } from "./source-relink";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), native: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.native }));
describe("source relocation bridge", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.native.mockReturnValue(true); });
  it("prepares the existing selected IDs on their immutable channel", async () => {
    await prepareSourceRelink(["item-a", "item-b"], "channel-a", "D:/moved", "folder");
    expect(mocks.invoke).toHaveBeenCalledWith("prepare_source_relink", { itemIds: ["item-a", "item-b"], expectedChannelId: "channel-a", path: "D:/moved", mode: "folder" });
  });
  it("keeps verification and bounded progress attached to the same review", async () => {
    await verifySourceRelink("review-a", "channel-a"); await loadSourceRelinkReview("review-a", "channel-a");
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "verify_source_relink", { reviewId: "review-a", expectedChannelId: "channel-a" });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "load_source_relink_review", { reviewId: "review-a", expectedChannelId: "channel-a" });
  });
  it("saves only the explicitly selected verified items with resume and approval choices", async () => {
    await commitSourceRelink("review-a", "channel-a", ["item-a"], false, true);
    expect(mocks.invoke).toHaveBeenCalledWith("commit_source_relink", { reviewId: "review-a", expectedChannelId: "channel-a", itemIds: ["item-a"], resumeUploads: false, approved: true });
    expect(mocks.invoke.mock.calls.some(([command]) => String(command).includes("import"))).toBe(false);
  });
  it("cancels only the owned review without clearing a queue or checkpoint", async () => {
    await cancelSourceRelink("review-a", "channel-a");
    expect(mocks.invoke).toHaveBeenCalledWith("cancel_source_relink", { reviewId: "review-a", expectedChannelId: "channel-a" });
  });
  it("does not pretend to relocate files outside the native app", () => {
    mocks.native.mockReturnValue(false);
    expect(() => prepareSourceRelink(["item-a"], "channel-a", "D:/moved", "file")).toThrow("desktop app");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
