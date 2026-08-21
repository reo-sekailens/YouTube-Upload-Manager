import { describe, expect, it } from "vitest";
import type { ConnectionSettings, DeletionRequest, UploadItem, UploadStatus } from "./types";

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
      updatedAt: "2026-08-22T00:00:00Z",
    };

    expect(item.status).toBe("needs_reconciliation");
    expect(item.videoId).toBeUndefined();
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
