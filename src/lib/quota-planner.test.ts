import { beforeEach, describe, expect, it, vi } from "vitest";
import { defaultQuotaChoices, estimateQuotaOperations, formatQuotaTime, loadQuotaPlanner, openQuotaResource, parseQuotaLimit, quotaLimitsChanged, quotaResourceUrls, quotaSnapshotMatches, saveQuotaProjectLimits, validQuotaTimeZone, type QuotaSnapshot } from "./quota-planner";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), desktop: vi.fn(() => true), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.desktop }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mocks.open }));

describe("scoped quota planning bridge", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.desktop.mockReturnValue(true); });
  it("loads local evidence and passes exact reviewed operation assumptions to native", async () => {
    const choices = defaultQuotaChoices({ uploads: 51, refreshLibrary: true, libraryVideos: 101, thumbnailWrites: 3, captionInserts: 2, retryReserve: 1, playlists: [{ existingItems: 51, additions: 5 }] });
    await loadQuotaPlanner("channel-a"); await estimateQuotaOperations("channel-a", choices);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "load_quota_planner", { expectedChannelId: "channel-a" });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "estimate_quota_operations", { expectedChannelId: "channel-a", choices });
    expect(choices.searchPages).toBe(0); expect(choices.metadataUpdates).toBe(0);
  });
  it("keeps unknown allowance null and zero as a deliberate entered limit", async () => {
    await saveQuotaProjectLimits("channel-a", "opaque-scope", { uploadCalls: 0, otherUnits: 12000 });
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("save_quota_project_limits", { expectedChannelId: "channel-a", expectedScopeId: "opaque-scope", limits: { uploadCalls: 0, searchCalls: null, otherUnits: 12000 } });
    expect(parseQuotaLimit("")).toBeNull(); expect(parseQuotaLimit("0")).toBe(0); expect(parseQuotaLimit("1000000000000")).toBe(1_000_000_000_000);
    for (const invalid of ["-1", "1.5", "1e4", "words", "1000000000001"]) expect(() => parseQuotaLimit(invalid)).toThrow();
    expect(quotaLimitsChanged({}, { uploadCalls: null })).toBe(false); expect(quotaLimitsChanged({}, { uploadCalls: 0 })).toBe(true);
  });
  it("rejects browser privilege calls and never supplies automatic project defaults", async () => {
    mocks.desktop.mockReturnValue(false);
    await expect(loadQuotaPlanner("channel-a")).rejects.toThrow("desktop app");
    await expect(estimateQuotaOperations("channel-a", defaultQuotaChoices())).rejects.toThrow("desktop app");
    await expect(saveQuotaProjectLimits("channel-a", "scope", {})).rejects.toThrow("desktop app");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
  it("opens only the official fixed Console, extension form and cost-table resources", async () => {
    await openQuotaResource("console"); await openQuotaResource("extension"); await openQuotaResource("costs");
    expect(mocks.open.mock.calls.map(([url]) => url)).toEqual([quotaResourceUrls.console, quotaResourceUrls.extension, quotaResourceUrls.costs]);
    expect(quotaResourceUrls.console).toContain("console.cloud.google.com"); expect(quotaResourceUrls.extension).toBe("https://support.google.com/youtube/contact/yt_api_form");
  });
  it("validates channel/project fences and converts Pacific resets into the chosen timezone", () => {
    const snapshot = { channelId: "channel-a", scopeId: "project-a" } as QuotaSnapshot;
    expect(quotaSnapshotMatches(snapshot, "channel-a", "project-a")).toBe(true); expect(quotaSnapshotMatches(snapshot, "channel-b")).toBe(false); expect(quotaSnapshotMatches(snapshot, "channel-a", "project-b")).toBe(false);
    expect(validQuotaTimeZone("Asia/Singapore")).toBe(true); expect(validQuotaTimeZone("invalid/timezone")).toBe(false);
    expect(formatQuotaTime("2026-10-14T07:00:00Z", "Asia/Singapore")).toContain("3:00");
    expect(formatQuotaTime("invalid", "UTC")).toBe("Saved deadline unavailable"); expect(formatQuotaTime("2026-10-14T07:00:00Z", "invalid/timezone")).toBe("Choose a valid timezone");
  });
});
