import { invoke, isTauri } from "@tauri-apps/api/core";

export type SourceRelinkMode = "file" | "folder";
export type SourceRelinkFile = {
  itemId: string;
  title: string;
  fileName: string;
  previousLocation: string;
  replacementLocation: string;
  sizeBytes: number;
  status: "pending" | "verified" | "blocked";
  detail: string;
};
export type SourceRelinkReview = {
  reviewId: string;
  channelId: string;
  channelName?: string;
  state: "prepared" | "verifying" | "verified" | "committed" | "cancelled";
  expiresAt: string;
  previousRoot?: string;
  replacementRoot: string;
  files: SourceRelinkFile[];
  verifiedCount: number;
  blockedCount: number;
};
export type SourceRelinkReceipt = {
  reviewId: string;
  channelId: string;
  relinkedItemIds: string[];
  resumedItemIds: string[];
  attentionItemIds: string[];
  detail: string;
};

function nativeOnly() {
  if (!isTauri()) throw new Error("Locate originals is available in the desktop app.");
}

export function prepareSourceRelink(itemIds: string[], expectedChannelId: string, path: string, mode: SourceRelinkMode) {
  nativeOnly();
  return invoke<SourceRelinkReview>("prepare_source_relink", { itemIds, expectedChannelId, path, mode });
}
export function verifySourceRelink(reviewId: string, expectedChannelId: string) {
  nativeOnly();
  return invoke<SourceRelinkReview>("verify_source_relink", { reviewId, expectedChannelId });
}
export function loadSourceRelinkReview(reviewId: string, expectedChannelId: string) {
  nativeOnly();
  return invoke<SourceRelinkReview>("load_source_relink_review", { reviewId, expectedChannelId });
}
export function commitSourceRelink(reviewId: string, expectedChannelId: string, itemIds: string[], resumeUploads: boolean, approved: boolean) {
  nativeOnly();
  return invoke<SourceRelinkReceipt>("commit_source_relink", { reviewId, expectedChannelId, itemIds, resumeUploads, approved });
}
export function cancelSourceRelink(reviewId: string, expectedChannelId: string) {
  nativeOnly();
  return invoke<void>("cancel_source_relink", { reviewId, expectedChannelId });
}
