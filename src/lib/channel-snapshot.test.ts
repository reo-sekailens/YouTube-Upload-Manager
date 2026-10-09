import { describe, expect, it } from "vitest";
import { canAcceptChannelSnapshot } from "./channel-snapshot";
import type { ChannelSnapshotCursor } from "./channel-snapshot";
import type { DashboardSnapshot } from "./types";

const cursor: ChannelSnapshotCursor = { channelId: "channel-a", generation: 1, revision: 10 };
const snapshot: DashboardSnapshot = {
  activeChannelId: "channel-a", revision: 11, items: [], duplicates: [], pendingTitleDuplicates: [],
};

describe("channel snapshot request fence", () => {
  it("accepts current-channel catch-up and equal revision snapshots", () => {
    expect(canAcceptChannelSnapshot(cursor, cursor, snapshot)).toBe(true);
    expect(canAcceptChannelSnapshot(cursor, cursor, { ...snapshot, revision: 10 })).toBe(true);
  });

  it("rejects an old-channel response after an account switch or disconnect", () => {
    expect(canAcceptChannelSnapshot(cursor, { ...cursor, channelId: "channel-b", generation: 2 }, snapshot)).toBe(false);
    expect(canAcceptChannelSnapshot(cursor, { generation: 2, revision: 0 }, snapshot)).toBe(false);
  });

  it("rejects a response from before disconnect and reconnect to the same channel", () => {
    expect(canAcceptChannelSnapshot(cursor, { ...cursor, generation: 3 }, snapshot)).toBe(false);
  });

  it("rejects mismatched provider projections and revision regressions", () => {
    expect(canAcceptChannelSnapshot(cursor, cursor, { ...snapshot, activeChannelId: "channel-b" })).toBe(false);
    expect(canAcceptChannelSnapshot(cursor, cursor, { ...snapshot, revision: 9 })).toBe(false);
  });

  it("allows device-only snapshots only while the disconnected generation is current", () => {
    const disconnected = { generation: 2, revision: 0 };
    expect(canAcceptChannelSnapshot(disconnected, disconnected, { ...snapshot, activeChannelId: undefined, revision: 0 })).toBe(true);
    expect(canAcceptChannelSnapshot(disconnected, cursor, { ...snapshot, activeChannelId: undefined, revision: 0 })).toBe(false);
  });
});
