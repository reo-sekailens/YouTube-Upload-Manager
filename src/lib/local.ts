import { invoke, isTauri as detectTauri } from "@tauri-apps/api/core";
import type { ConnectionSettings, DashboardSnapshot, DeletionRequest, FolderMonitorSettings, FolderMonitorVisibility, ManualUploadDefaults, ManualUploadSettings, RemoteVideo, UploadItem, UploadVisibility, YouTubeConnectionStart, YouTubePlaylist } from "./types";

// Use Tauri's public runtime detector. Internal bridge properties are not an
// application capability contract and can leave interactive controls disabled.
export const isTauri = detectTauri();

export async function loadSnapshot(): Promise<DashboardSnapshot> {
  if (!isTauri) return { items: [], duplicates: [] };
  return invoke<DashboardSnapshot>("dashboard_snapshot");
}

export async function importAsset(path: string, settings: ManualUploadSettings): Promise<UploadItem> {
  return invoke<UploadItem>("import_asset", { path, settings });
}

export async function listYouTubePlaylists(): Promise<YouTubePlaylist[]> {
  if (!isTauri) return [];
  return invoke<YouTubePlaylist[]>("list_youtube_playlists");
}

export async function loadManualUploadDefaults(): Promise<ManualUploadDefaults> {
  if (!isTauri) return { madeForKids: false };
  return invoke<ManualUploadDefaults>("load_manual_upload_defaults");
}

export async function saveManualUploadDefaults(madeForKids: boolean): Promise<ManualUploadDefaults> {
  return invoke<ManualUploadDefaults>("save_manual_upload_defaults", { madeForKids });
}

export async function queueItem(id: string): Promise<UploadItem> {
  return invoke<UploadItem>("queue_item", { id });
}

/** Saves the operator-selected visibility on one manual upload before it is queued. */
export async function setItemVisibility(id: string, visibility: UploadVisibility): Promise<UploadItem> {
  return invoke<UploadItem>("set_item_visibility", { id, visibility });
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

export async function loadFolderMonitorSettings(): Promise<FolderMonitorSettings> {
  if (!isTauri) {
    return {
      enabled: false,
      visibility: "private",
      status: "disabled",
      detail: "Folder monitoring is available only in the signed desktop app.",
    };
  }
  return invoke<FolderMonitorSettings>("load_folder_monitor_settings");
}

export async function enableFolderMonitor(path: string, visibility: FolderMonitorVisibility): Promise<FolderMonitorSettings> {
  return invoke<FolderMonitorSettings>("enable_folder_monitor", { path, visibility });
}

export async function disableFolderMonitor(): Promise<FolderMonitorSettings> {
  return invoke<FolderMonitorSettings>("disable_folder_monitor");
}

export async function scanFolderMonitorNow(): Promise<FolderMonitorSettings> {
  return invoke<FolderMonitorSettings>("scan_folder_monitor_now");
}

export async function loadConnectionSettings(): Promise<ConnectionSettings> {
  if (!isTauri) return { connected: false };
  return invoke<ConnectionSettings>("load_connection_settings");
}

export async function saveOAuthClientId(clientId: string): Promise<ConnectionSettings> {
  return invoke<ConnectionSettings>("save_oauth_client_id", { oauthClientId: clientId });
}

/** Parses a downloaded Google Desktop OAuth JSON file only in Rust; its secret stays in OS-protected storage. */
export async function importDesktopOAuthClient(path: string): Promise<ConnectionSettings> {
  return invoke<ConnectionSettings>("import_desktop_oauth_client", { path });
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
