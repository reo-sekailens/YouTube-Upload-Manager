import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

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
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });

  it("passes only the selected Desktop OAuth JSON path to Rust", async () => {
    const { importDesktopOAuthClient } = await import("./local");
    await importDesktopOAuthClient("C:\\Secrets\\desktop-client.json");

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

    await enableFolderMonitor(path, "unlisted", true, false, "playlist-1", "Episodes");

    expect(invoke).toHaveBeenCalledWith("enable_folder_monitor", { path, visibility: "unlisted", madeForKids: true, deleteSourceAfterUpload: false, playlistId: "playlist-1", playlistTitle: "Episodes" });
  });

  it("uses narrow native commands for disable and operator-requested scans", async () => {
    const { disableFolderMonitor, processExistingFolderFiles, scanFolderMonitorNow } = await import("./local");

    await disableFolderMonitor();
    await scanFolderMonitorNow();
    await processExistingFolderFiles();

    expect(invoke).toHaveBeenNthCalledWith(1, "disable_folder_monitor");
    expect(invoke).toHaveBeenNthCalledWith(2, "scan_folder_monitor_now");
    expect(invoke).toHaveBeenNthCalledWith(3, "process_existing_folder_files");
  });

  it("returns a safe disabled state in browser preview mode", async () => {
    const { loadFolderMonitorSettings } = await import("./local");

    await expect(loadFolderMonitorSettings()).resolves.toMatchObject({
      enabled: false,
      status: "disabled",
    });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("loads the live folder overview through one native command", async () => {
    isTauri.mockReturnValue(true);
    vi.resetModules();
    const { loadFolderMonitorOverview } = await import("./local");

    await loadFolderMonitorOverview();

    expect(invoke).toHaveBeenCalledWith("load_folder_monitor_overview");
  });
});

describe("confirmed application exit", () => {
  beforeEach(() => {
    invoke.mockReset();
    isTauri.mockReturnValue(true);
    vi.resetModules();
  });

  afterEach(() => {
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });

  it("uses the native application exit boundary after confirmation", async () => {
    const { exitApplication } = await import("./local");

    await exitApplication();

    expect(invoke).toHaveBeenCalledWith("exit_application");
  });
});

describe("duplicate scan command", () => {
  beforeEach(() => {
    invoke.mockReset();
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });

  afterEach(() => {
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });

  it("synchronizes the active channel inventory through the native boundary", async () => {
    const { syncChannelInventory } = await import("./local");

    await syncChannelInventory();

    expect(invoke).toHaveBeenCalledWith("sync_channel_inventory");
  });

  it("keeps ignored-match decisions and re-audit requests in the native layer", async () => {
    const { ignoreDuplicateCandidate, reAuditIgnoredDuplicateCandidates } = await import("./local");

    await ignoreDuplicateCandidate("remote:video-a:video-b");
    await reAuditIgnoredDuplicateCandidates();

    expect(invoke).toHaveBeenNthCalledWith(1, "ignore_duplicate_candidate", { candidateId: "remote:video-a:video-b" });
    expect(invoke).toHaveBeenNthCalledWith(2, "re_audit_ignored_duplicate_candidates");
  });

  it("keeps arbitrary pre-ingest file paths at the native duplicate-check boundary", async () => {
    isTauri.mockReturnValue(true);
    vi.resetModules();
    const { preflightDuplicateFiles } = await import("./local");
    const paths = ["C:\\Camera\\clip.insv", "C:\\Camera\\preview.lrv"];

    await preflightDuplicateFiles(paths);

    expect(invoke).toHaveBeenCalledWith("start_preflight_duplicate_files", { paths, mode: "light" });
  });

  it("requests resource-intensive hashing only when the operator selects deep matching", async () => {
    isTauri.mockReturnValue(true);
    vi.resetModules();
    const { preflightDuplicateFiles } = await import("./local");

    await preflightDuplicateFiles(["C:\\Camera\\clip.insv"], "deep");

    expect(invoke).toHaveBeenCalledWith("start_preflight_duplicate_files", { paths: ["C:\\Camera\\clip.insv"], mode: "deep" });
  });

  it("keeps the opaque local-delete token and filename confirmation in the native boundary", async () => {
    isTauri.mockReturnValue(true);
    vi.resetModules();
    const { deletePreflightDuplicateFile } = await import("./local");

    await deletePreflightDuplicateFile("opaque-review-token", "duplicate.insv");

    expect(invoke).toHaveBeenCalledWith("delete_preflight_duplicate_file", {
      token: "opaque-review-token",
      confirmation: "duplicate.insv",
    });
  });

  it("revalidates a light or deep local match natively before showing deletion confirmation", async () => {
    isTauri.mockReturnValue(true);
    vi.resetModules();
    const { preparePreflightLocalDeleteFile } = await import("./local");

    await preparePreflightLocalDeleteFile("scan-1", 2);

    expect(invoke).toHaveBeenCalledWith("prepare_preflight_local_delete_file", {
      jobId: "scan-1",
      ordinal: 2,
    });
  });
});

describe("portable metadata transfer commands", () => {
  beforeEach(() => { invoke.mockReset(); });

  it("keeps archive paths at the native export and import boundaries", async () => {
    const { exportPortableArchive, importPortableArchive } = await import("./local");
    await exportPortableArchive("C:\\Exports\\dedupe.yumx.gz");
    await importPortableArchive("C:\\Imports\\dedupe.yumx.gz");

    expect(invoke).toHaveBeenNthCalledWith(1, "export_portable_archive", { path: "C:\\Exports\\dedupe.yumx.gz" });
    expect(invoke).toHaveBeenNthCalledWith(2, "import_portable_archive", { path: "C:\\Imports\\dedupe.yumx.gz" });
  });
});

describe("persisted-work cancellation commands", () => {
  beforeEach(() => { invoke.mockReset(); });

  it("keeps queue and job cancellation decisions in the native layer", async () => {
    const { cancelPreflightDuplicateScan, clearDeletionRequests, clearUploadQueue } = await import("./local");
    await cancelPreflightDuplicateScan("scan-1");
    await clearUploadQueue();
    await clearDeletionRequests();

    expect(invoke).toHaveBeenNthCalledWith(1, "cancel_preflight_duplicate_scan", { jobId: "scan-1" });
    expect(invoke).toHaveBeenNthCalledWith(2, "clear_upload_queue");
    expect(invoke).toHaveBeenNthCalledWith(3, "clear_deletion_requests");
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

describe("manual upload source cleanup command", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("keeps the per-upload cleanup choice in the native boundary", async () => {
    const { setItemDeleteSourceAfterUpload } = await import("./local");

    await setItemDeleteSourceAfterUpload("item-1", true);

    expect(invoke).toHaveBeenCalledWith("set_item_delete_source_after_upload", {
      id: "item-1",
      deleteSourceAfterUpload: true,
    });
  });
});

describe("manual upload intake settings", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("keeps required audience, visibility, and selected-playlist metadata on the native import boundary", async () => {
    const { importAsset } = await import("./local");
    const settings = { madeForKids: false, visibility: "unlisted" as const, playlistId: "playlist-1", playlistTitle: "Review queue", deleteSourceAfterUpload: false };

    await importAsset("C:\\Media\\review.mp4", settings);

    expect(invoke).toHaveBeenCalledWith("import_asset", { path: "C:\\Media\\review.mp4", settings });
  });

  it("creates a playlist only through the native command boundary", async () => {
    isTauri.mockReturnValue(true);
    vi.resetModules();
    const { createYouTubePlaylist } = await import("./local");

    await createYouTubePlaylist("New uploads");

    expect(invoke).toHaveBeenCalledWith("create_youtube_playlist", {
      title: "New uploads",
    });
    isTauri.mockReturnValue(false);
    vi.resetModules();
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
