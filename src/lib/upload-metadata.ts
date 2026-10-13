import { invoke, isTauri } from "@tauri-apps/api/core";
import type { UploadMetadata, UploadMetadataMapping, UploadMetadataPreset } from "./types";

/** Native owns manifest parsing, source mapping, token expansion and validation. */
export async function loadUploadPresets(expectedChannelId: string): Promise<UploadMetadataPreset[]> {
  if (!isTauri()) return [];
  return invoke("load_upload_presets", { expectedChannelId });
}

export async function saveUploadPreset(expectedChannelId: string, name: string, metadata: UploadMetadata, id?: string): Promise<UploadMetadataPreset> {
  return invoke("save_upload_preset", { expectedChannelId, id, name, metadata });
}

export async function deleteUploadPreset(expectedChannelId: string, id: string): Promise<void> {
  return invoke("delete_upload_preset", { expectedChannelId, id });
}

export async function mapUploadManifest(expectedChannelId: string, path: string, selectedPaths: string[], rootPath?: string): Promise<UploadMetadataMapping> {
  return invoke("map_upload_manifest", { expectedChannelId, path, selectedPaths, ...(rootPath ? { rootPath } : {}) });
}

export async function resolveUploadMetadata(expectedChannelId: string, paths: string[], metadata: UploadMetadata, itemMetadata?: Record<string, UploadMetadata>): Promise<UploadMetadataMapping> {
  return invoke("resolve_upload_metadata", { expectedChannelId, paths, metadata, itemMetadata });
}

/** Reject malformed mappings before associating any metadata with a selected source. */
export function metadataMappingToItems(mapping: UploadMetadataMapping, paths: string[]): Record<string, UploadMetadata> {
  const indices = new Set<number>();
  const result: Record<string, UploadMetadata> = {};
  for (const match of mapping.matches) {
    if (!Number.isInteger(match.index) || match.index < 0 || match.index >= paths.length || indices.has(match.index))
      throw new Error("The metadata result did not match the selected files. Choose the manifest again.");
    indices.add(match.index);
    result[paths[match.index]] = match.templateMetadata || match.metadata;
  }
  return result;
}

/** Explicit absent booleans remain absent; false is a deliberate declaration. */
export function metadataBooleanChoice(value: boolean | undefined): string {
  return value === undefined ? "" : value ? "true" : "false";
}

export function metadataFromBooleanChoice(value: string): boolean | undefined {
  return value === "true" ? true : value === "false" ? false : undefined;
}

export function metadataPreviewReady(mapping: UploadMetadataMapping | undefined, count: number): boolean {
  return Boolean(mapping && count > 0 && mapping.issues.length === 0 && mapping.matches.length === count && new Set(mapping.matches.map((match) => match.index)).size === count && mapping.matches.every((match) => Number.isInteger(match.index) && match.index >= 0 && match.index < count));
}

export function metadataFileName(path: string): string {
  return path.split(/[\\/]/).pop() || "Selected video";
}
