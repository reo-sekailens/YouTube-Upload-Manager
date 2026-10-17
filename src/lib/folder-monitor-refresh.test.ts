import { afterEach, describe, expect, it, vi } from "vitest";
import { createFolderMonitorRefreshQueue } from "./folder-monitor-refresh";
import { createWorkspaceRequestFence } from "./workspace-requests";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

afterEach(() => { vi.useRealTimers(); });

describe("watched-folder background refresh queue", () => {
  it("loads an initial scope immediately, then coalesces an event burst", async () => {
    vi.useFakeTimers();
    const read = vi.fn(async () => undefined);
    const queue = createFolderMonitorRefreshQueue(read, vi.fn());
    queue.start();
    expect(read).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(0);
    for (let index = 0; index < 100; index += 1) queue.request();
    await vi.advanceTimersByTimeAsync(249);
    expect(read).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(read).toHaveBeenCalledTimes(2);
    queue.dispose();
  });

  it("does not postpone the first refresh when events continue arriving", async () => {
    vi.useFakeTimers();
    const read = vi.fn(async () => undefined);
    const queue = createFolderMonitorRefreshQueue(read, vi.fn());
    queue.request();
    for (let index = 0; index < 4; index += 1) {
      await vi.advanceTimersByTimeAsync(50);
      queue.request();
    }
    expect(read).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(50);
    expect(read).toHaveBeenCalledTimes(1);
    queue.dispose();
  });

  it("keeps the current result and follows it with one serial refresh", async () => {
    vi.useFakeTimers();
    const first = deferred();
    const applied: number[] = [];
    let calls = 0;
    const read = vi.fn(async () => {
      const call = ++calls;
      if (call === 1) await first.promise;
      applied.push(call);
    });
    const queue = createFolderMonitorRefreshQueue(read, vi.fn());
    queue.start();
    for (let index = 0; index < 100; index += 1) queue.request();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(read).toHaveBeenCalledTimes(1);
    first.resolve();
    await vi.advanceTimersByTimeAsync(0);
    expect(applied).toEqual([1]);
    await vi.advanceTimersByTimeAsync(250);
    expect(applied).toEqual([1, 2]);
    expect(read).toHaveBeenCalledTimes(2);
    queue.dispose();
  });

  it("releases a failed read so a later event can recover", async () => {
    vi.useFakeTimers();
    const failure = new Error("Native read temporarily unavailable");
    const onFailure = vi.fn();
    const read = vi.fn().mockRejectedValueOnce(failure).mockResolvedValue(undefined);
    const queue = createFolderMonitorRefreshQueue(read, onFailure);
    queue.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(onFailure).toHaveBeenCalledWith(failure);
    queue.request();
    await vi.advanceTimersByTimeAsync(250);
    expect(read).toHaveBeenCalledTimes(2);
    queue.dispose();
  });

  it("cancels a scheduled refresh and ignores further requests after disposal", async () => {
    vi.useFakeTimers();
    const read = vi.fn(async () => undefined);
    const queue = createFolderMonitorRefreshQueue(read, vi.fn());
    queue.request();
    queue.dispose();
    queue.start();
    queue.request();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(read).not.toHaveBeenCalled();
  });

  it("never carries a pending refresh or failure into a replacement scope", async () => {
    vi.useFakeTimers();
    const first = deferred();
    const onFailure = vi.fn();
    const read = vi.fn(() => first.promise.then(() => { throw new Error("Old scope failed"); }));
    const queue = createFolderMonitorRefreshQueue(read, onFailure);
    queue.start();
    queue.request();
    queue.dispose();
    first.resolve();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(read).toHaveBeenCalledTimes(1);
    expect(onFailure).not.toHaveBeenCalled();
  });

  it.each(["channel-b/folder-a", "channel-a/folder-b"])("fences an old read when switching to %s", async (nextScope) => {
    vi.useFakeTimers();
    const first = deferred();
    const fence = createWorkspaceRequestFence("channel-a/folder-a");
    const current = fence.begin();
    const applied: string[] = [];
    const queue = createFolderMonitorRefreshQueue(async () => {
      await first.promise;
      if (current()) applied.push("old scope");
    }, vi.fn());
    queue.start();
    queue.request();
    queue.dispose();
    fence.setScope(nextScope);
    const nextCurrent = fence.begin();
    first.resolve();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(applied).toEqual([]);
    expect(nextCurrent()).toBe(true);
  });
});
