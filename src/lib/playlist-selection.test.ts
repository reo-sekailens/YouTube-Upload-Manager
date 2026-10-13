import { describe, expect, it } from "vitest";
import type { PlaylistAddReceipt } from "./types";
import { addPlaylistSelection, isPlaylistAddReceiptForSelection, PLAYLIST_SELECTION_LIMIT, playlistRangeIds } from "./playlist-selection";

describe("bulk playlist selection", () => {
  it("adds another filtered page without discarding selections from other searches", () => {
    const previous = new Set(["outside-search", "earlier-page"]);
    const result = addPlaylistSelection(previous, ["next-page-a", "next-page-b"]);
    expect([...result.selected]).toEqual(["outside-search", "earlier-page", "next-page-a", "next-page-b"]);
    expect([...previous]).toEqual(["outside-search", "earlier-page"]);
    expect(result.omitted).toBe(0);
  });

  it("stops a large matching selection at 200 in the reviewed display order", () => {
    const matching = Array.from({ length: 245 }, (_, index) => `video-${index}`);
    const result = addPlaylistSelection(new Set(), matching);
    expect([...result.selected]).toEqual(matching.slice(0, PLAYLIST_SELECTION_LIMIT));
    expect(result.omitted).toBe(45);
  });

  it("does not count existing matches or duplicate IDs against remaining capacity", () => {
    const previous = new Set(Array.from({ length: 199 }, (_, index) => `selected-${index}`));
    const result = addPlaylistSelection(previous, ["selected-0", "new-a", "new-a", "new-b", "new-b"]);
    expect(result.selected.size).toBe(200);
    expect(result.selected.has("new-a")).toBe(true);
    expect(result.selected.has("new-b")).toBe(false);
    expect(result.omitted).toBe(1);
  });

  it("selects a Shift range across page boundaries using the complete filtered order", () => {
    const matching = Array.from({ length: 70 }, (_, index) => `video-${index}`);
    expect(playlistRangeIds(matching, "video-30", "video-36")).toEqual(matching.slice(30, 37));
  });

  it("handles reversed display order and backward ranges inclusively", () => {
    expect(playlistRangeIds(["d", "c", "b", "a"], "a", "c")).toEqual(["c", "b", "a"]);
  });

  it("uses only the clicked target when the anchor is absent from the current filters", () => {
    expect(playlistRangeIds(["a", "b", "c"], "hidden-anchor", "b")).toEqual(["b"]);
    expect(playlistRangeIds(["a", "b", "c"], undefined, "b")).toEqual(["b"]);
    expect(playlistRangeIds(["a", "b", "c"], "a", "unknown")).toEqual([]);
  });
});

describe("playlist addition receipt acceptance", () => {
  function receipt(): PlaylistAddReceipt {
    return { channelId: "channel-a", playlistId: "playlist-a", requestedVideoIds: ["a", "b", "c"], addedVideoIds: ["a"], skippedVideoIds: ["b"], remainingVideoIds: ["c"], error: "Please retry", uncertain: true };
  }
  const accepts = (value: PlaylistAddReceipt) => isPlaylistAddReceiptForSelection(value, "channel-a", "playlist-a", ["a", "b", "c"]);

  it("accepts a confirmed partial receipt with explicit unresolved selections", () => {
    expect(accepts(receipt())).toBe(true);
  });

  it("rejects another channel, destination, or reviewed request order", () => {
    expect(accepts({ ...receipt(), channelId: "channel-b" })).toBe(false);
    expect(accepts({ ...receipt(), playlistId: "playlist-b" })).toBe(false);
    expect(accepts({ ...receipt(), requestedVideoIds: ["b", "a", "c"] })).toBe(false);
  });

  it("rejects missing, unexpected, or doubly reported IDs before clearing selection", () => {
    expect(accepts({ ...receipt(), remainingVideoIds: [] })).toBe(false);
    expect(accepts({ ...receipt(), remainingVideoIds: ["unknown"] })).toBe(false);
    expect(accepts({ ...receipt(), skippedVideoIds: ["a"] })).toBe(false);
    expect(accepts({ ...receipt(), addedVideoIds: ["a", "a"] })).toBe(false);
  });

  it("accepts completion only when every reviewed video has a confirmed outcome", () => {
    expect(accepts({ ...receipt(), error: undefined, uncertain: false, addedVideoIds: ["a", "c"], remainingVideoIds: [] })).toBe(true);
  });
});
