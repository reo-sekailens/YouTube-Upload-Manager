import { invoke } from "@tauri-apps/api/core";
import type { ConnectionSettings, DashboardSnapshot, DeletionRequest, RemoteVideo, UploadItem, YouTubeConnectionStart } from "./types";

export const isTauri = "__TAURI_INTERNALS__" in window;

export async function loadSnapshot(): Promise<DashboardSnapshot> {
  if (!isTauri) return { items: [], duplicates: [] };
  return invoke<DashboardSnapshot>("dashboard_snapshot");
}

export async function importAsset(path: string): Promise<UploadItem> {
  return invoke<UploadItem>("import_asset", { path });
}

export async function queueItem(id: string): Promise<UploadItem> {
  return invoke<UploadItem>("queue_item", { id });
}

export async function reconcileQueue(): Promise<UploadItem[]> {
  return invoke<UploadItem[]>("reconcile_queue");
}

export async function startQueuedUploads(): Promise<number> {
  return invoke<number>("start_queued_uploads");
}

export async function syncChannelInventory(): Promise<number> {
  return invoke<number>("sync_channel_inventory");
}

export async function loadConnectionSettings(): Promise<ConnectionSettings> {
  if (!isTauri) return { connected: false };
  return invoke<ConnectionSettings>("load_connection_settings");
}

export async function saveOAuthClientId(clientId: string): Promise<ConnectionSettings> {
  return invoke<ConnectionSettings>("save_oauth_client_id", { oauthClientId: clientId });
}

export async function beginYoutubeConnection(): Promise<YouTubeConnectionStart> {
  return invoke<YouTubeConnectionStart>("begin_youtube_connection");
}

export async function disconnectYoutube(): Promise<ConnectionSettings> {
  return invoke<ConnectionSettings>("disconnect_youtube");
}

export async function listRemoteVideos(): Promise<RemoteVideo[]> {
  if (!isTauri) return [];
  return invoke<RemoteVideo[]>("list_remote_videos");
}

export async function listDeletionRequests(): Promise<DeletionRequest[]> {
  if (!isTauri) return [];
  return invoke<DeletionRequest[]>("list_deletion_requests");
}

export async function requestVideoDeletion(videoId: string, confirmation: string): Promise<DeletionRequest> {
  return invoke<DeletionRequest>("request_video_deletion", { videoId, confirmation });
}

export async function cancelDeletionRequest(id: string): Promise<void> {
  return invoke<void>("cancel_deletion_request", { id });
}

/** Starts a fresh native Google re-authorization that includes deletion scope. */
export async function beginDeletionAuthorization(): Promise<YouTubeConnectionStart> {
  return invoke<YouTubeConnectionStart>("begin_deletion_authorization");
}

/** Executes one already-reviewed request after its video ID is typed again. */
export async function executeDeletionRequest(id: string, confirmation: string): Promise<DeletionRequest> {
  return invoke<DeletionRequest>("execute_deletion_request", { id, confirmation });
}
