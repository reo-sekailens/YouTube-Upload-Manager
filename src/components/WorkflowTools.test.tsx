import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BatchWorkflowTools, CompletionReportScopeDialog, CompletionReportScopeReview, executeReviewedUploadPlan, loadCompletionScopesForChannel, OriginalSelectionDialog, refreshReviewedWorkflow, reviewedProfileTransitionCurrent } from "./BatchWorkflowTools";
import type { BatchImportReceipt, UploadItem } from "../lib/types";
import type { UploadCompletionBatchPage } from "../lib/upload-completion";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), invoke: vi.fn(), resume: vi.fn(), batches: vi.fn() }));
vi.mock("../lib/local", () => ({ isTauri: true, loadConnectionSettings: mocks.connection, resumeQueuedUploads: mocks.resume }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("../lib/upload-completion", async (original) => ({ ...await original<object>(), loadUploadCompletionBatches: mocks.batches }));
const receipt: BatchImportReceipt = { requestedCount: 2, importedCount: 2, queuedCount: 1, duplicateCount: 1, failedCount: 0, items: [] };
const items: UploadItem[] = Array.from({ length: 60 }, (_, index) => ({ id: `item-${index}`, title: `<Clip ${index}>`, fileName: `video-${index}.mp4`, sizeBytes: 12, status: "failed", confirmedBytes: 0, totalBytes: 12, visibility: "private", madeForKids: false, updatedAt: "2026-10-13T00:00:00Z" }));
const noAction = () => undefined;
const batchPage: UploadCompletionBatchPage = { channelId: "channel-a", totalBatches: 2, offset: 0, limit: 25, batches: [{ batchId: "batch-a", createdAt: "2026-10-13T00:00:00Z", itemCount: 512, title: "<Archive batch>" }, { batchId: "batch-b", createdAt: "2026-10-12T00:00:00Z", itemCount: 3, title: "Another reviewed batch" }] };
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
function buttonMarkup(markup: string, label: string) { const buttons = markup.match(/<button\b[\s\S]*?<\/button>/g) || []; return buttons.find((button) => button.includes(label)) || ""; }
function buttonDisabled(markup: string, label: string) { const matching = buttonMarkup(markup, label); expect(matching).not.toBe(""); return /\sdisabled(?:=|[\s>])/.test(matching); }

describe("lazy reviewed workflow entries", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.invoke.mockResolvedValue(receipt); mocks.batches.mockResolvedValue(batchPage); });

  it("exposes all workflow entries without eagerly mounting dialogs or calling native commands", () => {
    const markup = renderToStaticMarkup(<BatchWorkflowTools items={items} channelId="channel-a" busy={false} onRefresh={async () => undefined} onNotice={noAction} />);
    for (const label of ["Preview upload batch", "Estimate API quota", "Upload speed, hours and priority", "Publication calendar", "Locate moved originals", "Upload completion", "Add thumbnails and captions", "Compare original videos", "Saved channel profiles", "Desktop activity and startup"]) expect(buttonDisabled(markup, label)).toBe(false);
    expect(markup).toContain("Plan and transfer"); expect(markup).toContain("Saved uploads"); expect(markup).not.toContain("<dialog");
    expect(mocks.connection).not.toHaveBeenCalled(); expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("keeps device profiles and settings usable while disconnected and holds channel-scoped actions", () => {
    const markup = renderToStaticMarkup(<BatchWorkflowTools items={[]} busy={false} onRefresh={async () => undefined} onNotice={noAction} />);
    for (const label of ["Saved channel profiles", "Desktop activity and startup"]) expect(buttonDisabled(markup, label)).toBe(false);
    for (const label of ["Preview upload batch", "Estimate API quota", "Upload speed, hours and priority", "Publication calendar", "Add thumbnails and captions", "Compare original videos"]) expect(buttonDisabled(markup, label)).toBe(true);
    expect(markup).toContain("Select or connect a YouTube channel");
  });

  it("keeps completion history accessible when no uncancelled queue items remain", () => {
    const markup = renderToStaticMarkup(<BatchWorkflowTools items={[]} channelId="channel-a" busy={false} onRefresh={async () => undefined} onNotice={noAction} />);
    expect(buttonDisabled(markup, "Upload completion")).toBe(false); expect(buttonDisabled(markup, "Locate moved originals")).toBe(true);
  });

  it("accepts only the reviewed profile transition and rejects a later switch away and back", () => {
    expect(reviewedProfileTransitionCurrent("channel-a", "channel-b", 3, "channel-a", 3)).toBe(true);
    expect(reviewedProfileTransitionCurrent("channel-a", "channel-b", 3, "channel-b", 4)).toBe(true);
    expect(reviewedProfileTransitionCurrent("channel-a", undefined, 3, undefined, 4)).toBe(true);
    expect(reviewedProfileTransitionCurrent("channel-a", "channel-b", 3, "channel-c", 4)).toBe(false);
    expect(reviewedProfileTransitionCurrent("channel-a", "channel-b", 3, "channel-a", 5)).toBe(false);
    expect(reviewedProfileTransitionCurrent("channel-a", "channel-b", 3, "channel-b", 6)).toBe(false);
  });

  it("disables new workflow starts while the shared workspace is busy", () => {
    const markup = renderToStaticMarkup(<BatchWorkflowTools items={items} channelId="channel-a" busy onRefresh={async () => undefined} onNotice={noAction} />);
    for (const label of ["Preview upload batch", "Saved channel profiles", "Desktop activity and startup", "Compare original videos"]) expect(buttonDisabled(markup, label)).toBe(true);
  });

  it("holds active or confirmed-deleted originals while retaining attachment tools for confirmed videos", () => {
    const unavailableOriginals: UploadItem[] = [{ ...items[0], status: "uploading" }, { ...items[1], status: "uploaded", videoId: "video-a", sourceDeleteStatus: "deleted" }];
    const markup = renderToStaticMarkup(<BatchWorkflowTools items={unavailableOriginals} channelId="channel-a" busy={false} onRefresh={async () => undefined} onNotice={noAction} />);
    expect(buttonDisabled(markup, "Locate moved originals")).toBe(true); expect(buttonDisabled(markup, "Compare original videos")).toBe(true);
    expect(buttonDisabled(markup, "Add thumbnails and captions")).toBe(false); expect(buttonDisabled(markup, "Upload completion")).toBe(false);
  });

  it("bounds saved-original selection and escapes source names", () => {
    const markup = renderToStaticMarkup(<OriginalSelectionDialog items={items} onSelected={noAction} onClose={noAction} />);
    expect(markup.match(/type="checkbox"/g)).toHaveLength(25); expect(markup).toContain("&lt;Clip 24&gt;"); expect(markup).not.toContain("&lt;Clip 25&gt;"); expect(markup).toContain("Page 1 of 3");
    expect(markup).toContain("Choose up to 100 saved uploads"); expect(markup).toContain('aria-label="Upload selection steps"'); expect(buttonDisabled(markup, "Next: Review selection")).toBe(true);
  });

  it("supports an attachment-specific selection prompt without pretending to relocate originals", () => {
    const markup = renderToStaticMarkup(<OriginalSelectionDialog items={items.slice(0, 2)} onSelected={noAction} onClose={noAction} options={{ label: "Video attachments", title: "Select videos for attachments", description: "Match approved thumbnails and captions to saved videos.", continueLabel: "Choose thumbnails and captions", limit: 2 }} />);
    expect(markup).toContain("Select videos for attachments"); expect(markup).toContain("Choose up to 2 saved uploads"); expect(markup).not.toContain("same original bytes"); expect(markup).not.toContain("Choose replacement location");
  });

  it("imports the exact reviewed plan once and reports its confirmed partial outcomes", async () => {
    const refresh = vi.fn().mockResolvedValue(undefined); const notice = vi.fn();
    await executeReviewedUploadPlan("plan-a", "channel-a", () => true, refresh, notice);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("import_upload_plan", { planId: "plan-a", expectedChannelId: "channel-a" }); expect(refresh).toHaveBeenCalledOnce();
    expect(notice).toHaveBeenCalledWith("2 reviewed originals saved; 1 queued, 1 need duplicate review, 0 need attention.");
  });

  it("blocks a changed or closed workspace before native plan execution", async () => {
    const refresh = vi.fn(); const notice = vi.fn();
    await executeReviewedUploadPlan("plan-a", "channel-a", () => false, refresh, notice);
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-b" });
    await expect(executeReviewedUploadPlan("plan-a", "channel-a", () => true, refresh, notice)).rejects.toThrow("connected channel changed");
    expect(mocks.invoke).not.toHaveBeenCalled(); expect(refresh).not.toHaveBeenCalled(); expect(notice).not.toHaveBeenCalled();
  });

  it("ignores an import receipt that arrives after the channel or workflow changes", async () => {
    const native = deferred<BatchImportReceipt>(); mocks.invoke.mockReturnValue(native.promise); let current = true;
    const refresh = vi.fn(); const notice = vi.fn(); const operation = executeReviewedUploadPlan("plan-a", "channel-a", () => current, refresh, notice);
    await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledOnce()); current = false; native.resolve(receipt); await operation;
    expect(refresh).not.toHaveBeenCalled(); expect(notice).not.toHaveBeenCalled();
  });

  it("refreshes a saved workflow and suppresses success text if its workspace changes during refresh", async () => {
    const notice = vi.fn(); const refresh = vi.fn().mockResolvedValue(undefined);
    await refreshReviewedWorkflow(() => true, refresh, notice, "Attachments saved."); expect(notice).toHaveBeenCalledWith("Attachments saved.");
    notice.mockClear(); const pending = deferred<void>(); refresh.mockReturnValue(pending.promise); let current = true;
    const operation = refreshReviewedWorkflow(() => current, refresh, notice, "Late success"); current = false; pending.resolve(undefined); await operation; expect(notice).not.toHaveBeenCalled();
  });

  it("reports a current refresh failure without applying a late error to another workspace", async () => {
    const notice = vi.fn(); const refresh = vi.fn().mockRejectedValue("Queue refresh unavailable");
    await refreshReviewedWorkflow(() => true, refresh, notice, "Saved"); expect(notice).toHaveBeenCalledWith("Queue refresh unavailable"); notice.mockClear();
    await refreshReviewedWorkflow(() => false, refresh, notice, "Saved"); expect(refresh).toHaveBeenCalledOnce(); expect(notice).not.toHaveBeenCalled();
  });

  it("starts completion reporting with scope and review steps without deriving batches from visible queue rows", () => {
    const selected = vi.fn(); const markup = renderToStaticMarkup(<CompletionReportScopeDialog expectedChannelId="channel-a" channelName="Archive" onClose={noAction} onSelected={selected} />);
    for (const label of ["Choose completion report scope", 'aria-label="Completion report scope steps"', "Choose scope", "Review scope", "All uploads in this channel", "cancelled uploads", "Saved upload batches"]) expect(markup).toContain(label);
    expect(buttonDisabled(markup, "Next: Review report scope")).toBe(true); expect(markup).not.toContain("Open completion report"); expect(mocks.batches).not.toHaveBeenCalled(); expect(selected).not.toHaveBeenCalled();
  });

  it("reviews a complete saved batch above the per-action limit and includes cancelled uploads", () => {
    const markup = renderToStaticMarkup(<CompletionReportScopeReview batch={batchPage.batches[0]} channelId="channel-a" channelName="Archive" />);
    for (const text of ["One saved upload batch", "512 retained uploads", "&lt;Archive batch&gt;", "Batch batch-a", "every retained upload", "including cancelled uploads", "Each follow-up action still requires its own review"]) expect(markup).toContain(text);
    expect(markup).not.toContain("<Archive batch>");
  });

  it("keeps legacy records explicitly in the all-channel report without inventing a batch", () => {
    const markup = renderToStaticMarkup(<CompletionReportScopeReview channelId="channel-a" />);
    expect(markup).toContain("All uploads in this channel"); expect(markup).toContain("older uploads without a saved batch"); expect(markup).toContain("Other channels stay separate"); expect(markup).not.toContain("One saved upload batch");
  });

  it("loads authoritative paginated batch counts with channel checks before and after the native read", async () => {
    expect(await loadCompletionScopesForChannel("channel-a", 0, () => true)).toEqual(batchPage);
    expect(mocks.batches).toHaveBeenCalledExactlyOnceWith("channel-a", 0, 25); expect(mocks.connection).toHaveBeenCalledTimes(2);
    const second = { ...batchPage, offset: 25, totalBatches: 27 }; mocks.batches.mockResolvedValue(second);
    expect(await loadCompletionScopesForChannel("channel-a", 25, () => true)).toEqual(second);
  });

  it("rejects foreign, duplicated or stale batch scope evidence", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(loadCompletionScopesForChannel("channel-a", 0, () => true)).rejects.toThrow("connected channel changed"); expect(mocks.batches).not.toHaveBeenCalled();
    for (const invalid of [{ ...batchPage, channelId: "channel-b" }, { ...batchPage, batches: [batchPage.batches[0], batchPage.batches[0]] }, { ...batchPage, offset: 25 }]) {
      mocks.batches.mockResolvedValueOnce(invalid); await expect(loadCompletionScopesForChannel("channel-a", 0, () => true)).rejects.toThrow("Saved batch scopes did not match");
    }
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-a" }).mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(loadCompletionScopesForChannel("channel-a", 0, () => true)).rejects.toThrow("connected channel changed");
  });

  it("discards a scope page arriving after close or channel change", async () => {
    const native = deferred<UploadCompletionBatchPage>(); mocks.batches.mockReturnValue(native.promise); let current = true;
    const operation = loadCompletionScopesForChannel("channel-a", 0, () => current); await vi.waitFor(() => expect(mocks.batches).toHaveBeenCalledOnce()); current = false; native.resolve(batchPage);
    expect(await operation).toBeUndefined(); expect(mocks.connection).toHaveBeenCalledOnce();
  });
});
