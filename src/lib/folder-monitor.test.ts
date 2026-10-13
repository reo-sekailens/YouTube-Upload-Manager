import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.isTauri }));

describe("independent folder-monitor command scopes", () => {
  beforeEach(() => { mocks.invoke.mockReset(); mocks.isTauri.mockReturnValue(true); });
  afterEach(() => { mocks.isTauri.mockReturnValue(true); });

  it("loads the connected channel collection and one chosen folder separately", async () => {
    const { loadFolderMonitors, loadFolderMonitorOverview } = await import("./folder-monitor");
    await loadFolderMonitors("channel-a");
    await loadFolderMonitorOverview("folder-a", "channel-a");
    expect(mocks.invoke.mock.calls).toEqual([
      ["load_folder_monitors", { expectedChannelId: "channel-a" }],
      ["load_folder_monitor_overview", { monitorId: "folder-a", expectedChannelId: "channel-a" }],
    ]);
  });

  it("adds a new folder without targeting an existing record and edits only an explicit ID", async () => {
    const { enableFolderMonitor } = await import("./folder-monitor");
    await enableFolderMonitor("C:/Fixture/One", "private", false, false, undefined, undefined, undefined, "channel-a");
    await enableFolderMonitor("C:/Fixture/Two", "unlisted", true, false, "playlist-a", "Archive", "folder-b", "channel-a");
    expect(mocks.invoke.mock.calls[0][1]).not.toHaveProperty("monitorId");
    expect(mocks.invoke.mock.calls[0][1]).toMatchObject({ path: "C:/Fixture/One", expectedChannelId: "channel-a" });
    expect(mocks.invoke.mock.calls[1][1]).toMatchObject({ monitorId: "folder-b", expectedChannelId: "channel-a", path: "C:/Fixture/Two" });
  });

  it("keeps selected file IDs and monitor/channel fences for retries and cleanup", async () => {
    const { requeueCancelledFolderMonitorFiles, deleteFolderMonitorUploadedSources } = await import("./folder-monitor");
    await requeueCancelledFolderMonitorFiles(["item-a"], "folder-a", "channel-a");
    await deleteFolderMonitorUploadedSources(["item-a"], "DELETE LOCAL WATCHED FILES", "folder-a", "channel-a");
    expect(mocks.invoke.mock.calls).toEqual([
      ["requeue_cancelled_folder_monitor_files", { itemIds: ["item-a"], monitorId: "folder-a", expectedChannelId: "channel-a" }],
      ["delete_folder_monitor_uploaded_sources", { itemIds: ["item-a"], confirmation: "DELETE LOCAL WATCHED FILES", monitorId: "folder-a", expectedChannelId: "channel-a" }],
    ]);
  });

  it("does not offer configured folders or source activity in browser mode", async () => {
    mocks.isTauri.mockReturnValue(false);
    const { loadFolderMonitors, loadFolderMonitorOverview } = await import("./folder-monitor");
    await expect(loadFolderMonitors("channel-a")).resolves.toEqual({ monitors: [] });
    const overview = await loadFolderMonitorOverview("folder-a", "channel-a");
    expect(overview.settings.enabled).toBe(false);
    expect(overview.files).toEqual([]);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
