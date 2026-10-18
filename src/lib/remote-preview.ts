import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import type { DuplicateMediaPreviewTarget } from "./types";

export type RemotePreviewConfiguration = {
  configured: boolean;
  runtimeConfigured: boolean;
  supported: boolean;
};

function nativeOnly(): void {
  if (!isTauri()) throw new Error("Streaming comparison is available in the desktop app.");
}

export async function getRemotePreviewConfiguration(): Promise<RemotePreviewConfiguration> {
  nativeOnly();
  return invoke("get_remote_preview_configuration");
}

/** Only operator-selected tool locations cross this bridge; media URLs stay native. */
export async function configureRemotePreviewTool(toolPath: string, runtimePath?: string): Promise<RemotePreviewConfiguration> {
  nativeOnly();
  return invoke("configure_remote_preview_tool", { toolPath, runtimePath: runtimePath ?? null });
}

/** The request identity can be released before extraction finishes. */
export async function prepareRemotePreview(
  target: DuplicateMediaPreviewTarget,
  expectedChannelId: string | undefined,
  videoId: string,
  requestId: string = crypto.randomUUID(),
): Promise<{ token: string; url: string }> {
  nativeOnly();
  const { token } = await invoke<{ token: string }>("prepare_remote_preview", {
    target, expectedChannelId: expectedChannelId ?? null, videoId, requestId,
  });
  return { token, url: convertFileSrc(token, "media-preview") };
}

export async function releaseRemotePreview(tokens: string[]): Promise<void> {
  if (!isTauri() || tokens.length === 0) return;
  await invoke("release_remote_preview", { tokens });
}
