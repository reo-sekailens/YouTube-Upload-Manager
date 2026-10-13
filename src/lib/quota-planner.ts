import { invoke, isTauri } from "@tauri-apps/api/core";

export type QuotaBucket = "video_uploads" | "search" | "other";
export type QuotaProjectLimits = { uploadCalls?: number | null; searchCalls?: number | null; otherUnits?: number | null };
export type QuotaChoices = {
  uploads: number;
  refreshLibrary: boolean;
  libraryVideos: number;
  playlists: Array<{ existingItems: number; additions: number }>;
  thumbnailWrites: number;
  captionInserts: number;
  captionUpdates: number;
  metadataUpdates: number;
  searchPages: number;
  processingChecksPerVideo: number;
  retryReserve: number;
};
export type QuotaUsage = { bucket: QuotaBucket; requests: number; estimatedUnits: number; configuredDailyLimit?: number | null };
export type QuotaHold = { deadline: string; active: boolean };
export type QuotaSnapshot = {
  channelId: string;
  channelName: string;
  scopeId: string;
  configuredProjectId?: string | null;
  limits: QuotaProjectLimits;
  usage: QuotaUsage[];
  unknownRequests: number;
  pacificDay: string;
  pacificResetAt: string;
  observedAt: string;
  cloudHold?: QuotaHold | null;
  channelHold?: QuotaHold | null;
  costVersion: string;
  costAccessedAt: string;
  costSourceUrl: string;
  costs: Array<{ method: string; bucket: QuotaBucket; cost: number }>;
  observationLabel: string;
  warnings: string[];
};
export type QuotaOperation = { label: string; method: string; bucket: QuotaBucket; requests: number; retryRequests: number; costPerRequest: number; estimatedUnits: number };
export type QuotaEstimate = {
  snapshot: QuotaSnapshot;
  operations: QuotaOperation[];
  buckets: Array<{ bucket: QuotaBucket; estimatedRequests: number; estimatedUnits: number; observedUnits: number; configuredDailyLimit?: number | null; exceedsConfiguredLimit?: boolean | null }>;
  warnings: string[];
};

export const quotaResourceUrls = {
  console: "https://console.cloud.google.com/apis/api/youtube.googleapis.com/quotas",
  extension: "https://support.google.com/youtube/contact/yt_api_form",
  costs: "https://developers.google.com/youtube/v3/determine_quota_cost",
} as const;

export const quotaBucketLabels: Record<QuotaBucket, string> = { video_uploads: "Video upload calls", search: "Search calls", other: "Other API units" };

export function defaultQuotaChoices(initial: Partial<QuotaChoices> = {}): QuotaChoices {
  return { uploads: 0, refreshLibrary: false, libraryVideos: 0, playlists: [], thumbnailWrites: 0, captionInserts: 0, captionUpdates: 0, metadataUpdates: 0, searchPages: 0, processingChecksPerVideo: 0, retryReserve: 0, ...initial };
}

export function validQuotaChoices(choices: QuotaChoices) {
  const count = (value: number, maximum = 1_000_000) => Number.isSafeInteger(value) && value >= 0 && value <= maximum;
  return [choices.uploads, choices.libraryVideos, choices.thumbnailWrites, choices.captionInserts, choices.captionUpdates, choices.metadataUpdates, choices.searchPages].every((value) => count(value))
    && count(choices.processingChecksPerVideo, 100) && count(choices.retryReserve, 5)
    && choices.playlists.length <= 64 && choices.playlists.every((playlist) => count(playlist.existingItems) && count(playlist.additions));
}

function desktop() { if (!isTauri()) throw new Error("Quota planning is available in the desktop app."); }
export async function loadQuotaPlanner(expectedChannelId: string): Promise<QuotaSnapshot> {
  desktop(); return invoke("load_quota_planner", { expectedChannelId });
}
export async function estimateQuotaOperations(expectedChannelId: string, choices: QuotaChoices): Promise<QuotaEstimate> {
  desktop(); return invoke("estimate_quota_operations", { expectedChannelId, choices });
}
export async function saveQuotaProjectLimits(expectedChannelId: string, expectedScopeId: string, limits: QuotaProjectLimits): Promise<QuotaSnapshot> {
  desktop(); return invoke("save_quota_project_limits", { expectedChannelId, expectedScopeId, limits: { uploadCalls: limits.uploadCalls ?? null, searchCalls: limits.searchCalls ?? null, otherUnits: limits.otherUnits ?? null } });
}
export async function openQuotaResource(resource: keyof typeof quotaResourceUrls): Promise<void> {
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(quotaResourceUrls[resource]);
}

export function quotaLimitsChanged(before: QuotaProjectLimits, after: QuotaProjectLimits) {
  return (["uploadCalls", "searchCalls", "otherUnits"] as const).some((key) => (before[key] ?? null) !== (after[key] ?? null));
}
export function parseQuotaLimit(value: string): number | null {
  if (!value.trim()) return null;
  if (!/^\d+$/.test(value.trim())) throw new Error("Enter a whole-number planning limit or leave it blank for unknown.");
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number > 1_000_000_000_000) throw new Error("Planning limits cannot exceed 1,000,000,000,000.");
  return number;
}
export function quotaSnapshotMatches(snapshot: QuotaSnapshot, expectedChannelId: string, expectedScopeId?: string) {
  return Boolean(expectedChannelId && snapshot.scopeId) && snapshot.channelId === expectedChannelId && (!expectedScopeId || snapshot.scopeId === expectedScopeId);
}
export function formatQuotaTime(value: string, timeZone: string) {
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return "Saved deadline unavailable";
  try { return new Intl.DateTimeFormat(undefined, { timeZone, dateStyle: "medium", timeStyle: "short" }).format(date); }
  catch { return "Choose a valid timezone"; }
}
export function validQuotaTimeZone(value: string) {
  try { new Intl.DateTimeFormat(undefined, { timeZone: value }).format(); return true; } catch { return false; }
}
