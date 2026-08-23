import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type {
  DeletionRequest,
  DuplicateCandidate,
  PreIngestDuplicateScan,
  RemoteVideo,
  UploadItem,
} from "../lib/types";
import { DeletionReview } from "./DeletionReview";
import { DuplicateReview } from "./DuplicateReview";
import { PreIngestDuplicatePanel } from "./PreIngestDuplicatePanel";
import { QueueTable } from "./QueueTable";

const fixtureSize = 10_000;
const countAttribute = (markup: string, attribute: string) =>
  markup.match(new RegExp(attribute, "g"))?.length ?? 0;

describe("large-list component rendering", () => {
  it("renders fewer than 100 upload queue rows from 10,000 items", () => {
    const items: UploadItem[] = Array.from({ length: fixtureSize }, (_, index) => ({
      confirmedBytes: 0,
      fileName: `video-${index}.mp4`,
      id: `upload-${index}`,
      madeForKids: false,
      sizeBytes: 1,
      status: "queued",
      title: `Video ${index}`,
      totalBytes: 1,
      updatedAt: "2026-08-23T00:00:00.000Z",
      visibility: "private",
    }));
    const markup = renderToStaticMarkup(
      <QueueTable
        busy={false}
        items={items}
        onCancel={() => undefined}
        onDeleteSourceAfterUploadChange={() => undefined}
        onDeleteUploadedSource={() => undefined}
        onQueue={() => undefined}
        onVisibilityChange={() => undefined}
      />,
    );

    expect(countAttribute(markup, "data-queue-record")).toBe(48);
    expect(countAttribute(markup, "data-queue-record")).toBeLessThan(100);
  });

  it("renders fewer than 100 duplicate candidate cards from 10,000 candidates", () => {
    const candidates: DuplicateCandidate[] = Array.from(
      { length: fixtureSize },
      (_, index) => ({
        confidence: "exact_local",
        evidence: `Exact local evidence ${index}`,
        id: `duplicate-${index}`,
        leftTitle: `Video ${index}`,
        rightTitle: `Video ${index} (2)`,
      }),
    );
    const markup = renderToStaticMarkup(
      <DuplicateReview candidates={candidates} onIgnore={() => undefined} />,
    );

    expect(countAttribute(markup, "data-duplicate-record")).toBe(48);
    expect(countAttribute(markup, "data-duplicate-record")).toBeLessThan(100);
  });

  it("renders fewer than 100 preflight result and activity records from 10,000 inputs", () => {
    const scan: PreIngestDuplicateScan = {
      activityLog: Array.from({ length: fixtureSize }, (_, index) => ({
        createdAt: "2026-08-23T00:00:00.000Z",
        fileName: `video-${index}.mp4`,
        message: "Checked",
      })),
      completedFiles: fixtureSize,
      files: Array.from({ length: fixtureSize }, (_, index) => ({
        canDeleteLocalDuplicate: false,
        droppedDuplicateFileNames: [],
        fileName: `video-${index}.mp4`,
        localMatches: [],
        localMetadata: { metadataFields: [], streams: [] },
        ordinal: index,
        sizeBytes: 1,
        uploadedTitleMatches: [],
      })),
      id: "large-preflight",
      mode: "light",
      pendingMetadataFiles: 0,
      status: "complete",
      totalFiles: fixtureSize,
      youtubeTitleChecked: true,
    };
    const markup = renderToStaticMarkup(
      <PreIngestDuplicatePanel
        busy={false}
        dropActive={false}
        fileCount={0}
        onCancel={() => undefined}
        onChoose={() => undefined}
        onDeleteLocalDuplicate={async () => undefined}
        onPrepareLocalDuplicateDelete={async () => "token"}
        scan={scan}
      />,
    );
    const resultCount = countAttribute(markup, "data-preflight-record");
    const activityCount = countAttribute(
      markup,
      "data-preflight-activity-record",
    );

    expect(resultCount).toBe(48);
    expect(activityCount).toBe(48);
    expect(resultCount + activityCount).toBeLessThan(100);
  });

  it("renders fewer than 100 deletion inventory and request cards from 10,000 inputs each", () => {
    const videos: RemoteVideo[] = Array.from(
      { length: fixtureSize },
      (_, index) => ({
        title: `Video ${index}`,
        updatedAt: "2026-08-23T00:00:00.000Z",
        videoId: `video-${index}`,
      }),
    );
    const requests: DeletionRequest[] = Array.from(
      { length: fixtureSize },
      (_, index) => ({
        detail: "Pending local request",
        id: `request-${index}`,
        status: "pending",
        title: `Requested video ${index}`,
        updatedAt: "2026-08-23T00:00:00.000Z",
        videoId: `requested-video-${index}`,
      }),
    );
    const markup = renderToStaticMarkup(
      <DeletionReview
        activeChannel="Fixture channel"
        initialRequests={requests}
        initialVideos={videos}
        onNotice={() => undefined}
      />,
    );
    const inventoryCount = countAttribute(
      markup,
      "data-deletion-inventory-record",
    );
    const requestCount = countAttribute(
      markup,
      "data-deletion-request-record",
    );

    expect(inventoryCount).toBe(48);
    expect(requestCount).toBe(48);
    expect(inventoryCount + requestCount).toBeLessThan(100);
  });
});
