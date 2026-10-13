import { beforeEach, describe, expect, it, vi } from "vitest";
import { cancelPerceptualReview, loadPerceptualReview, startPerceptualReview } from "./perceptual-review";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), native: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.native }));
describe("local footage review bridge", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.native.mockReturnValue(true); });
  it("requires explicit opt-in and scopes existing IDs to the connected channel", async () => {
    await startPerceptualReview("channel-a", ["item-a", "item-b"], true);
    expect(mocks.invoke).toHaveBeenCalledWith("start_perceptual_review", { expectedChannelId: "channel-a", itemIds: ["item-a", "item-b"], approved: true });
  });
  it("loads and cancels only the saved review without mutating the upload", async () => {
    await loadPerceptualReview("channel-a", "review-a"); await cancelPerceptualReview("channel-a", "review-a");
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "load_perceptual_review", { expectedChannelId: "channel-a", reviewId: "review-a" });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "cancel_perceptual_review", { expectedChannelId: "channel-a", reviewId: "review-a" });
    expect(mocks.invoke.mock.calls.some(([command]) => /delete|import|queue|upload/.test(String(command)))).toBe(false);
  });
  it("does not pretend browser state is local evidence", () => {
    mocks.native.mockReturnValue(false);
    expect(() => startPerceptualReview("channel-a", ["item-a"], true)).toThrow("desktop app");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
