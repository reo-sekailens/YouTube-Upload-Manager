import { invoke, isTauri } from "@tauri-apps/api/core";
import type { UploadItem } from "./types";

export type UploadRetryItem = {
  itemId: string;
  title: string;
  fileName: string;
  detail: string;
  eligible: boolean;
  blockedReason?: string;
};
export type UploadRetryReview = {
  reviewId: string;
  channelId: string;
  channelName?: string;
  failureKey: string;
  failureDetail: string;
  expiresAt: string;
  items: UploadRetryItem[];
  eligibleCount: number;
  blockedCount: number;
};
export type UploadRetryReceipt = {
  reviewId: string;
  channelId: string;
  queuedItemIds: string[];
  blockedItems: Array<{ itemId: string; detail: string }>;
  detail: string;
};

/** This is a presentation hint. Native review rechecks the failure and channel. */
export function uploadRetryIssueKey(item: UploadItem & { retryFailureKey?: string }) {
  if (!["failed", "needs_reconciliation"].includes(item.status) || item.videoId) return undefined;
  if (item.retryFailureKey?.trim() && (item.status === "failed" || (item.confirmedBytes === 0 && item.retryFailureKey.startsWith("setup:")))) return item.retryFailureKey;
  if (item.confirmedBytes === 0 && item.detail === "YouTube rejected the upload setup request.")
    return "setup:legacy_unknown";
  return undefined;
}

function nativeOnly() {
  if (!isTauri()) throw new Error("Upload retry is available in the desktop app.");
}
export function loadUploadRetryReview(expectedChannelId: string, anchorItemId: string, allMatching: boolean) {
  nativeOnly();
  return invoke<UploadRetryReview>("load_upload_retry_review", { expectedChannelId, anchorItemId, allMatching });
}
export function retryUploadReview(expectedChannelId: string, reviewId: string, itemIds: string[]) {
  nativeOnly();
  return invoke<UploadRetryReceipt>("retry_upload_review", { expectedChannelId, reviewId, itemIds });
}
