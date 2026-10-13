import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { assertCompletionReportScope, CompletionItemChecklist, CompletionPermissionControls, completionRepairDescription, completionRepairValid, completionReportScopeLabel, exportCompletionForChannel, loadCompletionForChannel, repairCompletionForChannel, repairableCompletionSteps, reviewedCompletionJobs, UploadCompletionDialog } from "./UploadCompletionDialog";
import type { UploadCompletionItem, UploadCompletionPage } from "../lib/upload-completion";
const mocks = vi.hoisted(() => ({ connection: vi.fn(), load: vi.fn(), repair: vi.fn(), export: vi.fn() }));
vi.mock("../lib/local", () => ({ beginDeletionAuthorization: vi.fn(), enableDeletionSudoMode: vi.fn(), loadConnectionSettings: mocks.connection }));
vi.mock("../lib/upload-completion", async (original) => ({ ...await original<typeof import("../lib/upload-completion")>(), checkUploadCompletion: vi.fn(), loadUploadCompletion: mocks.load, repairUploadCompletion: mocks.repair, exportUploadCompletion: mocks.export }));
const item: UploadCompletionItem = { itemId: "item-a", channelId: "channel-a", title: "<A clip>", fileName: "original.mp4", videoId: "video-a", updatedAt: "2026-10-13T00:00:00Z", steps: [
  { key: "upload", outcome: "succeeded", detail: "Confirmed", repairable: false },
  { key: "processing", outcome: "succeeded", detail: "Owner verified", repairable: false },
  { key: "playlist", outcome: "failed", detail: "Saved playlist step failed", repairable: true },
  { key: "source_cleanup", outcome: "failed", detail: "Original retained", repairable: true },
] };
function followupItem(): UploadCompletionItem {
  return { ...item, steps: [
    { key: "thumbnail", outcome: "failed", detail: "Reviewed thumbnail retained", repairable: true, repairJobIds: ["thumb-a"] },
    { key: "caption", outcome: "unknown", detail: "Caption insertion requires reconciliation", repairable: true, repairJobIds: ["caption-a", "caption-b"] },
    { key: "publication", outcome: "waiting", detail: "Private upload and reviewed time retained", repairable: true, publicationSnapshot: { publishAt: "2026-10-14T10:00:00Z", updatedAt: "2026-10-13T10:00:00Z" } },
  ] };
}
describe("completion review", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a", deletionAuthorized: true, deletionSudoActive: true }); mocks.load.mockResolvedValue({ channelId: "channel-a", items: [item], total: 1, pendingProcessing: 0 }); mocks.repair.mockResolvedValue(item); });
  it("opens an accessible checklist with explicit stepped navigation", () => {
    const markup = renderToStaticMarkup(<UploadCompletionDialog expectedChannelId="channel-a" onClose={() => undefined} />);
    expect(markup).toContain('aria-modal="true"');
    expect(markup).toContain('aria-label="Completion repair steps"');
    expect(markup).toContain("Checklist"); expect(markup).toContain("Repair choices"); expect(markup).toContain("Review");
    expect(markup).toContain("Loading this channel’s saved uploads");
    expect(markup).not.toContain("Repair selected steps");
  });
  it("shows separate truthful outcomes without an invented percentage", () => {
    const markup = renderToStaticMarkup(<CompletionItemChecklist item={item} onRepair={() => undefined} />);
    expect(markup).toContain("&lt;A clip&gt;"); expect(markup).not.toContain("<A clip>");
    for (const text of ["Upload", "YouTube processing", "Playlist", "Original source cleanup", "Complete", "Needs attention", "video-a", "Review repair"]) expect(markup).toContain(text);
    expect(markup).not.toContain("%");
  });
  it("only permits the repairable follow-up steps for an existing receipt", () => {
    expect(repairableCompletionSteps(item)).toEqual(["playlist", "source_cleanup"]);
    expect(completionRepairValid(item, ["playlist"], "", true)).toBe(true);
    expect(completionRepairValid({ ...item, videoId: undefined }, ["playlist"], "", true)).toBe(false);
    expect(completionRepairValid(item, ["playlist", "playlist"], "", true)).toBe(false);
    expect(completionRepairValid(item, ["playlist"], "", false)).toBe(false);
  });
  it("requires exact original filename plus final review for source deletion", () => {
    expect(completionRepairValid(item, ["source_cleanup"], "original.mp4", true)).toBe(true);
    expect(completionRepairValid(item, ["source_cleanup"], " original.mp4", true)).toBe(false);
    expect(completionRepairValid(item, ["source_cleanup"], "wrong.mp4", true)).toBe(false);
    expect(completionRepairValid(item, ["source_cleanup"], "original.mp4", false)).toBe(false);
  });
  it("does not present repair actions for succeeded or unrequested follow-ups", () => {
    const complete = { ...item, steps: item.steps.map((step) => ({ ...step, outcome: "succeeded" as const, repairable: false })) };
    expect(repairableCompletionSteps(complete)).toEqual([]);
    expect(renderToStaticMarkup(<CompletionItemChecklist item={complete} onRepair={() => undefined} />)).not.toContain("Review repair");
  });
  it("renders grant, temporary-mode and ready states for waiting playlist work", () => {
    const render = (permission?: { connected: boolean; deletionAuthorized?: boolean; deletionSudoActive?: boolean }) => renderToStaticMarkup(<CompletionPermissionControls permission={permission} busy={false} onAction={() => undefined} />);
    expect(render()).toContain("Grant management permission");
    expect(render({ connected: true, deletionAuthorized: true })).toContain("Enter management mode");
    const ready = render({ connected: true, deletionAuthorized: true, deletionSudoActive: true });
    expect(ready).toContain("Pending video follow-ups can continue"); expect(ready).not.toContain("Enter management mode"); expect(ready).toContain("Refresh permission");
  });
  it("loads and repairs the selected existing video without initiating any upload", async () => {
    const loaded = await loadCompletionForChannel("channel-a", ["item-a"], 0, () => true);
    expect(loaded?.page.items).toEqual([item]);
    expect(await repairCompletionForChannel("channel-a", item, ["playlist", "source_cleanup"], "original.mp4", true, () => true)).toEqual(item);
    expect(mocks.repair).toHaveBeenCalledWith("channel-a", "item-a", "video-a", ["playlist", "source_cleanup"], "original.mp4", undefined);
  });
  it("rejects mismatched channel or selected-item completion pages", async () => {
    mocks.load.mockResolvedValue({ channelId: "channel-b", items: [item], total: 1, pendingProcessing: 0 });
    await expect(loadCompletionForChannel("channel-a", ["item-a"], 0, () => true)).rejects.toThrow("another channel or selection");
    mocks.load.mockResolvedValue({ channelId: "channel-a", items: [{ ...item, itemId: "other" }], total: 1, pendingProcessing: 0 });
    await expect(loadCompletionForChannel("channel-a", ["item-a"], 0, () => true)).rejects.toThrow("another channel or selection");
  });
  it("blocks stale channel or expired management mode before a playlist write", async () => {
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-b", deletionAuthorized: true, deletionSudoActive: true });
    await expect(repairCompletionForChannel("channel-a", item, ["playlist"], "", true, () => true)).rejects.toThrow("channel changed");
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a", deletionAuthorized: true, deletionSudoActive: false });
    await expect(repairCompletionForChannel("channel-a", item, ["playlist"], "", true, () => true)).rejects.toThrow("temporary mode");
    expect(mocks.repair).not.toHaveBeenCalled();
  });
  it("keeps source cleanup independent of management consent while requiring exact confirmation", async () => {
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a", deletionAuthorized: false });
    await expect(repairCompletionForChannel("channel-a", item, ["source_cleanup"], "wrong", true, () => true)).rejects.toThrow("Review and approve");
    expect(mocks.repair).not.toHaveBeenCalled();
    expect(await repairCompletionForChannel("channel-a", item, ["source_cleanup"], "original.mp4", true, () => true)).toEqual(item);
  });
  it("does not adopt late completion reads after close or scope invalidation", async () => {
    let resolve!: (page: UploadCompletionPage) => void; let current = true;
    mocks.load.mockReturnValue(new Promise<UploadCompletionPage>((done) => { resolve = done; }));
    const operation = loadCompletionForChannel("channel-a", ["item-a"], 0, () => current);
    await Promise.resolve(); current = false; resolve({ channelId: "channel-a", items: [item], total: 1, pendingProcessing: 0 });
    expect(await operation).toBeUndefined();
  });
  it("leaves failures visible and rejects an unrelated native repair receipt", async () => {
    mocks.repair.mockRejectedValue("Membership unavailable");
    await expect(repairCompletionForChannel("channel-a", item, ["playlist"], "", true, () => true)).rejects.toBe("Membership unavailable");
    mocks.repair.mockResolvedValue({ ...item, videoId: "wrong-video" });
    await expect(repairCompletionForChannel("channel-a", item, ["playlist"], "", true, () => true)).rejects.toThrow("did not match");
  });
  it("shows thumbnail, caption and publication outcomes as separate existing-video steps", () => {
    const retained = followupItem();
    const markup = renderToStaticMarkup(<CompletionItemChecklist item={retained} onRepair={() => undefined} />);
    for (const text of ["Thumbnail", "Caption", "Scheduled publication", "Needs attention", "Unknown", "Waiting", "video-a"]) expect(markup).toContain(text);
    expect(completionRepairDescription(retained, "caption")).toContain("2 reviewed caption jobs");
    expect(completionRepairDescription(retained, "caption")).toContain("no caption is replaced or deleted");
    expect(completionRepairDescription(retained, "publication")).toContain("2026-10-14T10:00:00Z");
    expect(completionRepairDescription(retained, "thumbnail")).toContain("Later attachments require a separate review");
  });
  it("requires native-retained exact job identities and a publication version before review", () => {
    const retained = followupItem();
    expect(repairableCompletionSteps(retained)).toEqual(["thumbnail", "caption", "publication"]);
    expect(completionRepairValid(retained, ["thumbnail", "caption", "publication"], "", true)).toBe(true);
    const incomplete = { ...retained, steps: retained.steps.map((step) => ({ ...step, repairJobIds: undefined, publicationSnapshot: undefined })) };
    expect(repairableCompletionSteps(incomplete)).toEqual([]);
    expect(completionRepairValid(incomplete, ["caption"], "", true)).toBe(false);
    expect(renderToStaticMarkup(<CompletionItemChecklist item={incomplete} onRepair={() => undefined} />)).not.toContain("Review repair");
    expect(repairableCompletionSteps({ ...retained, steps: [{ ...retained.steps[1], repairJobIds: ["caption-a", "caption-a"] }] })).toEqual([]);
    expect(repairableCompletionSteps({ ...retained, steps: [{ ...retained.steps[1], repairJobIds: [] }] })).toEqual([]);
    expect(repairableCompletionSteps({ ...retained, steps: [{ ...retained.steps[2], publicationSnapshot: { publishAt: "2026-10-14T10:00:00Z", updatedAt: "" } }] })).toEqual([]);
  });
  it("copies only selected retained jobs, excluding unselected and later declarations", () => {
    const retained = followupItem();
    const snapshot = reviewedCompletionJobs(retained, ["caption", "publication"]);
    retained.steps[1].repairJobIds!.push("later-caption");
    retained.steps[2].publicationSnapshot!.publishAt = "2026-10-15T10:00:00Z";
    expect(snapshot).toEqual([
      { key: "caption", jobIds: ["caption-a", "caption-b"] },
      { key: "publication", publishAt: "2026-10-14T10:00:00Z", updatedAt: "2026-10-13T10:00:00Z" },
    ]);
    expect(reviewedCompletionJobs(item, ["playlist", "source_cleanup"])).toEqual([]);
  });
  it("freezes approved job identities before an asynchronous permission refresh", async () => {
    let resolve!: (settings: { connected: boolean; activeChannelId: string; deletionAuthorized: boolean; deletionSudoActive: boolean }) => void;
    mocks.connection.mockReturnValue(new Promise((done) => { resolve = done; }));
    const retained = followupItem();
    const resultItem = followupItem();
    mocks.repair.mockResolvedValue(resultItem);
    const steps = ["caption", "publication"] as const;
    const operation = repairCompletionForChannel("channel-a", retained, [...steps], "", true, () => true);
    retained.steps[1].repairJobIds!.push("later-caption");
    retained.steps[2].publicationSnapshot!.updatedAt = "2026-10-13T11:00:00Z";
    resolve({ connected: true, activeChannelId: "channel-a", deletionAuthorized: true, deletionSudoActive: true });
    expect(await operation).toEqual(resultItem);
    expect(mocks.repair).toHaveBeenCalledWith("channel-a", "item-a", "video-a", [...steps], undefined, [
      { key: "caption", jobIds: ["caption-a", "caption-b"] },
      { key: "publication", publishAt: "2026-10-14T10:00:00Z", updatedAt: "2026-10-13T10:00:00Z" },
    ]);
  });
  it("requires fresh temporary management permission for caption or publication repair", async () => {
    const retained = followupItem();
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a", deletionAuthorized: true, deletionSudoActive: false });
    for (const step of ["caption", "publication"] as const) {
      await expect(repairCompletionForChannel("channel-a", retained, [step], "", true, () => true)).rejects.toThrow("temporary mode");
    }
    expect(mocks.repair).not.toHaveBeenCalled();
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a", deletionAuthorized: true, deletionSudoActive: true });
    mocks.repair.mockResolvedValue(retained);
    expect(await repairCompletionForChannel("channel-a", retained, ["thumbnail"], "", true, () => true)).toEqual(retained);
    expect(mocks.repair).toHaveBeenCalledWith("channel-a", "item-a", "video-a", ["thumbnail"], undefined, [{ key: "thumbnail", jobIds: ["thumb-a"] }]);
  });
  it("keeps thumbnail repair on normal upload permission without requiring management consent", async () => {
    const retained = followupItem();
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a", deletionAuthorized: false, deletionSudoActive: false });
    mocks.repair.mockResolvedValue(retained);
    expect(await repairCompletionForChannel("channel-a", retained, ["thumbnail"], "", true, () => true)).toEqual(retained);
    expect(mocks.repair).toHaveBeenCalledWith("channel-a", "item-a", "video-a", ["thumbnail"], undefined, [{ key: "thumbnail", jobIds: ["thumb-a"] }]);
  });
  it("discards a late asset repair response after the channel request is invalidated", async () => {
    let resolve!: (result: UploadCompletionItem) => void;
    let current = true;
    const retained = followupItem();
    mocks.repair.mockReturnValue(new Promise<UploadCompletionItem>((done) => { resolve = done; }));
    const operation = repairCompletionForChannel("channel-a", retained, ["caption"], "", true, () => current);
    await Promise.resolve();
    current = false;
    resolve(retained);
    expect(await operation).toBeUndefined();
  });
  it("uses a saved batch scope on later pages without a selected-item cap", async () => {
    mocks.load.mockResolvedValue({ channelId: "channel-a", uploadBatchId: "batch-a", items: [item], total: 250, pendingProcessing: 0 });
    const result = await loadCompletionForChannel("channel-a", undefined, 225, () => true, "batch-a");
    expect(result?.page.total).toBe(250);
    expect(mocks.load).toHaveBeenCalledWith("channel-a", undefined, 225, 25, "batch-a");
    expect(completionReportScopeLabel(undefined, "batch-a", 250)).toBe("all 250 uploads in the selected saved batch");
    expect(completionReportScopeLabel(["item-a"])).toBe("the selected uploads");
    expect(completionReportScopeLabel()).toBe("all uploads for this connected channel");
    const markup = renderToStaticMarkup(<UploadCompletionDialog expectedChannelId="channel-a" uploadBatchId="batch-a" onClose={() => undefined} />);
    expect(markup).toContain("Loading this saved upload batch");
    expect(markup).not.toContain("Loading this channel’s saved uploads");
  });
  it("rejects mixed report scopes and missing saved batch identity before any native read", async () => {
    expect(() => assertCompletionReportScope(["item-a"], "batch-a")).toThrow("rather than combining");
    expect(() => assertCompletionReportScope(undefined, " ")).toThrow("valid saved upload batch");
    await expect(loadCompletionForChannel("channel-a", ["item-a"], 0, () => true, "batch-a")).rejects.toThrow("rather than combining");
    expect(mocks.connection).not.toHaveBeenCalled();
    expect(mocks.load).not.toHaveBeenCalled();
  });
  it("rejects another batch or an unfiltered native page while a saved batch is reviewed", async () => {
    for (const batch of [undefined, "batch-b"]) {
      mocks.load.mockResolvedValue({ channelId: "channel-a", uploadBatchId: batch, items: [item], total: 250, pendingProcessing: 0 });
      await expect(loadCompletionForChannel("channel-a", undefined, 0, () => true, "batch-a")).rejects.toThrow("another channel or selection");
    }
    mocks.load.mockResolvedValue({ channelId: "channel-a", uploadBatchId: "batch-a", items: [item], total: 250, pendingProcessing: 0 });
    await expect(loadCompletionForChannel("channel-a", ["item-a"], 0, () => true)).rejects.toThrow("another channel or selection");
  });
  it("exports the whole reviewed saved batch and validates its returned identity", async () => {
    mocks.export.mockResolvedValue({ count: 250, format: "csv", uploadBatchId: "batch-a" });
    expect(await exportCompletionForChannel("channel-a", "csv", "D:/reports/batch-a.csv", undefined, () => true, "batch-a")).toEqual({ count: 250, format: "csv", uploadBatchId: "batch-a" });
    expect(mocks.export).toHaveBeenCalledWith("channel-a", "csv", "D:/reports/batch-a.csv", undefined, "batch-a");
    mocks.export.mockResolvedValue({ count: 250, format: "csv", uploadBatchId: "batch-b" });
    await expect(exportCompletionForChannel("channel-a", "csv", "D:/reports/batch-a.csv", undefined, () => true, "batch-a")).rejects.toThrow("reviewed report scope");
  });
  it("retains the item selection export route and refuses stale-channel batch export", async () => {
    mocks.export.mockResolvedValue({ count: 1, format: "json" });
    expect(await exportCompletionForChannel("channel-a", "json", "D:/reports/selected.json", ["item-a"], () => true)).toEqual({ count: 1, format: "json" });
    expect(mocks.export).toHaveBeenCalledWith("channel-a", "json", "D:/reports/selected.json", ["item-a"], undefined);
    mocks.export.mockClear();mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-b" });
    await expect(exportCompletionForChannel("channel-a", "json", "D:/reports/batch-a.json", undefined, () => true, "batch-a")).rejects.toThrow("channel changed");
    expect(mocks.export).not.toHaveBeenCalled();
  });
  it("drops late full-batch reads and export receipts after scope invalidation", async () => {
    let resolveRead!: (page: UploadCompletionPage) => void;
    let current = true;
    mocks.load.mockReturnValue(new Promise<UploadCompletionPage>((done) => { resolveRead = done; }));
    const read = loadCompletionForChannel("channel-a", undefined, 25, () => current, "batch-a");
    await Promise.resolve();current = false;
    resolveRead({ channelId: "channel-a", uploadBatchId: "batch-a", items: [item], total: 250, pendingProcessing: 0 });
    expect(await read).toBeUndefined();
    let resolveExport!: (receipt: { count: number; format: "json"; uploadBatchId: string }) => void;
    current = true;
    mocks.export.mockReturnValue(new Promise((done) => { resolveExport = done; }));
    const report = exportCompletionForChannel("channel-a", "json", "D:/reports/batch-a.json", undefined, () => current, "batch-a");
    await Promise.resolve();current = false;
    resolveExport({ count: 250, format: "json", uploadBatchId: "batch-a" });
    expect(await report).toBeUndefined();
  });
});
