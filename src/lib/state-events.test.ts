import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot, StateChangeBatch, UploadItem } from "./types";

const listen = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({ listen }));

const upload = (id: string, confirmedBytes = 0): UploadItem => ({
  id,
  title: `Upload ${id}`,
  fileName: `${id}.mp4`,
  sizeBytes: 100,
  status: confirmedBytes > 0 ? "uploading" : "queued",
  confirmedBytes,
  totalBytes: 100,
  visibility: "private",
  madeForKids: false,
  updatedAt: `2026-08-23T00:00:0${confirmedBytes > 0 ? "2" : "1"}Z`,
});

const snapshot = (revision = 0): DashboardSnapshot => ({
  activeChannel: "Channel A",
  activeChannelId: "channel-a",
  revision,
  items: [upload("existing")],
  duplicates: [],
  pendingTitleDuplicates: [],
});

describe("revisioned state reducer", () => {
  it("incrementally upserts and removes uploads in revision order", async () => {
    const { reduceDashboardState } = await import("./state-events");
    const batch: StateChangeBatch = {
      fromRevision: 0,
      toRevision: 2,
      resetRequired: false,
      changes: [
        {
          revision: 1,
          channelId: "channel-a",
          surface: "upload",
          entityId: "existing",
          eventKind: "upsert",
          payload: upload("existing", 50),
        },
        {
          revision: 2,
          channelId: "channel-a",
          surface: "upload",
          entityId: "existing",
          eventKind: "delete",
        },
      ],
    };

    const reduced = reduceDashboardState(snapshot(), "channel-a", batch);

    expect(reduced.snapshot.revision).toBe(2);
    expect(reduced.snapshot.items).toEqual([]);
    expect(reduced.invalidatedSurfaces).toEqual([]);
  });

  it("rejects stale, cross-channel, reset, and mismatched-cursor envelopes", async () => {
    const { classifyStateChangeBatch } = await import("./state-events");
    const change = {
      revision: 3,
      channelId: "channel-a",
      surface: "upload",
      entityId: "existing",
      eventKind: "upsert",
      payload: upload("existing", 50),
    };

    expect(
      classifyStateChangeBatch(3, "channel-a", {
        fromRevision: 2,
        toRevision: 3,
        resetRequired: false,
        changes: [change],
      }).disposition,
    ).toBe("stale");
    expect(
      classifyStateChangeBatch(2, "channel-b", {
        fromRevision: 2,
        toRevision: 3,
        resetRequired: false,
        changes: [change],
      }).disposition,
    ).toBe("cross_channel");
    expect(
      classifyStateChangeBatch(2, "channel-a", {
        fromRevision: 2,
        toRevision: 3,
        resetRequired: true,
        changes: [change],
      }).disposition,
    ).toBe("reset");
    expect(
      classifyStateChangeBatch(1, "channel-a", {
        fromRevision: 0,
        toRevision: 3,
        resetRequired: false,
        changes: [change],
      }).disposition,
    ).toBe("gap");
  });

  it("accepts coalesced uploads across interleaved global revisions", async () => {
    const { classifyStateChangeBatch, reduceDashboardState } = await import("./state-events");
    const batch: StateChangeBatch = {
      fromRevision: 10,
      toRevision: 13,
      resetRequired: false,
      changes: [
        // Revision 11 was an older same-item upload update and revision 12
        // belongs to another channel. Both are intentionally absent after
        // native channel filtering and coalescing.
        {
          revision: 13,
          channelId: "channel-a",
          surface: "upload",
          entityId: "existing",
          eventKind: "upsert",
          payload: upload("existing", 75),
        },
      ],
    };

    expect(classifyStateChangeBatch(10, "channel-a", batch).disposition).toBe("apply");
    const reduced = reduceDashboardState(snapshot(10), "channel-a", batch);
    expect(reduced.snapshot.revision).toBe(13);
    expect(reduced.snapshot.items[0].confirmedBytes).toBe(75);
  });

  it("accepts only channel-free disconnect invalidations", async () => {
    const { normalizeStateChangeBatch } = await import("./state-events");

    expect(
      normalizeStateChangeBatch({
        fromRevision: 7,
        toRevision: 8,
        resetRequired: false,
        changes: [
          {
            revision: 8,
            channelId: "",
            surface: "connection",
            entityId: "connection",
            eventKind: "invalidate",
            payload: {},
          },
        ],
      }),
    ).toBeDefined();
    expect(
      normalizeStateChangeBatch({
        revision: 8,
        channelId: "",
        surface: "upload",
        entityId: "item-a",
        eventKind: "upsert",
      }),
    ).toBeUndefined();
  });

  it("never mixes an event into a snapshot owned by another channel", async () => {
    const { reduceDashboardState } = await import("./state-events");
    const current = snapshot(4);
    const result = reduceDashboardState(current, "channel-b", {
      fromRevision: 4,
      toRevision: 5,
      resetRequired: false,
      changes: [
        {
          revision: 5,
          channelId: "channel-b",
          surface: "upload",
          entityId: "other",
          eventKind: "upsert",
          payload: upload("other", 10),
        },
      ],
    });

    expect(result.snapshot).toBe(current);
    expect(result.snapshot.items.map((item) => item.id)).toEqual(["existing"]);
  });
});

describe("shared local state event subscription", () => {
  beforeEach(() => {
    listen.mockReset();
  });

  afterEach(async () => {
    const { resetStateChangeSubscriptionForTests } = await import("./state-events");
    resetStateChangeSubscriptionForTests();
  });

  it("installs one native listener for concurrent workspace subscribers", async () => {
    let nativeHandler: ((event: { payload: unknown }) => void) | undefined;
    const nativeUnlisten = vi.fn();
    listen.mockImplementation(
      async (_name: string, handler: (event: { payload: unknown }) => void) => {
        nativeHandler = handler;
        return nativeUnlisten;
      },
    );
    const { subscribeLocalStateChanges } = await import("./state-events");
    const first = vi.fn();
    const second = vi.fn();

    const [unsubscribeFirst, unsubscribeSecond] = await Promise.all([
      subscribeLocalStateChanges(first),
      subscribeLocalStateChanges(second),
    ]);
    nativeHandler?.({
      payload: {
        revision: 1,
        channelId: "channel-a",
        surface: "upload",
        entityId: "existing",
        eventKind: "upsert",
        payload: upload("existing", 25),
      },
    });

    expect(listen).toHaveBeenCalledTimes(1);
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    unsubscribeFirst();
    expect(nativeUnlisten).not.toHaveBeenCalled();
    unsubscribeSecond();
    expect(nativeUnlisten).toHaveBeenCalledTimes(1);
  });

  it("ignores malformed native payloads", async () => {
    let nativeHandler: ((event: { payload: unknown }) => void) | undefined;
    listen.mockImplementation(
      async (_name: string, handler: (event: { payload: unknown }) => void) => {
        nativeHandler = handler;
        return vi.fn();
      },
    );
    const { subscribeLocalStateChanges } = await import("./state-events");
    const subscriber = vi.fn();
    const unsubscribe = await subscribeLocalStateChanges(subscriber);

    nativeHandler?.({ payload: { revision: "not-a-revision" } });

    expect(subscriber).not.toHaveBeenCalled();
    unsubscribe();
  });
});
