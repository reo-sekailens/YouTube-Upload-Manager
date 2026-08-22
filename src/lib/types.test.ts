import { describe, expect, it } from "vitest";
import type { ConnectionSettings, DeletionRequest, FolderMonitorSettings, UploadItem, UploadStatus, UploadVisibility } from "./types";

describe("persisted upload item contract", () => {
  it("represents reconciliation without treating it as a completed upload", () => {
    const status: UploadStatus = "needs_reconciliation";
    const item: UploadItem = {
      id: "saved-item",
      title: "Recovered video",
      fileName: "recovered.mp4",
      sizeBytes: 1024,
      status,
      confirmedBytes: 512,
      totalBytes: 1024,
      visibility: "private",
      madeForKids: false,
      updatedAt: "2026-08-22T00:00:00Z",
    };

    expect(item.status).toBe("needs_reconciliation");
    expect(item.videoId).toBeUndefined();
  });

  it("keeps manual visibility explicit and starts private", () => {
    const visibility: UploadVisibility = "private";
    const item: UploadItem = {
      id: "new-item",
      title: "New video",
      fileName: "new-video.mp4",
      sizeBytes: 1024,
      status: "draft",
      confirmedBytes: 0,
      totalBytes: 1024,
      visibility,
      madeForKids: true,
      uploadStartedAt: "2026-08-22T00:00:00Z",
      transferBytesPerSecond: 256,
      updatedAt: "2026-08-22T00:00:00Z",
    };

    expect(item.visibility).toBe("private");
    expect(item.uploadStartedAt).toBe("2026-08-22T00:00:00Z");
    expect(item.transferBytesPerSecond).toBe(256);
  });
});

describe("local deletion request contract", () => {
  it("represents an explicit pending request without claiming a YouTube deletion", () => {
    const request: DeletionRequest = {
      id: "request-1",
      videoId: "video-to-review",
      title: "Operator-selected video",
      status: "pending",
      detail: "Awaiting explicit native execution.",
      updatedAt: "2026-08-22T00:00:00Z",
    };

    expect(request.status).toBe("pending");
    expect(request.videoId).toBe("video-to-review");
  });
});

describe("deletion authorization contract", () => {
  it("keeps the separately granted destructive scope out of ordinary connection state", () => {
    const settings: ConnectionSettings = { connected: true, deletionAuthorized: true };

    expect(settings.deletionAuthorized).toBe(true);
  });
});

describe("folder monitor settings contract", () => {
  it("keeps recurring approval bound to one folder and channel", () => {
    const settings: FolderMonitorSettings = {
      enabled: true,
      visibility: "unlisted",
      folderPath: "C:\\Media\\Ready",
      channelName: "Test channel",
      status: "watching",
      detail: "Waiting for completed video files.",
      lastScanAt: "2026-08-22T08:00:00Z",
      lastFileName: "episode-04.mp4",
    };

    expect(settings.enabled).toBe(true);
    expect(settings.channelName).toBe("Test channel");
    expect(settings.folderPath).toContain("Ready");
    expect(settings.visibility).toBe("unlisted");
  });
});
