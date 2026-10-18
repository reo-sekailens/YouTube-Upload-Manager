import { invoke, isTauri } from "@tauri-apps/api/core";

export type LiveQuotaLimit = {
  metric: string;
  displayName: string;
  unit: string;
  dimensions: Record<string, string>;
  effectiveLimit: number | null;
  used: number | null;
  remaining: number | null;
  sampleThrough: string | null;
  state: "reported" | "no_usage_data" | "unsupported_bucket" | "limit_unavailable";
};
export type LiveQuotaSnapshot = {
  fetchedAt: string;
  periodStart: string;
  periodEnd: string;
  metricLagSeconds: number;
  limits: LiveQuotaLimit[];
};
export type LiveQuotaStatus = {
  phase: "disconnected" | "authorizing" | "connected" | "failed";
  channelId: string;
  projectId: string;
  detail: string;
  attemptId: string | null;
  snapshot: LiveQuotaSnapshot | null;
};
export type LiveQuotaAuthorization = { authorizationUrl: string; attemptId: string };

function desktop() {
  if (!isTauri()) throw new Error("Google quota reports are available in the desktop app.");
}
export function loadLiveQuotaStatus(expectedChannelId: string): Promise<LiveQuotaStatus> {
  desktop(); return invoke("load_live_quota_status", { expectedChannelId });
}
export function refreshLiveQuota(expectedChannelId: string): Promise<LiveQuotaStatus> {
  desktop(); return invoke("refresh_live_quota", { expectedChannelId });
}
export function beginLiveQuotaAuthorization(expectedChannelId: string): Promise<LiveQuotaAuthorization> {
  desktop(); return invoke("begin_live_quota_authorization", { expectedChannelId });
}
export function cancelLiveQuotaAuthorization(expectedChannelId: string, attemptId: string): Promise<LiveQuotaStatus> {
  desktop(); return invoke("cancel_live_quota_authorization", { expectedChannelId, attemptId });
}
export async function openLiveQuotaAuthorization(authorization: LiveQuotaAuthorization) {
  const url = new URL(authorization.authorizationUrl);
  if (url.protocol !== "https:" || url.hostname !== "accounts.google.com" || url.pathname !== "/o/oauth2/v2/auth" || url.username || url.password)
    throw new Error("Google authorization returned an unexpected address.");
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url.href);
}
export function liveQuotaStatusMatches(status: LiveQuotaStatus, channelId: string, projectId: string) {
  return Boolean(channelId && projectId) && status.channelId === channelId && status.projectId === projectId;
}
export function reportedQuotaNumber(value: number | null) {
  return value != null && Number.isFinite(value) && value >= 0 ? value.toLocaleString() : "Unknown";
}
