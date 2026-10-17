import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import type { DuplicateMediaPreviewEvidence, DuplicateMediaPreviewTarget } from "./types";

/** Opens only duplicate evidence already authorized and resolved by the native app. */
export async function prepareDuplicateMediaPreview(
  target: DuplicateMediaPreviewTarget,
  expectedChannelId?: string,
): Promise<DuplicateMediaPreviewEvidence> {
  if (!isTauri()) throw new Error("Video comparison is available in the desktop app.");
  const evidence = await invoke<Omit<DuplicateMediaPreviewEvidence, "localSources"> & {
    localSources: Array<Omit<DuplicateMediaPreviewEvidence["localSources"][number], "url">>;
  }>("prepare_duplicate_media_preview", { target, expectedChannelId });
  return {
    ...evidence,
    localSources: evidence.localSources.map((source) => ({
      ...source,
      url: convertFileSrc(source.token, "media-preview"),
    })),
  };
}

/** Revokes read-only streams when comparison closes or the channel changes. */
export async function releaseDuplicateMediaPreview(tokens: string[]): Promise<void> {
  if (!isTauri() || tokens.length === 0) return;
  await invoke("release_duplicate_media_preview", { tokens });
}

export type DuplicatePreviewCompatibility = {
  durationSeconds: number;
  mimeType: string;
};

/** Native code resolves the existing, channel-scoped grant; a path is never accepted. */
export async function inspectDuplicatePreviewCompatibility(token: string): Promise<DuplicatePreviewCompatibility> {
  if (!isTauri()) throw new Error("Video comparison is available in the desktop app.");
  return invoke("inspect_duplicate_preview_compatibility", { token });
}

/** One bounded fragment is decoded from the original, without creating a media file. */
export async function readDuplicatePreviewSegment(token: string, startSeconds: number): Promise<ArrayBuffer> {
  if (!isTauri()) throw new Error("Video comparison is available in the desktop app.");
  const bytes = await invoke<ArrayBuffer | number[]>("read_duplicate_preview_segment", { token, startSeconds });
  return bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer;
}
