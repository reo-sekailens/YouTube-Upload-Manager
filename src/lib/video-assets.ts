import { invoke, isTauri } from "@tauri-apps/api/core";
import type { UploadPlanPage } from "./upload-plan";

export type VideoAssetKind = "thumbnail" | "caption";
export type VideoAssetItem = { itemId: string; title: string; fileName: string; videoId?: string | null; status: string };
export type VideoAssetCandidate = { assetId: string; fileName: string; kind: VideoAssetKind | "unknown"; sizeBytes: number; valid: boolean; dimensions?: string | null; cueCount?: number | null; suggestedItemIds: string[]; warnings: string[] };
export type VideoAssetReview = { reviewId: string; channelId: string; items: VideoAssetItem[]; assets: VideoAssetCandidate[]; expiresAt: string; limitsCheckedAt: string };
export type VideoAssetBinding = { assetId: string; itemId: string; language?: string; name?: string; draft?: boolean };
export type VideoAssetJob = { jobId: string; itemId: string; channelId: string; videoId?: string | null; kind: VideoAssetKind; fileName: string; language?: string | null; name?: string | null; draft?: boolean | null; status: string; detail: string; captionId?: string | null; updatedAt: string };
export type VideoAssetWorkspace = { channelId: string; items: VideoAssetItem[]; jobs: VideoAssetJob[] };
export type VideoAssetReceipt = { channelId: string; jobIds: string[] };
export type VideoAssetMatch = { itemIds: string[]; language: string; name: string; draft?: boolean };

function desktop() { if (!isTauri()) throw new Error("Video attachments are available in the desktop app."); }
export async function loadVideoAssetWorkspace(expectedChannelId: string, itemIds: string[]): Promise<VideoAssetWorkspace> { desktop(); return invoke("load_video_asset_workspace", { expectedChannelId, itemIds }); }
export async function prepareVideoAssets(expectedChannelId: string, itemIds: string[], paths: string[], autoMatch: boolean): Promise<VideoAssetReview> { desktop(); return invoke("prepare_video_assets", { expectedChannelId, itemIds, paths, autoMatch }); }
export async function saveVideoAssetJobs(expectedChannelId: string, reviewId: string, bindings: VideoAssetBinding[]): Promise<VideoAssetReceipt> { desktop(); return invoke("save_video_asset_jobs", { expectedChannelId, reviewId, bindings }); }
export async function discardVideoAssetReview(expectedChannelId: string, reviewId: string): Promise<void> { if (isTauri()) await invoke("discard_video_asset_review", { expectedChannelId, reviewId }); }
export async function repairVideoAssetJobs(expectedChannelId: string, jobIds: string[]): Promise<VideoAssetReceipt> { desktop(); return invoke("repair_video_asset_jobs", { expectedChannelId, jobIds }); }
export async function loadUploadPlanAssetWorkspace(expectedChannelId: string, planId: string): Promise<VideoAssetWorkspace> { desktop(); return invoke("load_upload_plan_asset_workspace", { expectedChannelId, planId }); }
export async function prepareUploadPlanAssets(expectedChannelId: string, planId: string, paths: string[], autoMatch: boolean): Promise<VideoAssetReview> { desktop(); return invoke("prepare_upload_plan_assets", { expectedChannelId, planId, paths, autoMatch }); }
/** Seal the reviewed mappings into a new immutable plan; never create jobs. */
export async function applyUploadPlanAssets(expectedChannelId: string, planId: string, reviewId: string, bindings: VideoAssetBinding[]): Promise<UploadPlanPage> { desktop(); return invoke("apply_upload_plan_assets", { expectedChannelId, planId, reviewId, bindings }); }

export function assetReviewCurrent(review: VideoAssetReview, now = Date.now()) { const expiry = Date.parse(review.expiresAt); return Number.isFinite(expiry) && expiry > now; }
export function assetWorkspaceMatches(workspace: VideoAssetWorkspace, expectedChannelId: string, itemIds: string[]) { return workspace.channelId === expectedChannelId && workspace.items.length === itemIds.length && workspace.items.every((item) => itemIds.includes(item.itemId)) && workspace.jobs.every((job) => job.channelId === expectedChannelId && itemIds.includes(job.itemId)); }
export function defaultAssetMatches(review: VideoAssetReview): Record<string, VideoAssetMatch> {
  return Object.fromEntries(review.assets.map((asset) => [asset.assetId, { itemIds: asset.valid && asset.suggestedItemIds.length === 1 ? [...asset.suggestedItemIds] : [], language: "", name: "", draft: undefined }]));
}
export function assetBindings(review: VideoAssetReview, matches: Record<string, VideoAssetMatch>): VideoAssetBinding[] {
  if (!assetReviewCurrent(review)) throw new Error("This asset review expired. Recheck the originals before saving jobs.");
  const result: VideoAssetBinding[] = []; const declarations = new Set<string>();
  for (const asset of review.assets) {
    const match = matches[asset.assetId]; if (!match?.itemIds.length) continue;
    if (!asset.valid || asset.kind === "unknown") throw new Error("Remove or leave unmatched any invalid attachment.");
    if (new Set(match.itemIds).size !== match.itemIds.length || match.itemIds.some((id) => !review.items.some((item) => item.itemId === id))) throw new Error("Choose only the reviewed videos for this attachment.");
    if (asset.kind === "caption" && (!/^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{1,8})*$/.test(match.language.trim()) || match.language.trim().length > 35 || !match.name.trim() || [...match.name.trim()].length > 150 || /[\u0000-\u001f\u007f]/.test(match.name) || match.draft === undefined)) throw new Error("Declare each caption’s language, name and draft choice before Review.");
    for (const itemId of match.itemIds) {
      const key = `${itemId}:${asset.kind}:${asset.kind === "caption" ? `${match.language.trim().toLowerCase()}:${match.name.trim()}` : ""}`;
      if (declarations.has(key)) throw new Error("Choose one thumbnail per video and unique caption language/name declarations."); declarations.add(key);
      result.push(asset.kind === "caption" ? { assetId: asset.assetId, itemId, language: match.language.trim().toLowerCase(), name: match.name.trim(), draft: match.draft } : { assetId: asset.assetId, itemId });
    }
  }
  if (!result.length || result.length > 400) throw new Error("Match between 1 and 400 attachments to the reviewed videos.");
  return result;
}

export const assetJobLabels: Record<string, string> = { waiting_upload: "Waiting for confirmed video", queued: "Queued separately", waiting_permission: "Waiting for permission or quota", running: "Applying attachment", waiting_processing: "Caption processing", succeeded: "Confirmed", failed: "Needs repair", needs_reconciliation: "Outcome uncertain · reconcile", conflict: "Existing track or processing conflict", superseded: "Earlier thumbnail receipt retained" };
