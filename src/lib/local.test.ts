import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const isTauri = vi.fn(() => false);

vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri }));

describe("native OAuth client configuration", () => {
  beforeAll(() => {
    vi.stubGlobal("window", {});
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    invoke.mockReset();
  });

  it("uses Tauri's camel-cased Rust command argument", async () => {
    const { importDesktopOAuthClient, saveOAuthClientId } = await import("./local");
    const clientId = "12345-example.apps.googleusercontent.com";

    await saveOAuthClientId(clientId);
    await importDesktopOAuthClient("C:\\Secrets\\desktop-client.json");

    expect(invoke).toHaveBeenCalledWith("save_oauth_client_id", { oauthClientId: clientId });
    expect(invoke).toHaveBeenCalledWith("import_desktop_oauth_client", { path: "C:\\Secrets\\desktop-client.json" });
  });
});

describe("folder monitor commands", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes the operator-selected directory and safe visibility to the native enable command", async () => {
    const { enableFolderMonitor } = await import("./local");
    const path = "C:\\Media\\Ready";

    await enableFolderMonitor(path, "unlisted");

    expect(invoke).toHaveBeenCalledWith("enable_folder_monitor", { path, visibility: "unlisted" });
  });

  it("uses narrow native commands for disable and operator-requested scans", async () => {
    const { disableFolderMonitor, scanFolderMonitorNow } = await import("./local");

    await disableFolderMonitor();
    await scanFolderMonitorNow();

    expect(invoke).toHaveBeenNthCalledWith(1, "disable_folder_monitor");
    expect(invoke).toHaveBeenNthCalledWith(2, "scan_folder_monitor_now");
  });

  it("returns a safe disabled state in browser preview mode", async () => {
    const { loadFolderMonitorSettings } = await import("./local");

    await expect(loadFolderMonitorSettings()).resolves.toMatchObject({
      enabled: false,
      status: "disabled",
    });
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("duplicate scan command", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("synchronizes the active channel inventory through the native boundary", async () => {
    const { syncChannelInventory } = await import("./local");

    await syncChannelInventory();

    expect(invoke).toHaveBeenCalledWith("sync_channel_inventory");
  });
});

describe("manual upload visibility command", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("passes the item visibility choice using Tauri camel-cased command arguments", async () => {
    const { setItemVisibility } = await import("./local");

    await setItemVisibility("item-1", "unlisted");

    expect(invoke).toHaveBeenCalledWith("set_item_visibility", {
      id: "item-1",
      visibility: "unlisted",
    });
  });
});

describe("manual upload intake settings", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("keeps required audience, visibility, and selected-playlist metadata on the native import boundary", async () => {
    const { importAsset } = await import("./local");
    const settings = { madeForKids: false, visibility: "unlisted" as const, playlistId: "playlist-1", playlistTitle: "Review queue" };

    await importAsset("C:\\Media\\review.mp4", settings);

    expect(invoke).toHaveBeenCalledWith("import_asset", { path: "C:\\Media\\review.mp4", settings });
  });
});

describe("device-wide manual audience default", () => {
  beforeEach(() => { invoke.mockReset(); });

  it("keeps a safe preview fallback and saves the made-for-kids default through a native command", async () => {
    const { loadManualUploadDefaults, saveManualUploadDefaults } = await import("./local");
    await expect(loadManualUploadDefaults()).resolves.toEqual({ madeForKids: false });
    await saveManualUploadDefaults(true);

    expect(invoke).toHaveBeenCalledWith("save_manual_upload_defaults", { madeForKids: true });
  });
});
