import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { prepareRelinkForChannel, saveRelinkForChannel, SourceRelinkDialog, SourceRelinkFileList, verifyRelinkForChannel } from "./SourceRelinkDialog";
import type { SourceRelinkReceipt, SourceRelinkReview } from "../lib/source-relink";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), prepare: vi.fn(), verify: vi.fn(), commit: vi.fn(), cancel: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection }));
vi.mock("../lib/source-relink", () => ({ prepareSourceRelink: mocks.prepare, verifySourceRelink: mocks.verify, commitSourceRelink: mocks.commit, cancelSourceRelink: mocks.cancel, loadSourceRelinkReview: vi.fn() }));
const review: SourceRelinkReview = { reviewId: "review-a", channelId: "channel-a", channelName: "Event channel", state: "prepared", expiresAt: "2099-01-01T00:00:00Z", replacementRoot: "D:/moved", files: [{ itemId: "item-a", title: "<Clip>", fileName: "original.mp4", previousLocation: "D:/old/original.mp4", replacementLocation: "D:/moved/original.mp4", sizeBytes: 3, status: "pending", detail: "Ready for verification" }], verifiedCount: 0, blockedCount: 0 };
const verified: SourceRelinkReview = { ...review, state: "verified", verifiedCount: 1, files: review.files.map((file) => ({ ...file, status: "verified" })) };
const receipt: SourceRelinkReceipt = { reviewId: "review-a", channelId: "channel-a", relinkedItemIds: ["item-a"], resumedItemIds: ["item-a"], attentionItemIds: [], detail: "Saved" };
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
describe("stepped source relocation", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.prepare.mockResolvedValue(review); mocks.verify.mockResolvedValue(verified); mocks.commit.mockResolvedValue(receipt); mocks.cancel.mockResolvedValue(undefined); });
  it("starts in location selection without filesystem verification or commit", () => {
    const markup = renderToStaticMarkup(<SourceRelinkDialog items={[{ id: "item-a", title: "Clip", fileName: "original.mp4" }]} expectedChannelId="channel-a" onSaved={() => undefined} onClose={() => undefined} />);
    for (const text of ['aria-modal="true"', 'aria-label="Locate originals steps"', "Select location", "Verify files", "Review", "Choose original file", "Next: Verify files"]) expect(markup).toContain(text);
    expect(markup).not.toContain("Save repaired locations"); expect(mocks.prepare).not.toHaveBeenCalled(); expect(mocks.commit).not.toHaveBeenCalled();
  });
  it("performs normal prepare, verify and explicit same-ID save in order", async () => {
    expect(await prepareRelinkForChannel(["item-a"], "channel-a", "D:/moved", "file", () => true)).toEqual(review);
    expect(await verifyRelinkForChannel(review, "channel-a", () => true)).toEqual(verified);
    expect(await saveRelinkForChannel(verified, "channel-a", ["item-a"], true, true, () => true)).toEqual(receipt);
    expect(mocks.commit).toHaveBeenCalledWith("review-a", "channel-a", ["item-a"], true, true);
  });
  it("rejects stale-channel preparation before reading replacements", async () => {
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-b" });
    await expect(prepareRelinkForChannel(["item-a"], "channel-a", "D:/moved", "file", () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.prepare).not.toHaveBeenCalled();
  });
  it("releases a late review after close or unmount instead of adopting it", async () => {
    let current = true; const native = deferred<SourceRelinkReview>(); mocks.prepare.mockReturnValue(native.promise);
    const operation = prepareRelinkForChannel(["item-a"], "channel-a", "D:/moved", "file", () => current);
    await Promise.resolve(); current = false; native.resolve(review);
    expect(await operation).toBeUndefined(); expect(mocks.cancel).toHaveBeenCalledWith("review-a", "channel-a");
  });
  it("releases a foreign-channel native review and blocks wrong verification identities", async () => {
    mocks.prepare.mockResolvedValue({ ...review, channelId: "channel-b" });
    await expect(prepareRelinkForChannel(["item-a"], "channel-a", "D:/moved", "file", () => true)).rejects.toThrow("channel changed");
    expect(mocks.cancel).toHaveBeenCalledWith("review-a", "channel-a");
    mocks.verify.mockResolvedValue({ ...verified, files: [{ ...verified.files[0], itemId: "foreign" }] });
    await expect(verifyRelinkForChannel(review, "channel-a", () => true)).rejects.toThrow("did not match");
  });
  it("cannot save unverified, unapproved or unselected replacements", async () => {
    for (const [candidate, ids, approved] of [[review, ["item-a"], true], [verified, ["item-a"], false], [verified, ["foreign"], true]] as const)
      await expect(saveRelinkForChannel(candidate, "channel-a", [...ids], true, approved, () => true)).rejects.toThrow("Review and approve");
    expect(mocks.commit).not.toHaveBeenCalled();
  });
  it("ignores a late committed receipt and rejects mismatched receipt selections", async () => {
    let current = true; const native = deferred<SourceRelinkReceipt>(); mocks.commit.mockReturnValue(native.promise);
    const operation = saveRelinkForChannel(verified, "channel-a", ["item-a"], true, true, () => current);
    await Promise.resolve(); current = false; native.resolve(receipt); expect(await operation).toBeUndefined();
    mocks.commit.mockResolvedValue({ ...receipt, relinkedItemIds: ["foreign"] });
    await expect(saveRelinkForChannel(verified, "channel-a", ["item-a"], true, true, () => true)).rejects.toThrow("did not match");
  });
  it("bounds file verification rendering to ten rows and escapes source text", () => {
    const large = { ...verified, files: Array.from({ length: 25 }, (_, index) => ({ ...verified.files[0], itemId: `item-${index}`, title: `<Clip ${index}>` })) };
    const markup = renderToStaticMarkup(<SourceRelinkFileList review={large} page={2} onPage={() => undefined} />);
    expect(markup).toContain("&lt;Clip 10&gt;"); expect(markup).not.toContain("&lt;Clip 0&gt;"); expect(markup).not.toContain("&lt;Clip 20&gt;"); expect(markup).toContain("Page 2 of 3"); expect(markup).not.toContain("<Clip");
  });
  it("surfaces native verification failures without inventing a verified result", async () => {
    mocks.verify.mockRejectedValue("Different bytes");
    await expect(verifyRelinkForChannel(review, "channel-a", () => true)).rejects.toBe("Different bytes"); expect(mocks.commit).not.toHaveBeenCalled();
  });
});
