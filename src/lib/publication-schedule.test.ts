import { beforeEach, describe, expect, it, vi } from "vitest";
import { editPublicationCalendar, listPublicationTimezones, loadPublicationCalendar, publicationJobEditable, publicationLocalInput, publicationPreviewReady, publicationTimesByPath, publicationUtcLabel, repairPublicationJob, resolvePublicationCalendar, type PublicationJob, type PublicationPreview, type PublicationRequest } from "./publication-schedule";
const mock = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => mock);
const preview: PublicationPreview = { channelId: "channel-a", timeZone: "America/New_York", tzdbVersion: "2025b", ready: true, rows: [{ index: 0, localTime: "2026-11-01T01:30", state: "ready", publishAt: "2026-11-01T06:30:00+00:00", candidates: [], preserved: false }] };
const job: PublicationJob = { itemId: "item-a", channelId: "channel-a", title: "Original", fileName: "clip.mp4", publishAt: preview.rows[0].publishAt!, timeZone: preview.timeZone, localTime: "2026-11-01T01:30:00-05:00", tzdbVersion: "2025b", state: "accepted", everPublic: false, detail: "Confirmed", updatedAt: "revision" };
describe("publication calendar bridge", () => {
  beforeEach(() => { mock.invoke.mockReset(); mock.isTauri.mockReturnValue(true); });
  it("keeps explicit timezone, fold choices and approved UTC dates in the native preview request", async () => {
    const request: PublicationRequest = { timeZone: "America/New_York", startLocal: "2026-11-01T01:30", spacingMinutes: 30, count: 1, itemLocalTimes: {}, ambiguousChoices: { 0: "later" }, preservedTimes: {} };
    await resolvePublicationCalendar("channel-a", request);
    expect(mock.invoke).toHaveBeenCalledWith("resolve_publication_calendar", { expectedChannelId: "channel-a", ...request });
  });
  it("requires final approval and optimistic versions for existing job edits and repair", async () => {
    const changes = [{ itemId: job.itemId, expectedPublishAt: job.publishAt, expectedUpdatedAt: job.updatedAt, publishAt: job.publishAt, context: { timeZone: job.timeZone, tzdbVersion: job.tzdbVersion } }];
    await loadPublicationCalendar("channel-a", [job.itemId], 50); await editPublicationCalendar("channel-a", changes, true); await repairPublicationJob("channel-a", job.itemId, true);
    expect(mock.invoke.mock.calls).toEqual([["load_publication_calendar", { expectedChannelId: "channel-a", itemIds: [job.itemId], offset: 50 }], ["edit_publication_calendar", { expectedChannelId: "channel-a", changes, approved: true }], ["repair_publication_job", { expectedChannelId: "channel-a", itemId: job.itemId, approved: true }]]);
  });
  it("maps exact selected paths only from a complete native channel preview", () => {
    expect(publicationTimesByPath(preview, ["D:\\Original <clip>.mp4"])).toEqual({ "D:\\Original <clip>.mp4": job.publishAt });
    for (const altered of [{ ...preview, channelId: "channel-b" }, { ...preview, ready: false }, { ...preview, rows: [{ ...preview.rows[0], index: 1 }] }, { ...preview, rows: [{ ...preview.rows[0], state: "ambiguous" as const }] }, { ...preview, rows: [{ ...preview.rows[0], publishAt: "bad" }] }]) expect(publicationPreviewReady(altered, "channel-a", 1)).toBe(false);
    expect(() => publicationTimesByPath(preview, ["one", "two"])).toThrow("complete native");
  });
  it("uses local timezone display without shifting a reviewed UTC value", () => {
    expect(publicationLocalInput(job.publishAt, "Asia/Singapore")).toBe("2026-11-01T14:30");
    expect(publicationLocalInput(job.publishAt, "America/New_York")).toBe("2026-11-01T01:30");
    expect(publicationUtcLabel(job.publishAt)).toBe("2026-11-01 06:30:00 UTC"); expect(publicationLocalInput(job.publishAt, "Unknown/Place")).toBe("");
  });
  it("withholds unknown and published jobs from time editing", () => {
    expect(publicationJobEditable(job)).toBe(true);
    for (const state of ["uncertain", "applying", "published"]) expect(publicationJobEditable({ ...job, state })).toBe(false);
    expect(publicationJobEditable({ ...job, everPublic: true })).toBe(false);
  });
  it("does not fabricate native timezone rules in browser mode", async () => { mock.isTauri.mockReturnValue(false); expect(await listPublicationTimezones()).toEqual([]); expect(mock.invoke).not.toHaveBeenCalled(); });
});
