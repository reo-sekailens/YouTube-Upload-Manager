import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ManualUploadSettings, UploadMetadata } from "./types";

export type UploadPlanRow = {
  ordinal: number;
  fileName: string;
  title: string;
  channelId: string;
  visibility: string;
  madeForKids: boolean;
  playlistId?: string;
  playlistTitle?: string;
  playlistEvidence: "not_requested" | "owner_verified" | "unverified_cached";
  deleteSourceAfterUpload: boolean;
  sizeBytes: number;
  sourceStatus: "verified" | "unavailable";
  duplicateDisposition: "ready" | "review_duplicate" | "blocked";
  duplicateMatches: string[];
  metadata: Record<string, unknown>;
  publicationContext?: NonNullable<ManualUploadSettings["publicationContexts"]>[string];
  /** Native inspected original attachments sealed into this preview; no provider job exists yet. */
  assetJobs?: Array<{ kind: "thumbnail" | "caption"; fileName: string; dimensions?: string | null; cueCount?: number | null; language?: string | null; name?: string | null; draft?: boolean | null }>;
  warnings: string[];
};

/** An empty item override deliberately suppresses the common metadata. */
export function planPublicationTimes(paths: string[], settings: ManualUploadSettings): Record<string, string> {
  return Object.fromEntries(paths.flatMap((path) => {
    const metadata = settings.itemMetadata && Object.hasOwn(settings.itemMetadata, path) ? settings.itemMetadata[path] : settings.metadata;
    return metadata?.publishAt ? [[path, metadata.publishAt]] : [];
  }));
}

export function planWithMetadata(paths: string[], settings: ManualUploadSettings, metadata: UploadMetadata, itemMetadata: Record<string, UploadMetadata>): ManualUploadSettings {
  const previous = planPublicationTimes(paths, settings);
  const next = planPublicationTimes(paths, { ...settings, metadata, itemMetadata });
  const contexts = Object.fromEntries(Object.entries(settings.publicationContexts ?? {}).filter(([path]) => Object.hasOwn(previous, path) && Object.hasOwn(next, path) && Number.isFinite(Date.parse(next[path])) && Date.parse(previous[path]) === Date.parse(next[path])));
  return { ...settings, metadata, itemMetadata, visibility: Object.keys(next).length ? "private" : settings.visibility, publicationContexts: Object.keys(contexts).length ? contexts : undefined };
}

export function planWithPublication(paths: string[], settings: ManualUploadSettings, result: { publishAtByPath: Record<string, string>; timeZone: string; tzdbVersion: string }): ManualUploadSettings {
  if (!paths.length || new Set(paths).size !== paths.length || Object.keys(result.publishAtByPath).length !== paths.length || paths.some((path) => !Object.hasOwn(result.publishAtByPath, path) || !Number.isFinite(Date.parse(result.publishAtByPath[path]))) || !result.timeZone || !result.tzdbVersion) throw new Error("Review publication dates and native timezone proof for every selected original.");
  const itemMetadata = Object.fromEntries(paths.map((path) => [path, { ...(settings.itemMetadata && Object.hasOwn(settings.itemMetadata, path) ? settings.itemMetadata[path] : settings.metadata), publishAt: result.publishAtByPath[path] }]));
  const publicationContexts = Object.fromEntries(paths.map((path) => [path, { timeZone: result.timeZone, tzdbVersion: result.tzdbVersion }]));
  return { ...settings, visibility: "private", itemMetadata, publicationContexts };
}

/** A different source selection needs its own complete metadata and schedule review. */
export function planWithoutSourceChoices(settings: ManualUploadSettings): ManualUploadSettings {
  return { ...settings, metadata: undefined, itemMetadata: undefined, publicationContexts: undefined };
}

export type UploadPlanPage = {
  id: string;
  channelId: string;
  channelName: string;
  createdAt: string;
  expiresAt: string;
  evidenceMode: "cached" | "online";
  evidenceAsOf?: string;
  totalFiles: number;
  readyFiles: number;
  duplicateFiles: number;
  blockedFiles: number;
  offset: number;
  limit: number;
  files: UploadPlanRow[];
  warnings: string[];
};

function desktop() {
  if (!isTauri()) throw new Error("Batch planning is available in the desktop app.");
}

export async function createUploadPlan(paths: string[], settings: ManualUploadSettings, expectedChannelId: string, onlineRefresh = false): Promise<UploadPlanPage> {
  desktop();
  return invoke("create_upload_plan", { paths, settings, expectedChannelId, onlineRefresh });
}

export async function loadUploadPlan(planId: string, expectedChannelId: string, offset = 0, limit = 25): Promise<UploadPlanPage> {
  desktop();
  return invoke("load_upload_plan", { planId, expectedChannelId, offset, limit });
}

/** Explicit export of display metadata. Native excludes file locators and recovery secrets. */
export async function exportUploadPlan(planId: string, expectedChannelId: string): Promise<string | undefined> {
  desktop();
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ title: "Export batch preview", defaultPath: "upload-plan.json", filters: [{ name: "JSON batch preview", extensions: ["json"] }] });
  if (!path) return undefined;
  return invoke<string>("save_upload_plan_export", { planId, expectedChannelId, path });
}

export async function discardUploadPlan(planId: string, expectedChannelId: string): Promise<void> {
  if (isTauri()) await invoke("discard_upload_plan", { planId, expectedChannelId });
}

export function planMatchesChannel(plan: UploadPlanPage, expectedChannelId: string) {
  return Boolean(expectedChannelId) && Boolean(plan.id) && plan.channelId === expectedChannelId && Number.isSafeInteger(plan.totalFiles) && plan.totalFiles > 0 && plan.totalFiles <= 512 && new Set(plan.files.map((file) => file.ordinal)).size === plan.files.length && plan.files.every((file) => file.channelId === expectedChannelId && Number.isSafeInteger(file.ordinal) && file.ordinal >= 0 && file.ordinal < plan.totalFiles);
}

/** The native seal owns source proof. UI checks keep a late seal tied to the displayed base context. */
export function planSealMatchesBase(base: UploadPlanPage, sealed: UploadPlanPage): boolean {
  return planMatchesChannel(sealed, base.channelId) && sealed.id !== base.id && sealed.totalFiles === base.totalFiles && sealed.createdAt === base.createdAt && sealed.evidenceMode === base.evidenceMode && planIsCurrent(base) && planIsCurrent(sealed) && Date.parse(sealed.expiresAt) <= Date.parse(base.expiresAt);
}

export function planIsCurrent(plan: UploadPlanPage, currentTime = Date.now()) {
  const expiry = Date.parse(plan.expiresAt);
  return Number.isFinite(expiry) && expiry > currentTime;
}

export function fileNameFromSelection(path: string) {
  return path.split(/[\\/]/).pop() || "Selected video";
}
