import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { applyIntakePublicationTimes, intakePublicationTimes, retainIntakePublicationContexts, resolveDefaultUploadPlaylist, UploadIntakeReview } from "./UploadIntakeReview";
import type { ManualUploadDefaults } from "../lib/types";

const defaults: ManualUploadDefaults = { madeForKids: false, visibility: "unlisted", playlistId: "saved", playlistTitle: "Old cached title", playlistChannelId: "channel-a" };
const playlists = [{ id: "saved", title: "Current playlist title" }];

describe("manual upload review defaults", () => {
  it("uses current playlist evidence for the same channel", () => {
    expect(resolveDefaultUploadPlaylist(defaults, "channel-a", playlists)).toEqual(playlists[0]);
  });

  it("does not apply a playlist from another channel or a missing playlist", () => {
    expect(resolveDefaultUploadPlaylist(defaults, "channel-b", playlists)).toBeUndefined();
    expect(resolveDefaultUploadPlaylist(defaults, undefined, playlists)).toBeUndefined();
    expect(resolveDefaultUploadPlaylist(defaults, "channel-a", [])).toBeUndefined();
    expect(resolveDefaultUploadPlaylist(undefined, "channel-a", playlists)).toBeUndefined();
  });

  it("holds import until saved defaults load and presents editable choices", () => {
    const markup = renderToStaticMarkup(<UploadIntakeReview paths={["fixture.mp4"]} activeChannelId="channel-a" onCancel={() => undefined} onConfirm={() => undefined} />);
    expect(markup).toContain("Loading saved upload defaults");
    expect(markup).toContain("Visibility");
    expect(markup).toContain("No playlist");
    expect(markup).toMatch(/disabled=""[^>]*>Import reviewed videos/);
    expect(markup).toContain("Publication calendar");
    expect(markup).toContain("Choose a timezone and per-video publication dates in three steps");
    expect(markup).toContain("Scheduled videos upload privately");
  });

  it("initializes the calendar from exact per-file schedules or their common metadata fallback", () => {
    const common = { publishAt: "2027-01-01T00:00:00Z" };
    expect(intakePublicationTimes(["a", "b", "c"], common, { a: {}, b: { publishAt: "2027-02-01T00:00:00Z" }, unrelated: common })).toEqual({ b: "2027-02-01T00:00:00Z", c: common.publishAt });
    expect(intakePublicationTimes(["a"], common, Object.create({ a: {} }))).toEqual({ a: common.publishAt });
  });

  it("applies full per-file metadata replacements while preserving each file’s other reviewed fields", () => {
    const common = { description: "Common description", tags: ["common"], notifySubscribers: true };
    const individual = { "D:\\a.mp4": { description: "", tags: [], notifySubscribers: false, containsSyntheticMedia: false }, "D:\\b.mp4": {}, removed: { title: "Old selection" } };
    const result = applyIntakePublicationTimes(["D:\\a.mp4", "D:\\b.mp4", "D:\\c.mp4"], { "D:\\a.mp4": "2027-01-01T00:00:00Z", "D:\\b.mp4": "2027-01-01T01:00:00Z", "D:\\c.mp4": "2027-01-01T02:00:00Z" }, common, individual);
    expect(result["D:\\a.mp4"]).toEqual({ ...individual["D:\\a.mp4"], publishAt: "2027-01-01T00:00:00Z" });
    expect(result["D:\\b.mp4"]).toEqual({ publishAt: "2027-01-01T01:00:00Z" });
    expect(result["D:\\c.mp4"]).toEqual({ ...common, publishAt: "2027-01-01T02:00:00Z" });
    expect(result).not.toHaveProperty("removed"); expect(individual["D:\\a.mp4"]).not.toHaveProperty("publishAt"); expect(common).not.toHaveProperty("publishAt");
  });

  it("withholds incomplete, duplicate, unselected and invalid calendar values", () => {
    for (const [paths, values] of [[[], {}], [["a", "a"], { a: "2027-01-01T00:00:00Z" }], [["a", "b"], { a: "2027-01-01T00:00:00Z" }], [["a"], { b: "2027-01-01T00:00:00Z" }], [["a"], { a: "invalid" }], [["a"], { a: "2027-01-01T00:00:00Z", b: "2027-01-01T00:00:00Z" }]] as Array<[string[], Record<string, string>]>) expect(() => applyIntakePublicationTimes(paths, values)).toThrow("every selected original");
  });

  it("retains timezone proof only while its exact selected publication instant is unchanged", () => {
    const context = { timeZone: "Asia/Singapore", tzdbVersion: "2025b" };
    const previous = { a: "2027-01-01T00:00:00Z", b: "2027-01-01T01:00:00Z", c: "2027-01-01T02:00:00Z" };
    expect(retainIntakePublicationContexts(previous, { a: "2027-01-01T00:00:00+00:00", b: "2027-01-02T01:00:00Z" }, { a: context, b: context, c: context, stale: context })).toEqual({ a: context });
    expect(retainIntakePublicationContexts(previous, {}, { a: context })).toBeUndefined();
    expect(retainIntakePublicationContexts(previous, { a: "bad" }, { a: context })).toBeUndefined();
  });

  it("keeps the calendar unavailable until a channel and original selection exist", () => {
    const markup = renderToStaticMarkup(<UploadIntakeReview paths={[]} onCancel={() => undefined} onConfirm={() => undefined} />);
    expect(markup).toMatch(/disabled=""[^>]*><svg[^>]*>[\s\S]*?Publication calendar/);
    expect(markup).not.toContain("Private visibility is required for");
  });
});
