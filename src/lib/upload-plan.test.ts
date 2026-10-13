import { beforeEach, describe, expect, it, vi } from "vitest";
import { createUploadPlan, discardUploadPlan, exportUploadPlan, fileNameFromSelection, loadUploadPlan, planIsCurrent, planMatchesChannel, planPublicationTimes, planSealMatchesBase, planWithMetadata, planWithPublication, planWithoutSourceChoices, type UploadPlanPage } from "./upload-plan";
import type { ManualUploadSettings } from "./types";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), desktop: vi.fn(() => true), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.desktop }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: mocks.save }));

const plan: UploadPlanPage = { id: "preview-a", channelId: "channel-a", channelName: "Channel A", createdAt: "2026-10-13T00:00:00Z", expiresAt: "2026-10-13T00:15:00Z", evidenceMode: "cached", totalFiles: 1, readyFiles: 1, duplicateFiles: 0, blockedFiles: 0, offset: 0, limit: 25, files: [], warnings: [] };

describe("native read-only upload planning bridge", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.desktop.mockReturnValue(true); });

  it("passes every reviewed choice and channel to native without creating jobs", async () => {
    const settings = { madeForKids: true, visibility: "unlisted" as const, playlistId: "playlist-a", playlistTitle: "Archive", deleteSourceAfterUpload: true, metadata: { title: "{filename}", notifySubscribers: false }, itemMetadata: { "D:/selected/a.mp4": { description: "Per-file override" } } };
    mocks.invoke.mockResolvedValue(plan);
    expect(await createUploadPlan(["D:/selected/a.mp4"], settings, "channel-a")).toEqual(plan);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("create_upload_plan", { paths: ["D:/selected/a.mp4"], settings, expectedChannelId: "channel-a", onlineRefresh: false });
  });

  it("discloses online reads only with the explicit refresh argument", async () => {
    await createUploadPlan(["/selected/a.mp4"], { madeForKids: false, visibility: "private", deleteSourceAfterUpload: false }, "channel-a", true);
    expect(mocks.invoke.mock.calls[0][1]).toMatchObject({ expectedChannelId: "channel-a", onlineRefresh: true });
  });

  it("pages the native snapshot using its immutable channel and opaque identity", async () => {
    await loadUploadPlan("preview-a", "channel-a", 25, 25);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("load_upload_plan", { planId: "preview-a", expectedChannelId: "channel-a", offset: 25, limit: 25 });
  });

  it("cancelled export never creates a file", async () => {
    mocks.save.mockResolvedValue(null);
    expect(await exportUploadPlan("preview-a", "channel-a")).toBeUndefined();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("saves redacted data through native rather than webview filesystem access", async () => {
    mocks.save.mockResolvedValue("D:/reports/batch.json");
    mocks.invoke.mockResolvedValue("D:/reports/batch.json");
    expect(await exportUploadPlan("preview-a", "channel-a")).toBe("D:/reports/batch.json");
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("save_upload_plan_export", { planId: "preview-a", expectedChannelId: "channel-a", path: "D:/reports/batch.json" });
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({ filters: [{ name: "JSON batch preview", extensions: ["json"] }] }));
  });

  it("propagates export failures and scopes revocation to the original channel", async () => {
    mocks.save.mockResolvedValue("D:/reports/existing.json");
    mocks.invoke.mockRejectedValueOnce(new Error("Choose a new preview filename"));
    await expect(exportUploadPlan("preview-a", "channel-a")).rejects.toThrow("new preview filename");
    await discardUploadPlan("preview-a", "channel-a");
    expect(mocks.invoke).toHaveBeenLastCalledWith("discard_upload_plan", { planId: "preview-a", expectedChannelId: "channel-a" });
  });

  it("never enables planning or filesystem export from an ordinary browser", async () => {
    mocks.desktop.mockReturnValue(false);
    await expect(createUploadPlan(["/selected/a.mp4"], { madeForKids: false, visibility: "private", deleteSourceAfterUpload: false }, "channel-a")).rejects.toThrow("desktop app");
    await expect(loadUploadPlan("preview-a", "channel-a")).rejects.toThrow("desktop app");
    await expect(exportUploadPlan("preview-a", "channel-a")).rejects.toThrow("desktop app");
    await discardUploadPlan("preview-a", "channel-a");
    expect(mocks.invoke).not.toHaveBeenCalled(); expect(mocks.save).not.toHaveBeenCalled();
  });

  it("rejects expired approvals, malformed expiry and channel mismatches", () => {
    expect(planIsCurrent(plan, Date.parse("2026-10-13T00:14:59Z"))).toBe(true);
    expect(planIsCurrent(plan, Date.parse(plan.expiresAt))).toBe(false);
    expect(planIsCurrent({ ...plan, expiresAt: "invalid" }, 0)).toBe(false);
    expect(planMatchesChannel(plan, "channel-a")).toBe(true);
    expect(planMatchesChannel(plan, "channel-b")).toBe(false);
    expect(planMatchesChannel({ ...plan, id: "" }, "channel-a")).toBe(false);
    expect(planMatchesChannel({ ...plan, files: [{ channelId: "channel-b" } as UploadPlanPage["files"][number]] }, "channel-a")).toBe(false);
    expect(fileNameFromSelection("D:\\private\\camera.mp4")).toBe("camera.mp4");
    expect(fileNameFromSelection("/private/camera.mp4")).toBe("camera.mp4");
  });

  it("keeps publication fallback consistent with complete per-file metadata replacement", () => {
    const settings: ManualUploadSettings = { madeForKids: false, visibility: "private", deleteSourceAfterUpload: false, metadata: { publishAt: "2027-01-01T00:00:00Z" }, itemMetadata: { a: {}, b: { publishAt: "2027-01-02T00:00:00Z" } } };
    expect(planPublicationTimes(["a", "b", "c"], settings)).toEqual({ b: "2027-01-02T00:00:00Z", c: "2027-01-01T00:00:00Z" });
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("applies reviewed UTC schedules as private without clearing any other deliberate metadata", async () => {
    const settings: ManualUploadSettings = { madeForKids: true, visibility: "public", deleteSourceAfterUpload: true, playlistId: "playlist-a", metadata: { description: "Common", tags: ["event"], notifySubscribers: true }, itemMetadata: { a: { description: "", tags: [], notifySubscribers: false, containsSyntheticMedia: false }, b: {} } };
    const result = planWithPublication(["a", "b", "c"], settings, { publishAtByPath: { a: "2027-01-01T00:00:00Z", b: "2027-01-01T01:00:00Z", c: "2027-01-01T02:00:00Z" }, timeZone: "Asia/Singapore", tzdbVersion: "2025b" });
    expect(result.visibility).toBe("private"); expect(result.madeForKids).toBe(true); expect(result.playlistId).toBe("playlist-a"); expect(result.deleteSourceAfterUpload).toBe(true);
    expect(result.itemMetadata?.a).toEqual({ ...settings.itemMetadata?.a, publishAt: "2027-01-01T00:00:00Z" }); expect(result.itemMetadata?.b).toEqual({ publishAt: "2027-01-01T01:00:00Z" }); expect(result.itemMetadata?.c).toEqual({ ...settings.metadata, publishAt: "2027-01-01T02:00:00Z" });
    expect(result.publicationContexts).toEqual(Object.fromEntries(["a", "b", "c"].map((path) => [path, { timeZone: "Asia/Singapore", tzdbVersion: "2025b" }])));
    expect(settings.visibility).toBe("public"); expect(settings.itemMetadata?.a).not.toHaveProperty("publishAt"); expect(mocks.invoke).not.toHaveBeenCalled();
    await createUploadPlan(["a", "b", "c"], result, "channel-a");
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("create_upload_plan", { paths: ["a", "b", "c"], settings: result, expectedChannelId: "channel-a", onlineRefresh: false });
  });

  it("rejects incomplete schedules and missing native timezone proof before changing settings", () => {
    const settings: ManualUploadSettings = { madeForKids: false, visibility: "private", deleteSourceAfterUpload: false };
    const invalid: Array<Parameters<typeof planWithPublication>[2]> = [{ publishAtByPath: {}, timeZone: "UTC", tzdbVersion: "2025b" }, { publishAtByPath: { a: "bad" }, timeZone: "UTC", tzdbVersion: "2025b" }, { publishAtByPath: { b: "2027-01-01T00:00:00Z" }, timeZone: "UTC", tzdbVersion: "2025b" }, { publishAtByPath: { a: "2027-01-01T00:00:00Z" }, timeZone: "UTC", tzdbVersion: "" }];
    for (const result of invalid) expect(() => planWithPublication(["a"], settings, result)).toThrow("native timezone proof");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("drops old timezone proof when metadata changes a publication instant", () => {
    const context = { timeZone: "UTC", tzdbVersion: "2025b" };
    const settings: ManualUploadSettings = { madeForKids: false, visibility: "public", deleteSourceAfterUpload: false, itemMetadata: { a: { publishAt: "2027-01-01T00:00:00Z" }, b: { publishAt: "2027-01-01T01:00:00Z" } }, publicationContexts: { a: context, b: context } };
    const result = planWithMetadata(["a", "b"], settings, {}, { a: { publishAt: "2027-01-01T00:00:00+00:00" }, b: { publishAt: "2027-01-02T01:00:00Z" } });
    expect(result.publicationContexts).toEqual({ a: context }); expect(result.visibility).toBe("private"); expect(settings.publicationContexts).toEqual({ a: context, b: context });
    expect(planWithMetadata(["a", "b"], result, {}, {}).publicationContexts).toBeUndefined();
  });

  it("clears source-bound choices for new originals while retaining visible audience and destination choices", () => {
    const settings: ManualUploadSettings = { madeForKids: true, visibility: "private", deleteSourceAfterUpload: false, playlistId: "playlist-a", metadata: { description: "Old original" }, itemMetadata: { a: { title: "Old title", publishAt: "2027-01-01T00:00:00Z" } }, publicationContexts: { a: { timeZone: "UTC", tzdbVersion: "2025b" } } };
    expect(planWithoutSourceChoices(settings)).toEqual({ ...settings, metadata: undefined, itemMetadata: undefined, publicationContexts: undefined });
    expect(settings.itemMetadata?.a.title).toBe("Old title"); expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("accepts a sealed asset snapshot only in its original channel and expiry context", () => {
    const base = { ...plan, createdAt: new Date().toISOString(), expiresAt: new Date(Date.now() + 900_000).toISOString() };
    const sealed = { ...base, id: "sealed-new" };
    expect(planSealMatchesBase(base, sealed)).toBe(true);
    for (const invalid of [{ ...sealed, id: base.id }, { ...sealed, channelId: "other" }, { ...sealed, totalFiles: 2 }, { ...sealed, createdAt: "changed" }, { ...sealed, evidenceMode: "online" as const }, { ...sealed, expiresAt: new Date(Date.now() + 1_800_000).toISOString() }, { ...sealed, expiresAt: "expired" }]) expect(planSealMatchesBase(base, invalid)).toBe(false);
    expect(planSealMatchesBase({ ...base, expiresAt: "2020-01-01T00:00:00Z" }, sealed)).toBe(false);
  });

  it("rejects duplicate or out-of-range native file ordinals before associating attachments", () => {
    const row = { ordinal: 0, channelId: "channel-a" } as UploadPlanPage["files"][number];
    expect(planMatchesChannel({ ...plan, files: [row, row] }, "channel-a")).toBe(false);
    expect(planMatchesChannel({ ...plan, files: [{ ...row, ordinal: 1 }] }, "channel-a")).toBe(false);
    expect(planMatchesChannel({ ...plan, totalFiles: 513, files: [row] }, "channel-a")).toBe(false);
  });
});
