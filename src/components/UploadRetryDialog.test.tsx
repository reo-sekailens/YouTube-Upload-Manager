import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadRetryForChannel, retryForChannel, UploadRetryDialog, UploadRetryList } from "./UploadRetryDialog";
import type { UploadRetryReceipt, UploadRetryReview } from "../lib/upload-retry";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), review: vi.fn(), retry: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection }));
vi.mock("../lib/upload-retry", async (original) => ({ ...await original<object>(), loadUploadRetryReview: mocks.review, retryUploadReview: mocks.retry }));
function review(overrides: Partial<UploadRetryReview> = {}): UploadRetryReview {
  return { reviewId: "review-a", channelId: "channel-a", channelName: "Channel A", failureKey: "upload_setup:400:invalidTitle", failureDetail: "YouTube rejected the upload setup request.", expiresAt: new Date(Date.now() + 900_000).toISOString(), eligibleCount: 2, blockedCount: 1, items: [
    { itemId: "item-a", title: "<Video A>", fileName: "a.mp4", eligible: true, detail: "Original available; content will be verified." },
    { itemId: "item-b", title: "Video B", fileName: "b.mp4", eligible: true, detail: "Original available; content will be verified." },
    { itemId: "item-c", title: "Video C", fileName: "c.mp4", eligible: false, detail: "Original unavailable.", blockedReason: "Locate the original before retrying." },
  ], ...overrides };
}
const receipt: UploadRetryReceipt = { reviewId: "review-a", channelId: "channel-a", queuedItemIds: ["item-a"], blockedItems: [{ itemId: "item-b", detail: "The original changed after review." }], detail: "Saved sessions reconcile before transfer. Existing quota pauses still apply." };
describe("stepped upload retry", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.review.mockResolvedValue(review()); mocks.retry.mockResolvedValue(receipt); });
  it("starts at Choose with explicit Review and Results steps", () => {
    const markup = renderToStaticMarkup(<UploadRetryDialog expectedChannelId="channel-a" anchorItemId="item-a" allMatching onRetried={() => undefined} onClose={() => undefined} />);
    expect(markup).toContain('aria-modal="true"'); expect(markup).toContain("Choose uploads to retry"); expect(markup).toContain('aria-label="Upload retry steps"'); expect(markup).toContain("Review"); expect(markup).toContain("Results"); expect(markup).toContain("Checking the saved uploads");
    expect(mocks.review).not.toHaveBeenCalled(); expect(mocks.retry).not.toHaveBeenCalled();
  });
  it("shows eligible choices and blocked originals with accessible labels and escaped titles", () => {
    const markup = renderToStaticMarkup(<UploadRetryList review={review()} selected={new Set(["item-a", "item-b"])} onSelect={() => undefined} page={1} onPage={() => undefined} />);
    expect(markup).toContain("&lt;Video A&gt;"); expect(markup).not.toContain("<Video A>");
    expect(markup).toContain('aria-label="Retry &lt;Video A&gt;"'); expect(markup).toContain("Locate the original before retrying."); expect(markup).toMatch(/aria-label="Retry Video C"[^>]*disabled=""/);
  });
  it("shows partial results on their original records and omits unselected records", () => {
    const markup = renderToStaticMarkup(<UploadRetryList review={review()} selected={new Set(["item-a", "item-b"])} receipt={receipt} page={1} onPage={() => undefined} />);
    expect(markup).toContain("Queued for retry"); expect(markup).toContain("The original changed after review."); expect(markup).not.toContain("Video C");
  });
  it("bounds each page and retains matching selection across the review", () => {
    const rows = Array.from({ length: 25 }, (_, index) => ({ ...review().items[0], itemId: `item-${index}`, title: `Clip ${index}` }));
    const markup = renderToStaticMarkup(<UploadRetryList review={review({ items: rows, eligibleCount: 25, blockedCount: 0 })} selected={new Set(rows.map((item) => item.itemId))} onSelect={() => undefined} page={2} onPage={() => undefined} />);
    expect(markup).toContain("Clip 10"); expect(markup).toContain("Clip 19"); expect(markup).not.toContain("Clip 20"); expect(markup).toContain("Page 2 of 3");
  });
  it("loads a read-only review with channel checks before and after", async () => {
    const expected = review(); mocks.review.mockResolvedValueOnce(expected);
    expect(await loadRetryForChannel("channel-a", "item-a", true, () => true)).toEqual(expected);
    expect(mocks.review).toHaveBeenCalledExactlyOnceWith("channel-a", "item-a", true); expect(mocks.connection).toHaveBeenCalledTimes(2); expect(mocks.retry).not.toHaveBeenCalled();
  });
  it("blocks a changed channel before loading and rejects a changed channel during review", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(loadRetryForChannel("channel-a", "item-a", true, () => true)).rejects.toThrow("connected channel changed"); expect(mocks.review).not.toHaveBeenCalled();
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-a" }).mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(loadRetryForChannel("channel-a", "item-a", true, () => true)).rejects.toThrow("connected channel changed");
  });
  it("discards late results after cancellation", async () => {
    let resolve!: (value: UploadRetryReview) => void;
    mocks.review.mockReturnValue(new Promise<UploadRetryReview>((done) => { resolve = done; }));
    let current = true; const pending = loadRetryForChannel("channel-a", "item-a", true, () => current);
    await vi.waitFor(() => expect(mocks.review).toHaveBeenCalledOnce()); current = false; resolve(review());
    expect(await pending).toBeUndefined(); expect(mocks.connection).toHaveBeenCalledOnce();
  });
  it("rejects foreign, expired, duplicated, incorrect counts and expanded single reviews", async () => {
    for (const result of [review({ channelId: "channel-b" }), review({ expiresAt: "2020-01-01T00:00:00Z" }), review({ eligibleCount: 3 }), review({ items: [review().items[0], review().items[0]] })]) {
      mocks.review.mockResolvedValueOnce(result); await expect(loadRetryForChannel("channel-a", "item-a", true, () => true)).rejects.toThrow("stale");
    }
    await expect(loadRetryForChannel("channel-a", "item-a", false, () => true)).rejects.toThrow("stale");
  });
  it("requires approval and a unique eligible subset before any queue mutation", async () => {
    for (const [ids, approved] of [[[], true], [["item-a"], false], [["item-c"], true], [["item-a", "item-a"], true], [["foreign-item"], true]] as [string[], boolean][]) await expect(retryForChannel(review(), "channel-a", ids, approved, () => true)).rejects.toThrow("Approve");
    await expect(retryForChannel(review({ expiresAt: "2020-01-01T00:00:00Z" }), "channel-a", ["item-a"], true, () => true)).rejects.toThrow("Approve");
    expect(mocks.retry).not.toHaveBeenCalled(); expect(mocks.connection).not.toHaveBeenCalled();
  });
  it("queues only selected IDs and accepts mixed queued/blocked outcomes", async () => {
    expect(await retryForChannel(review(), "channel-a", ["item-a", "item-b"], true, () => true)).toEqual(receipt);
    expect(mocks.retry).toHaveBeenCalledExactlyOnceWith("channel-a", "review-a", ["item-a", "item-b"]);
  });
  it("withholds foreign, omitted or duplicate outcome IDs and stale completion", async () => {
    for (const value of [{ ...receipt, channelId: "channel-b" }, { ...receipt, reviewId: "foreign-review" }, { ...receipt, queuedItemIds: ["item-c"] }, { ...receipt, blockedItems: [] }, { ...receipt, queuedItemIds: ["item-b"] }]) {
      mocks.retry.mockResolvedValueOnce(value); await expect(retryForChannel(review(), "channel-a", ["item-a", "item-b"], true, () => true)).rejects.toThrow("matched");
    }
    let current = true; mocks.retry.mockImplementationOnce(async () => { current = false; return receipt; });
    expect(await retryForChannel(review(), "channel-a", ["item-a", "item-b"], true, () => current)).toBeUndefined();
  });
});
