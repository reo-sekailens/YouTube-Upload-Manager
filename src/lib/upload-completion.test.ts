import { beforeEach, describe, expect, it, vi } from "vitest";
import { checkUploadCompletion, exportUploadCompletion, loadUploadCompletion, loadUploadCompletionBatches, repairUploadCompletion } from "./upload-completion";
const mocked = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocked.invoke }));
describe("completion commands", () => {
  beforeEach(() => vi.resetAllMocks());
  it("keeps completion pages explicitly channel and selection scoped", async () => {
    await loadUploadCompletion("channel-a", ["item-a"], 25, 25);
    expect(mocked.invoke).toHaveBeenCalledWith("load_upload_completion", { expectedChannelId: "channel-a", itemIds: ["item-a"], offset: 25, limit: 25, uploadBatchId: undefined });
  });
  it("checks processing only for the selected existing items", async () => {
    await checkUploadCompletion("channel-a", ["item-a", "item-b"]);
    expect(mocked.invoke).toHaveBeenCalledWith("check_upload_completion", { expectedChannelId: "channel-a", itemIds: ["item-a", "item-b"] });
  });
  it("repairs the same reviewed provider video with typed cleanup confirmation", async () => {
    await repairUploadCompletion("channel-a", "item-a", "video-a", ["playlist", "source_cleanup"], "clip.mp4");
    expect(mocked.invoke).toHaveBeenCalledWith("repair_upload_completion", { expectedChannelId: "channel-a", itemId: "item-a", videoId: "video-a", steps: ["playlist", "source_cleanup"], confirmation: "clip.mp4", reviewedJobs: undefined });
    expect(mocked.invoke.mock.calls.some(([command]) => String(command).includes("start_upload"))).toBe(false);
  });
  it("sends only the reviewed asset identities and exact publication version", async () => {
    const reviewedJobs = [
      { key: "thumbnail" as const, jobIds: ["thumb-a"] },
      { key: "caption" as const, jobIds: ["caption-a", "caption-b"] },
      { key: "publication" as const, publishAt: "2026-10-14T10:00:00Z", updatedAt: "2026-10-13T10:00:00Z" },
    ];
    await repairUploadCompletion("channel-a", "item-a", "video-a", ["thumbnail", "caption", "publication"], undefined, reviewedJobs);
    expect(mocked.invoke).toHaveBeenCalledWith("repair_upload_completion", { expectedChannelId: "channel-a", itemId: "item-a", videoId: "video-a", steps: ["thumbnail", "caption", "publication"], confirmation: undefined, reviewedJobs });
    expect(mocked.invoke.mock.calls).toHaveLength(1);
  });
  it("exports only on the explicit scoped command", async () => {
    expect(mocked.invoke).not.toHaveBeenCalled();
    await exportUploadCompletion("channel-a", "csv", "D:/reports/new.csv", ["item-a"]);
    expect(mocked.invoke).toHaveBeenCalledWith("export_upload_completion", { expectedChannelId: "channel-a", itemIds: ["item-a"], format: "csv", path: "D:/reports/new.csv", uploadBatchId: undefined });
  });
  it("lists native saved batches with bounded channel-scoped pagination", async () => {
    await loadUploadCompletionBatches("channel-a", 25, 25);
    expect(mocked.invoke).toHaveBeenCalledWith("load_upload_completion_batches", { expectedChannelId: "channel-a", offset: 25, limit: 25 });
  });
  it("keeps full-batch scope on every completion page and export without constructing a 200-item subset", async () => {
    await loadUploadCompletion("channel-a", undefined, 225, 25, "batch-a");
    await exportUploadCompletion("channel-a", "json", "D:/reports/batch-a.json", undefined, "batch-a");
    expect(mocked.invoke).toHaveBeenNthCalledWith(1, "load_upload_completion", { expectedChannelId: "channel-a", itemIds: undefined, offset: 225, limit: 25, uploadBatchId: "batch-a" });
    expect(mocked.invoke).toHaveBeenNthCalledWith(2, "export_upload_completion", { expectedChannelId: "channel-a", itemIds: undefined, format: "json", path: "D:/reports/batch-a.json", uploadBatchId: "batch-a" });
  });
});
