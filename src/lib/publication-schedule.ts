import { invoke, isTauri } from "@tauri-apps/api/core";
export type PublicationContext = { timeZone: string; tzdbVersion: string };
export type PublicationCandidate = { choice: "only" | "earlier" | "later"; publishAt: string; localTime: string };
export type PublicationRow = { index: number; localTime: string; state: "ready" | "gap" | "ambiguous" | "past" | "invalid"; publishAt?: string; candidates: PublicationCandidate[]; detail?: string; preserved: boolean };
export type PublicationPreview = PublicationContext & { channelId: string; rows: PublicationRow[]; ready: boolean };
export type PublicationJob = { itemId: string; channelId: string; title: string; fileName: string; publishAt: string; timeZone: string; localTime: string; tzdbVersion: string; state: string; videoId?: string; providerConfirmedPublishAt?: string; everPublic: boolean; detail: string; updatedAt: string };
export type PublicationPage = { channelId: string; jobs: PublicationJob[]; total: number };
export type PublicationEdit = { itemId: string; expectedPublishAt: string; expectedUpdatedAt: string; publishAt: string; context: PublicationContext };
export type PublicationRequest = { timeZone: string; startLocal: string; spacingMinutes: number; count: number; itemLocalTimes: Record<number, string>; ambiguousChoices: Record<number, "earlier" | "later">; preservedTimes: Record<number, string> };

export async function listPublicationTimezones(): Promise<string[]> { if (!isTauri()) return []; return invoke("list_publication_timezones"); }
export async function resolvePublicationCalendar(expectedChannelId: string, request: PublicationRequest): Promise<PublicationPreview> { return invoke("resolve_publication_calendar", { expectedChannelId, ...request }); }
export async function loadPublicationCalendar(expectedChannelId: string, itemIds?: string[], offset = 0): Promise<PublicationPage> { return invoke("load_publication_calendar", { expectedChannelId, itemIds, offset }); }
export async function editPublicationCalendar(expectedChannelId: string, changes: PublicationEdit[], approved: boolean): Promise<PublicationJob[]> { return invoke("edit_publication_calendar", { expectedChannelId, changes, approved }); }
export async function repairPublicationJob(expectedChannelId: string, itemId: string, approved: boolean): Promise<PublicationJob> { return invoke("repair_publication_job", { expectedChannelId, itemId, approved }); }

export function publicationPreviewReady(preview: PublicationPreview | undefined, expectedChannelId: string, count: number): boolean {
  return Boolean(preview && preview.channelId === expectedChannelId && preview.ready && count > 0 && preview.rows.length === count && new Set(preview.rows.map((row) => row.index)).size === count && preview.rows.every((row) => Number.isInteger(row.index) && row.index >= 0 && row.index < count && row.state === "ready" && typeof row.publishAt === "string" && Number.isFinite(Date.parse(row.publishAt))));
}
export function publicationTimesByPath(preview: PublicationPreview, paths: string[]): Record<string, string> {
  if (!publicationPreviewReady(preview, preview.channelId, paths.length)) throw new Error("Review a complete native publication preview for the selected videos.");
  return Object.fromEntries(preview.rows.map((row) => [paths[row.index], row.publishAt!]));
}
export function publicationJobEditable(job: PublicationJob): boolean { return !job.everPublic && !["applying", "uncertain", "published"].includes(job.state); }
export function publicationLocalInput(instant: string, timeZone: string): string {
  try { const parts = new Intl.DateTimeFormat("sv-SE", { timeZone, year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).format(new Date(instant)); return parts.replace(" ", "T"); } catch { return ""; }
}
export function publicationUtcLabel(instant: string): string { return instant.replace("T", " ").replace(/\+00:00$/, " UTC").replace(/Z$/, " UTC"); }
