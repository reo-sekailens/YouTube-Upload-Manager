import { describe, expect, it, vi } from "vitest";
import { refreshChannelLibrary, refreshDedupeInventory } from "./provider-library-actions";
import type { DashboardSnapshot } from "./types";

const snapshot: DashboardSnapshot = { revision: 1, activeChannelId: "a", items: [], duplicates: [], pendingTitleDuplicates: [] };
describe("lazy explicit library refresh", () => {
  it("retains successful sync, snapshot, connection acceptance and refresh notification", async () => {
    const settings = { connected: true, activeChannelId: "a", connectionRevision: "current" };
    const sync = vi.fn().mockResolvedValue(1); const refresh = vi.fn().mockResolvedValue(snapshot);
    const connection = vi.fn().mockResolvedValue(settings); const accept = vi.fn(); const invalidate = vi.fn(); const notice = vi.fn();
    await refreshChannelLibrary({ channel: "Demo", current: () => true, sync, refresh, connection, accept, invalidate, notice });
    expect(sync).toHaveBeenCalledOnce(); expect(refresh).toHaveBeenCalledOnce(); expect(connection).toHaveBeenCalledOnce();
    expect(accept).toHaveBeenCalledExactlyOnceWith(settings); expect(invalidate).toHaveBeenCalledOnce();
    expect(notice).toHaveBeenLastCalledWith("Library refreshed: 1 YouTube video saved locally for Demo. Eligible uploads resume automatically.");
  });
  it("stops a late library sync before accepting a different connection", async () => {
    let resolve!: (count: number) => void; let active = true;
    const sync = vi.fn(() => new Promise<number>((done) => { resolve = done; }));
    const refresh = vi.fn(); const connection = vi.fn(); const accept = vi.fn(); const invalidate = vi.fn(); const notice = vi.fn();
    const pending = refreshChannelLibrary({ channel: "Demo", current: () => active, sync, refresh, connection, accept, invalidate, notice });
    active = false; resolve(4); await pending;
    expect(refresh).not.toHaveBeenCalled(); expect(connection).not.toHaveBeenCalled();
    expect(accept).not.toHaveBeenCalled(); expect(invalidate).not.toHaveBeenCalled(); expect(notice).toHaveBeenCalledOnce();
  });
});
describe("lazy duplicate library refresh", () => {
  it("retains the normal provider sync, local rebuild and completion messages", async () => {
    const sync = vi.fn().mockResolvedValue(1); const refresh = vi.fn().mockResolvedValue(snapshot);
    const phase = vi.fn(); const log = vi.fn(); const notice = vi.fn();
    await refreshDedupeInventory({ channel: "Demo", current: () => true, sync, refresh, phase, log, notice, report: vi.fn() });
    expect(sync).toHaveBeenCalledOnce(); expect(refresh).toHaveBeenCalledOnce();
    expect(phase.mock.calls).toEqual([["rebuilding"], ["complete"]]);
    expect(log).toHaveBeenCalledWith("success", "Synced 1 uploaded video into this device's channel inventory.");
    expect(notice).toHaveBeenLastCalledWith("Dedupe complete for Demo: 1 uploaded video checked and 0 candidates ready for review.");
  });
  it("ignores a late sync after its connection changes and never starts an obsolete workflow", async () => {
    let resolve!: (count: number) => void; let active = true;
    const sync = vi.fn(() => new Promise<number>((done) => { resolve = done; }));
    const refresh = vi.fn(); const phase = vi.fn(); const log = vi.fn(); const notice = vi.fn();
    const context = { channel: "Demo", current: () => active, sync, refresh, phase, log, notice, report: vi.fn() };
    const pending = refreshDedupeInventory(context);
    active = false; resolve(5); await pending;
    expect(refresh).not.toHaveBeenCalled(); expect(phase).not.toHaveBeenCalled();
    expect(log.mock.calls).toHaveLength(2); expect(notice).toHaveBeenCalledOnce();
    await refreshDedupeInventory(context);
    expect(sync).toHaveBeenCalledOnce();
  });
  it("retains provider-failure details and the existing error phase and activity", async () => {
    const phase = vi.fn(); const log = vi.fn(); const notice = vi.fn();
    const report = vi.fn().mockResolvedValue("Current project quota reached");
    const failure = new Error("native quota detail");
    await refreshDedupeInventory({ channel: "Demo", current: () => true,
      sync: vi.fn().mockRejectedValue(failure), refresh: vi.fn(), phase, log, notice, report });
    expect(report).toHaveBeenCalledExactlyOnceWith(failure);
    expect(phase).toHaveBeenLastCalledWith("error");
    expect(log).toHaveBeenLastCalledWith("error", "Dedupe stopped: Current project quota reached");
    expect(notice).toHaveBeenLastCalledWith("Dedupe could not run: Current project quota reached");
  });
});
