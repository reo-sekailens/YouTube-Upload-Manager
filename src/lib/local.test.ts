import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const isTauri = vi.fn(() => false);
const convertFileSrc = vi.fn((token: string, protocol: string) => `http://${protocol}.localhost/${token}`);

vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri, convertFileSrc }));

describe("read-only duplicate video previews", () => {
  beforeEach(() => {
    invoke.mockReset();
    convertFileSrc.mockClear();
    isTauri.mockReturnValue(true);
    vi.resetModules();
  });
  afterEach(() => {
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });

  it("resolves an opaque item in its reviewed channel and converts only native tokens", async () => {
    invoke.mockResolvedValue({ localSources: [{ token: "preview-token", title: "Clip", fileName: "clip.mp4" }], youtubeVideos: [{ videoId: "video-1", title: "Clip" }] });
    const { prepareDuplicateMediaPreview } = await import("./duplicate-media-preview");
    const target = { kind: "upload", itemId: "item-1" } as const;
    const evidence = await prepareDuplicateMediaPreview(target, "channel-1");
    expect(invoke).toHaveBeenCalledWith("prepare_duplicate_media_preview", { target, expectedChannelId: "channel-1" });
    expect(convertFileSrc).toHaveBeenCalledWith("preview-token", "media-preview");
    expect(evidence.localSources[0].url).toBe("http://media-preview.localhost/preview-token");
    expect(evidence.youtubeVideos[0].videoId).toBe("video-1");
  });

  it("revokes all selected-source grants and avoids empty release calls", async () => {
    const { releaseDuplicateMediaPreview } = await import("./duplicate-media-preview");
    await releaseDuplicateMediaPreview(["one", "two"]);
    await releaseDuplicateMediaPreview([]);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("release_duplicate_media_preview", { tokens: ["one", "two"] });
  });

  it("does not grant filesystem preview access in browser mode", async () => {
    isTauri.mockReturnValue(false);
    vi.resetModules();
    const { prepareDuplicateMediaPreview } = await import("./duplicate-media-preview");
    await expect(prepareDuplicateMediaPreview({ kind: "preflight", jobId: "scan", ordinal: 0 })).rejects.toThrow("desktop app");
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("native OAuth client configuration", () => {
  afterEach(() => {
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });
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

  it("keeps replacement review and consent bound to the reviewed channel", async () => {
    const { prepareOAuthClientReplacement, beginOAuthClientReplacement, loadOAuthClientReplacementStatus, cancelOAuthClientReplacement } = await import("./local");
    await prepareOAuthClientReplacement("C:/Fixture/client.json", "channel-a");
    await beginOAuthClientReplacement("review-a", "channel-a");
    await loadOAuthClientReplacementStatus("review-a");
    await cancelOAuthClientReplacement("review-a");
    expect(invoke.mock.calls).toEqual([
      ["prepare_oauth_client_replacement", { path: "C:/Fixture/client.json", expectedChannelId: "channel-a" }],
      ["begin_oauth_client_replacement", { reviewId: "review-a", expectedChannelId: "channel-a" }],
      ["load_oauth_client_replacement_status", { reviewId: "review-a" }],
      ["cancel_oauth_client_replacement", { reviewId: "review-a" }],
    ]);
  });

  it("does not invent a provider pause outside the native app", async () => {
    const { loadUploadLimitStatus } = await import("./local");
    await expect(loadUploadLimitStatus("channel-a")).resolves.toEqual({ kind: "none" });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("requests native limit diagnosis with the active channel fence", async () => {
    isTauri.mockReturnValue(true);
    const { loadUploadLimitStatus } = await import("./local");
    await loadUploadLimitStatus("channel-a");
    expect(invoke).toHaveBeenCalledWith("load_upload_limit_status", { expectedChannelId: "channel-a" });
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
    const { disableFolderMonitor, processExistingFolderFiles, requeueCancelledFolderMonitorFiles, scanFolderMonitorNow } = await import("./local");

    await disableFolderMonitor();
    await scanFolderMonitorNow();
    await processExistingFolderFiles();
    await requeueCancelledFolderMonitorFiles(["watched-item-1"]);

    expect(invoke).toHaveBeenNthCalledWith(1, "disable_folder_monitor");
    expect(invoke).toHaveBeenNthCalledWith(2, "scan_folder_monitor_now");
    expect(invoke).toHaveBeenNthCalledWith(3, "process_existing_folder_files");
    expect(invoke).toHaveBeenNthCalledWith(4, "requeue_cancelled_folder_monitor_files", { itemIds: ["watched-item-1"] });
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

describe("two-phase startup commands", () => {
  beforeEach(() => {
    invoke.mockReset();
    isTauri.mockReturnValue(true);
    vi.resetModules();
  });

  afterEach(() => {
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });

  it("loads one coherent bootstrap envelope before completing safe-shell recovery", async () => {
    invoke.mockResolvedValue({});
    const {
      completeStartupAfterSafeShell,
      loadStartupBootstrap,
      primeStartupBootstrap,
    } =
      await import("./local");

    const primed = primeStartupBootstrap();
    const loaded = loadStartupBootstrap();
    expect(loaded).toBe(primed);
    await loaded;
    await completeStartupAfterSafeShell();

    expect(invoke).toHaveBeenNthCalledWith(1, "startup_bootstrap");
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "complete_startup_after_safe_shell",
    );
  });
});

describe("revisioned state catch-up command", () => {
  beforeEach(() => {
    invoke.mockReset();
    isTauri.mockReturnValue(true);
    vi.resetModules();
  });

  afterEach(() => {
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });

  it("keeps immutable channel scope and the last applied revision at the native boundary", async () => {
    const { loadStateChanges } = await import("./local");

    await loadStateChanges("UC-immutable", 42);

    expect(invoke).toHaveBeenCalledWith("load_state_changes", {
      channelId: "UC-immutable",
      afterRevision: 42,
    });
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

  it("loads bounded preflight pages, compact status, and one expanded metadata record independently", async () => {
    isTauri.mockReturnValue(true);
    vi.resetModules();
    const {
      loadPreflightDuplicateFileMetadata,
      loadPreflightDuplicateScan,
      loadPreflightDuplicateScanStatus,
    } = await import("./local");

    await loadPreflightDuplicateScan("scan-1", {
      fileOffset: 96,
      fileLimit: 48,
      activityOffset: 48,
      activityLimit: 48,
    });
    await loadPreflightDuplicateScanStatus("scan-1");
    await loadPreflightDuplicateFileMetadata("scan-1", 101);

    expect(invoke).toHaveBeenNthCalledWith(1, "load_preflight_duplicate_scan", {
      jobId: "scan-1",
      fileOffset: 96,
      fileLimit: 48,
      activityOffset: 48,
      activityLimit: 48,
    });
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "load_preflight_duplicate_scan_status",
      { jobId: "scan-1" },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      3,
      "load_preflight_duplicate_file_metadata",
      { jobId: "scan-1", ordinal: 101 },
    );
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
    const { cancelPreflightDuplicateScan, cancelUploadItem, clearDeletionRequests, clearUploadQueue } = await import("./local");
    await cancelPreflightDuplicateScan("scan-1");
    await clearUploadQueue();
    await cancelUploadItem("upload-1");
    await clearDeletionRequests();

    expect(invoke).toHaveBeenNthCalledWith(1, "cancel_preflight_duplicate_scan", { jobId: "scan-1" });
    expect(invoke).toHaveBeenNthCalledWith(2, "clear_upload_queue");
    expect(invoke).toHaveBeenNthCalledWith(3, "cancel_upload_item", { id: "upload-1" });
    expect(invoke).toHaveBeenNthCalledWith(4, "clear_deletion_requests");
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

  it("requests original-source deletion only through the completed-upload boundary", async () => {
    const { deleteUploadedSource } = await import("./local");

    await deleteUploadedSource("completed-item", "original.mp4");

    expect(invoke).toHaveBeenCalledWith("delete_uploaded_source", {
      id: "completed-item",
      confirmation: "original.mp4",
    });
  });

  it("targets only a confirmed watched local source for folder-monitor cleanup", async () => {
    const { deleteFolderMonitorUploadedSource } = await import("./local");

    await deleteFolderMonitorUploadedSource("watched-item", "camera.mp4");

    expect(invoke).toHaveBeenCalledWith("delete_folder_monitor_uploaded_source", {
      itemId: "watched-item",
      confirmation: "camera.mp4",
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

  it("submits a 100-file import and queue wave in exactly one webview/native round trip", async () => {
    const { importAndQueueBatch } = await import("./local");
    const paths = Array.from(
      { length: 100 },
      (_, index) => `C:\\Media\\clip-${index}.mp4`,
    );
    const settings = {
      madeForKids: false,
      visibility: "private" as const,
      deleteSourceAfterUpload: false,
    };

    await importAndQueueBatch(paths, settings);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("import_and_queue_batch", {
      paths,
      settings,
    });
  });

  it("creates a playlist only through the native command boundary", async () => {
    isTauri.mockReturnValue(true);
    vi.resetModules();
    const { createYouTubePlaylist } = await import("./local");

    await createYouTubePlaylist("New uploads");

    expect(invoke).toHaveBeenCalledWith("create_youtube_playlist", {
      title: "New uploads",
      privacyStatus: "private",
    });
    isTauri.mockReturnValue(false);
    vi.resetModules();
  });
});

describe("scoped playlist commands", () => {
  beforeEach(() => { invoke.mockReset(); isTauri.mockReturnValue(true); vi.resetModules(); });
  afterEach(() => { isTauri.mockReturnValue(false); vi.resetModules(); });

  it("binds creation to the reviewed channel and explicit visibility", async () => {
    const { createYouTubePlaylist } = await import("./local");
    await createYouTubePlaylist("Event highlights", "unlisted", "channel-a");
    expect(invoke).toHaveBeenCalledWith("create_youtube_playlist", { title: "Event highlights", privacyStatus: "unlisted", expectedChannelId: "channel-a" });
  });

  it("preserves selected order and operation identity on bulk addition", async () => {
    const receipt = { channelId: "channel-a", playlistId: "playlist-a", requestedVideoIds: ["video-z", "video-a"], addedVideoIds: ["video-z"], skippedVideoIds: ["video-a"], remainingVideoIds: [] };
    invoke.mockResolvedValue(receipt);
    const { addRemoteVideosToPlaylist } = await import("./local");
    await expect(addRemoteVideosToPlaylist("playlist-a", ["video-z", "video-a"], "channel-a", "operation-a")).resolves.toEqual(receipt);
    expect(invoke).toHaveBeenCalledWith("add_remote_videos_to_playlist", { playlistId: "playlist-a", videoIds: ["video-z", "video-a"], expectedChannelId: "channel-a", requestId: "operation-a" });
  });

  it("retains unresolved receipt IDs after a partial provider failure", async () => {
    const receipt = { channelId: "channel-a", playlistId: "playlist-a", requestedVideoIds: ["first", "second"], addedVideoIds: ["first"], skippedVideoIds: [], remainingVideoIds: ["second"], error: "The second result is uncertain.", uncertain: true };
    invoke.mockResolvedValue(receipt);
    const { addRemoteVideosToPlaylist } = await import("./local");
    await expect(addRemoteVideosToPlaylist("playlist-a", ["first", "second"], "channel-a", "job")).resolves.toEqual(receipt);
  });
});

describe("device-wide manual upload defaults", () => {
  beforeEach(() => { invoke.mockReset(); isTauri.mockReturnValue(false); vi.resetModules(); });
  afterEach(() => { isTauri.mockReturnValue(false); vi.resetModules(); });

  it("keeps a safe preview fallback and saves the made-for-kids default through a native command", async () => {
    const { loadManualUploadDefaults, saveManualUploadDefaults } = await import("./local");
    await expect(loadManualUploadDefaults()).resolves.toEqual({ madeForKids: false, visibility: "private" });
    await saveManualUploadDefaults(true);

    expect(invoke).toHaveBeenCalledWith("save_manual_upload_defaults", { madeForKids: true });
  });

  it("loads playlist defaults only for the reviewed connected channel", async () => {
    isTauri.mockReturnValue(true);
    const { loadManualUploadDefaults } = await import("./local");
    await loadManualUploadDefaults("channel-a");
    expect(invoke).toHaveBeenCalledWith("load_manual_upload_defaults", { expectedChannelId: "channel-a" });
  });

  it("keeps unspecified preferences out of a playlist or visibility patch", async () => {
    const { saveManualUploadDefaults } = await import("./local");
    await saveManualUploadDefaults({ visibility: "unlisted" });
    await saveManualUploadDefaults({ playlistId: "playlist-a", expectedChannelId: "channel-a" });
    await saveManualUploadDefaults({ playlistId: "", expectedChannelId: "channel-a" });
    expect(invoke.mock.calls).toEqual([
      ["save_manual_upload_defaults", { visibility: "unlisted" }],
      ["save_manual_upload_defaults", { playlistId: "playlist-a", expectedChannelId: "channel-a" }],
      ["save_manual_upload_defaults", { playlistId: "", expectedChannelId: "channel-a" }],
    ]);
  });
});
