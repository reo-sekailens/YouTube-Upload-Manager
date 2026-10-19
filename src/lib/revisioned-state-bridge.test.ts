import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionSettings, DashboardSnapshot, StateChangeBatch } from "./types";

const mocks = vi.hoisted(() => ({
  connection: vi.fn(), snapshot: vi.fn(), changes: vi.fn(), subscribe: vi.fn(),
}));
vi.mock("./local", () => ({
  loadConnectionSettings: mocks.connection,
  loadSnapshot: mocks.snapshot,
  loadStateChanges: mocks.changes,
  loadPreflightDuplicateScan: vi.fn(),
}));
vi.mock("./state-events", async (original) => ({
  ...await original<typeof import("./state-events")>(),
  subscribeLocalStateChanges: mocks.subscribe,
}));
import { installRevisionedStateBridge } from "./revisioned-state-bridge";

const snapshot: DashboardSnapshot = {
  revision: 1, activeChannelId: "channel-a", items: [], duplicates: [], pendingTitleDuplicates: [],
};
const emptyBatch: StateChangeBatch = { fromRevision: 1, toRevision: 1, resetRequired: false, changes: [] };
async function settle() { for (let index = 0; index < 12; index += 1) await Promise.resolve(); }
function pending<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("revision bridge channel switches", () => {
  let listener: (batch: StateChangeBatch) => void;
  let stop: (() => void) | undefined;
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.changes.mockResolvedValue(emptyBatch);
    mocks.subscribe.mockImplementation((callback) => { listener = callback; return Promise.resolve(vi.fn()); });
    vi.stubGlobal("window", { setTimeout: vi.fn(() => 1), clearTimeout: vi.fn() });
  });
  afterEach(() => { stop?.(); vi.unstubAllGlobals(); });

  function install() {
    const channel = { current: "channel-a" as string | undefined };
    const updateConnection = vi.fn();
    const setSnapshot = vi.fn();
    const setConnection = vi.fn();
    stop = installRevisionedStateBridge([
      "channel-a", snapshot, channel, { current: 1 }, setConnection,
      setSnapshot, vi.fn(), vi.fn(), updateConnection, vi.fn(),
    ]);
    return { channel, updateConnection, setSnapshot, setConnection };
  }

  it("ignores connection reads from the old bridge before effect teardown", async () => {
    const reply = pending<{ connected: boolean; activeChannelId: string }>();
    mocks.connection.mockReturnValue(reply.promise);
    const app = install();
    await settle();
    listener({
      fromRevision: 1, toRevision: 2, resetRequired: false,
      changes: [{ revision: 2, channelId: "channel-a", surface: "connection", entityId: "connection", eventKind: "invalidated" }],
    });
    await settle();
    expect(mocks.connection).toHaveBeenCalledOnce();
    app.channel.current = "channel-b";
    reply.resolve({ connected: true, activeChannelId: "channel-a" });
    await settle();
    expect(app.updateConnection).not.toHaveBeenCalled();
  });

  it("discards a pending replacement snapshot when the active channel changes", async () => {
    const reply = pending<DashboardSnapshot>();
    mocks.changes.mockResolvedValue({ ...emptyBatch, resetRequired: true });
    mocks.snapshot.mockReturnValue(reply.promise);
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" });
    const app = install();
    await settle();
    expect(mocks.snapshot).toHaveBeenCalledOnce();
    app.channel.current = undefined;
    reply.resolve({ ...snapshot, revision: 20 });
    await settle();
    expect(app.setSnapshot).not.toHaveBeenCalled();
    expect(app.setConnection).not.toHaveBeenCalled();
    expect(app.updateConnection).not.toHaveBeenCalled();
  });

  it("discards a late same-channel credential reply after the connection-keyed bridge is replaced", async () => {
    const reply = pending<ConnectionSettings>();
    const current: ConnectionSettings = { connected: true, activeChannelId: "channel-a", connectionRevision: "new-binding" };
    mocks.connection.mockReturnValueOnce(reply.promise).mockResolvedValue(current);
    const previous = install();
    await settle();
    const change: StateChangeBatch = {
      fromRevision: 1, toRevision: 2, resetRequired: false,
      changes: [{ revision: 2, channelId: "channel-a", surface: "connection", entityId: "connection", eventKind: "invalidated" }],
    };
    listener(change);
    await settle();
    expect(mocks.connection).toHaveBeenCalledOnce();
    stop?.();
    const replacement = install();
    await settle();
    listener(change);
    await settle();
    expect(replacement.updateConnection).toHaveBeenCalledExactlyOnceWith(current);
    reply.resolve({ ...current, connectionRevision: "old-binding" });
    await settle();
    expect(previous.updateConnection).not.toHaveBeenCalled();
    expect(replacement.updateConnection).toHaveBeenCalledExactlyOnceWith(current);
  });

  it("discards an old same-channel bootstrap pair after credential scope teardown", async () => {
    const reply = pending<DashboardSnapshot>();
    mocks.changes.mockResolvedValue({ ...emptyBatch, resetRequired: true });
    mocks.snapshot.mockReturnValue(reply.promise);
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a", connectionRevision: "old-binding" });
    const previous = install();
    await settle();
    expect(mocks.snapshot).toHaveBeenCalledOnce();
    stop?.();
    reply.resolve({ ...snapshot, revision: 20 });
    await settle();
    expect(previous.setSnapshot).not.toHaveBeenCalled();
    expect(previous.setConnection).not.toHaveBeenCalled();
    expect(previous.updateConnection).not.toHaveBeenCalled();
  });
});
