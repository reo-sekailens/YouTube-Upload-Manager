import { describe, expect, it } from "vitest";
import { isPlaylistAddProgressForScope } from "./playlist-add-progress";

const event = { channelId: "channel", playlistId: "playlist", requestId: "request", total: 5, completed: 3, added: 2, skipped: 1, currentTitle: "Clip", status: "running", detail: "Membership confirmed." };
const check = (payload: unknown) => isPlaylistAddProgressForScope(payload, "channel", "playlist", "request");
describe("playlist addition progress", () => {
  it("accepts real confirmed item counts for the exact operation", () => expect(check(event)).toBe(true));
  it("rejects foreign and stale operation contexts", () => {
    for (const field of ["channelId", "playlistId", "requestId"]) expect(check({ ...event, [field]: "another" })).toBe(false);
    expect(check({ ...event, requestId: undefined })).toBe(false);
  });
  it("rejects impossible counts and malformed statuses", () => {
    for (const change of [{ completed: 6 }, { added: 3 }, { skipped: -1 }, { total: 201 }, { completed: NaN }, { status: "unknown" }, { detail: undefined }]) expect(check({ ...event, ...change })).toBe(false);
    expect(check(null)).toBe(false);
  });
});
