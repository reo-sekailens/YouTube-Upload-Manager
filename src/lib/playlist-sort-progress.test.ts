import { describe, expect, it } from "vitest";
import { isPlaylistSortProgressForScope, type PlaylistSortProgress } from "./playlist-sort-progress";

const progress: PlaylistSortProgress = {
  channelId: "channel-a", playlistId: "playlist-a", completed: 1, total: 2,
  currentTitle: "Private channel A video", status: "running", detail: "Order verified.",
};

describe("playlist sort progress isolation", () => {
  it("accepts matching progress and rejects another channel or selected playlist", () => {
    expect(isPlaylistSortProgressForScope(progress, "channel-a", "playlist-a")).toBe(true);
    expect(isPlaylistSortProgressForScope(progress, "channel-b", "playlist-a")).toBe(false);
    expect(isPlaylistSortProgressForScope(progress, "channel-a", "playlist-b")).toBe(false);
  });

  it("rejects unscoped legacy events and malformed progress without showing old titles", () => {
    expect(isPlaylistSortProgressForScope(null, "channel-a", "playlist-a")).toBe(false);
    expect(isPlaylistSortProgressForScope({ ...progress, channelId: undefined } as unknown as PlaylistSortProgress, "channel-a", "playlist-a")).toBe(false);
    expect(isPlaylistSortProgressForScope({ ...progress, completed: 3 }, "channel-a", "playlist-a")).toBe(false);
    expect(isPlaylistSortProgressForScope({ ...progress, total: Number.NaN }, "channel-a", "playlist-a")).toBe(false);
  });
});
