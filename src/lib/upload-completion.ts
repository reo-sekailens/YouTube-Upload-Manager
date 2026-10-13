import { invoke } from "@tauri-apps/api/core";

export type CompletionOutcome = "succeeded" | "waiting" | "failed" | "not_requested" | "unknown";
export type CompletionStepKey = "upload" | "processing" | "playlist" | "source_cleanup" | "thumbnail" | "caption" | "publication";
export type CompletionStep = { key: CompletionStepKey; outcome: CompletionOutcome; detail: string; repairable: boolean; checkedAt?: string; repairJobIds?: string[]; publicationSnapshot?: { publishAt: string; updatedAt: string } };
export type UploadCompletionItem = { itemId: string; channelId: string; title: string; fileName: string; videoId?: string; playlistId?: string; playlistTitle?: string; updatedAt: string; steps: CompletionStep[] };
export type UploadCompletionPage = { channelId: string; uploadBatchId?: string; items: UploadCompletionItem[]; total: number; pendingProcessing: number };
export type UploadCompletionBatch = { batchId: string; createdAt: string; itemCount: number; title?: string };
export type UploadCompletionBatchPage = { channelId: string; totalBatches: number; batches: UploadCompletionBatch[]; offset: number; limit: number };
export type UploadCompletionExportReceipt = { count: number; format: "json" | "csv"; uploadBatchId?: string };
export type CompletionRepairStep = "playlist" | "source_cleanup" | "thumbnail" | "caption" | "publication";
export type CompletionReviewedJob = { key: "thumbnail" | "caption" | "publication"; jobIds?: string[]; publishAt?: string; updatedAt?: string };

export function loadUploadCompletionBatches(expectedChannelId: string, offset = 0, limit = 25) {
  return invoke<UploadCompletionBatchPage>("load_upload_completion_batches", { expectedChannelId, offset, limit });
}
export function loadUploadCompletion(expectedChannelId: string, itemIds?: string[], offset = 0, limit = 25, uploadBatchId?: string) {
  return invoke<UploadCompletionPage>("load_upload_completion", { expectedChannelId, itemIds, offset, limit, uploadBatchId });
}
export function checkUploadCompletion(expectedChannelId: string, itemIds: string[]) {
  return invoke<UploadCompletionPage>("check_upload_completion", { expectedChannelId, itemIds });
}
export function repairUploadCompletion(expectedChannelId: string, itemId: string, videoId: string, steps: CompletionRepairStep[], confirmation?: string, reviewedJobs?: CompletionReviewedJob[]) {
  return invoke<UploadCompletionItem>("repair_upload_completion", { expectedChannelId, itemId, videoId, steps, confirmation, reviewedJobs });
}
export function exportUploadCompletion(expectedChannelId: string, format: "json" | "csv", path: string, itemIds?: string[], uploadBatchId?: string) {
  return invoke<UploadCompletionExportReceipt>("export_upload_completion", { expectedChannelId, itemIds, format, path, uploadBatchId });
}

export const completionOutcomeLabels: Record<CompletionOutcome, string> = {
  succeeded: "Complete", waiting: "Waiting", failed: "Needs attention", not_requested: "Not requested", unknown: "Unknown",
};
export const completionStepLabels: Record<CompletionStepKey, string> = {
  upload: "Upload", processing: "YouTube processing", playlist: "Playlist", source_cleanup: "Original source cleanup", thumbnail: "Thumbnail", caption: "Caption", publication: "Scheduled publication",
};
