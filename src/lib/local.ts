import { invoke, isTauri as detectTauri } from "@tauri-apps/api/core";
import type {
  ConnectionSettings,
  DashboardSnapshot,
  DeletionRequest,
  FolderMonitorSettings,
  FolderMonitorOverview,
  FolderMonitorVisibility,
  ManualUploadDefaults,
  ManualUploadSettings,
  PortableArchiveReceipt,
  PreIngestDuplicateScan,
  RemoteVideo,
  UploadItem,
  UploadTitleDuplicate,
  UploadTitleDuplicateDecision,
  UploadVisibility,
  YouTubeConnectionStart,
  YouTubePlaylist,
} from "./types";

// Use Tauri's public runtime detector. Internal bridge properties are not an
// application capability contract and can leave interactive controls disabled.
export const isTauri = detectTauri();

/** Ends the native app only after the UI has completed its exit confirmation. */
export async function exitApplication(): Promise<void> {
  if (!isTauri)
    throw new Error("App exit is available only in the signed desktop app.");
  await invoke("exit_application");
}

const youtubeAccountBrowserLabel = "youtube-account-browser";
const youtubeAccountBrowserUrl = "https://www.youtube.com/";
const googleSetupBrowserUrls = {
  account: "https://accounts.google.com/",
  cloud: "https://console.cloud.google.com/",
} as const;

/** Opens an isolated app window directly on YouTube; this app never sees credentials entered there. */
export async function openYouTubeAccountBrowser(): Promise<void> {
  if (!isTauri)
    throw new Error(
      "The YouTube account browser is available only in the signed desktop app.",
    );
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const existing = await WebviewWindow.getByLabel(youtubeAccountBrowserLabel);
  if (existing) {
    await existing.show();
    await existing.setFocus();
    return;
  }
  const accountWindow = new WebviewWindow(youtubeAccountBrowserLabel, {
    url: youtubeAccountBrowserUrl,
    title: "YouTube account",
    width: 1_120,
    height: 760,
    minWidth: 720,
    minHeight: 520,
  });
  await new Promise<void>((resolve, reject) => {
    void accountWindow.once("tauri://created", () => resolve());
    void accountWindow.once("tauri://error", (event) =>
      reject(new Error(String(event.payload))),
    );
  });
}

/** Opens a separate, unprivileged Google page for an operator-owned setup action. */
export async function openGoogleSetupBrowser(
  destination: keyof typeof googleSetupBrowserUrls,
): Promise<void> {
  if (!isTauri)
    throw new Error(
      "Google setup is available only in the signed desktop app.",
    );
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const label = `google-setup-${destination}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.show();
    await existing.setFocus();
    return;
  }
  const setupWindow = new WebviewWindow(label, {
    url: googleSetupBrowserUrls[destination],
    title:
      destination === "account" ? "Google account setup" : "Google Cloud setup",
    width: 1_120,
    height: 760,
    minWidth: 720,
    minHeight: 520,
  });
  await new Promise<void>((resolve, reject) => {
    void setupWindow.once("tauri://created", () => resolve());
    void setupWindow.once("tauri://error", (event) =>
      reject(new Error(String(event.payload))),
    );
  });
}

export async function loadSnapshot(): Promise<DashboardSnapshot> {
  if (!isTauri)
    return { items: [], duplicates: [], pendingTitleDuplicates: [] };
  return invoke<DashboardSnapshot>("dashboard_snapshot");
}

export async function loadDiagnosticReport(): Promise<string> {
  if (!isTauri) {
    return `## What happened\n\n<!-- Describe what you saw and the steps to reproduce it. -->\n\n## Diagnostics\n\n- App: browser preview\n- User agent: ${navigator.userAgent}\n`;
  }
  return invoke<string>("github_issue_diagnostic_report");
}

export type ReleaseIdentity = {
  version: string;
  channel:
    | "regular"
    | "nightly"
    | `nightly-${string}`
    | `v${string}-nightly.${string}`;
  buildProfile: string;
};

const browserPreviewRelease: ReleaseIdentity = {
  version: "browser preview",
  channel: "regular",
  buildProfile: "preview",
};

export async function loadReleaseIdentity(): Promise<ReleaseIdentity> {
  if (!isTauri) return browserPreviewRelease;
  return invoke<ReleaseIdentity>("app_release_identity");
}

export type CrashRecoveryStatus = {
  crashDetected: boolean;
  detectedAt?: string;
  failureKind?: string;
};

export async function loadCrashRecoveryStatus(): Promise<CrashRecoveryStatus> {
  if (!isTauri) return { crashDetected: false };
  return invoke<CrashRecoveryStatus>("load_crash_recovery_status");
}

export async function acknowledgeCrashRecovery(): Promise<void> {
  if (!isTauri) return;
  await invoke("acknowledge_crash_recovery");
}

export async function recordWebviewError(): Promise<void> {
  if (!isTauri) return;
  await invoke("record_webview_error");
}

export async function importAsset(
  path: string,
  settings: ManualUploadSettings,
): Promise<UploadItem> {
  return invoke<UploadItem>("import_asset", { path, settings });
}

export async function listYouTubePlaylists(): Promise<YouTubePlaylist[]> {
  if (!isTauri) return [];
  return invoke<YouTubePlaylist[]>("list_youtube_playlists");
}

/** Creates a private playlist for the currently connected YouTube channel. */
export async function createYouTubePlaylist(title: string): Promise<YouTubePlaylist> {
  if (!isTauri)
    throw new Error("Playlist creation is available only in the signed desktop app.");
  return invoke<YouTubePlaylist>("create_youtube_playlist", { title });
}

export async function loadManualUploadDefaults(): Promise<ManualUploadDefaults> {
  if (!isTauri) return { madeForKids: false };
  return invoke<ManualUploadDefaults>("load_manual_upload_defaults");
}

export async function saveManualUploadDefaults(
  madeForKids: boolean,
): Promise<ManualUploadDefaults> {
  return invoke<ManualUploadDefaults>("save_manual_upload_defaults", {
    madeForKids,
  });
}

export async function queueItem(id: string): Promise<UploadItem> {
  return invoke<UploadItem>("queue_item", { id });
}

export async function clearUploadQueue(): Promise<number> {
  return invoke<number>("clear_upload_queue");
}

/** Removes one unfinished upload from the visible queue without deleting media. */
export async function cancelUploadItem(id: string): Promise<void> {
  return invoke<void>("cancel_upload_item", { id });
}

/** Synchronizes the connected channel's inventory, then returns title matches for these local items. */
export async function checkUploadTitleDuplicates(
  itemIds: string[],
): Promise<UploadTitleDuplicate[]> {
  if (!isTauri || itemIds.length === 0) return [];
  return invoke<UploadTitleDuplicate[]>("check_upload_title_duplicates", {
    itemIds,
  });
}

/** Hides one reviewed false-positive match locally until the operator explicitly re-audits ignored matches. */
export async function ignoreDuplicateCandidate(
  candidateId: string,
): Promise<void> {
  return invoke<void>("ignore_duplicate_candidate", { candidateId });
}

/** Restores all persisted ignored matches so the next dedupe review includes them again. */
export async function reAuditIgnoredDuplicateCandidates(): Promise<number> {
  return invoke<number>("re_audit_ignored_duplicate_candidates");
}

/** Starts a crash-resumable pre-ingest job. Light compares filenames; deep streams BLAKE3 locally. */
export async function preflightDuplicateFiles(
  paths: string[],
  mode: "light" | "deep" = "light",
): Promise<PreIngestDuplicateScan> {
  if (!isTauri || paths.length === 0)
    return {
      id: "",
      mode,
      status: "complete",
      totalFiles: 0,
      completedFiles: 0,
      pendingMetadataFiles: 0,
      files: [],
      activityLog: [],
      youtubeTitleChecked: false,
    };
  return invoke<PreIngestDuplicateScan>("start_preflight_duplicate_files", {
    paths,
    mode,
  });
}

/** Reads the latest checkpointed results for a persistent pre-ingest job. */
export async function loadPreflightDuplicateScan(
  jobId: string,
): Promise<PreIngestDuplicateScan> {
  return invoke<PreIngestDuplicateScan>("load_preflight_duplicate_scan", {
    jobId,
  });
}

export async function cancelPreflightDuplicateScan(
  jobId: string,
): Promise<void> {
  return invoke<void>("cancel_preflight_duplicate_scan", { jobId });
}

/** Reuses an accepted local duplicate review to return a short-lived native deletion token. */
export async function preparePreflightLocalDeleteFile(
  jobId: string,
  ordinal: number,
): Promise<string> {
  if (!isTauri)
    throw new Error(
      "Local duplicate deletion is available only in the signed desktop app.",
    );
  return invoke<string>("prepare_preflight_local_delete_file", { jobId, ordinal });
}

/** Permanently deletes one desktop source file after exact filename confirmation, without rehashing it. */
export async function deletePreflightDuplicateFile(
  token: string,
  confirmation: string,
): Promise<void> {
  if (!isTauri)
    throw new Error(
      "Local duplicate deletion is available only in the signed desktop app.",
    );
  return invoke<void>("delete_preflight_duplicate_file", {
    token,
    confirmation,
  });
}

/** Persists an explicit decision for one or more title matches. Ignored matches can then be queued. */
export async function resolveUploadTitleDuplicates(
  itemIds: string[],
  action: UploadTitleDuplicateDecision,
): Promise<UploadItem[]> {
  if (!isTauri || itemIds.length === 0) return [];
  return invoke<UploadItem[]>("resolve_upload_title_duplicates", {
    itemIds,
    action,
  });
}

/** Saves the operator-selected visibility on one manual upload before it is queued. */
export async function setItemVisibility(
  id: string,
  visibility: UploadVisibility,
): Promise<UploadItem> {
  return invoke<UploadItem>("set_item_visibility", { id, visibility });
}

export async function setItemDeleteSourceAfterUpload(
  id: string,
  deleteSourceAfterUpload: boolean,
): Promise<UploadItem> {
  return invoke<UploadItem>("set_item_delete_source_after_upload", {
    id,
    deleteSourceAfterUpload,
  });
}

/** Deletes a verified external original only after a completed YouTube upload. */
export async function deleteUploadedSource(
  id: string,
  confirmation: string,
): Promise<void> {
  return invoke("delete_uploaded_source", { id, confirmation });
}

export async function reconcileQueue(): Promise<UploadItem[]> {
  return invoke<UploadItem[]>("reconcile_queue");
}

export async function syncChannelInventory(): Promise<number> {
  return invoke<number>("sync_channel_inventory");
}

export async function loadFolderMonitorSettings(): Promise<FolderMonitorSettings> {
  if (!isTauri) {
    return {
      enabled: false,
      visibility: "private",
      madeForKids: false,
      deleteSourceAfterUpload: false,
      status: "disabled",
      detail: "Folder monitoring is available only in the signed desktop app.",
    };
  }
  return invoke<FolderMonitorSettings>("load_folder_monitor_settings");
}

export async function loadFolderMonitorOverview(): Promise<FolderMonitorOverview> {
  if (!isTauri) {
    return {
      settings: await loadFolderMonitorSettings(),
      files: [],
      logs: [],
    };
  }
  return invoke<FolderMonitorOverview>("load_folder_monitor_overview");
}

export async function enableFolderMonitor(
  path: string,
  visibility: FolderMonitorVisibility,
  madeForKids: boolean,
  deleteSourceAfterUpload: boolean,
  playlistId?: string,
  playlistTitle?: string,
): Promise<FolderMonitorSettings> {
  return invoke<FolderMonitorSettings>("enable_folder_monitor", {
    path,
    visibility,
    madeForKids,
    deleteSourceAfterUpload,
    playlistId,
    playlistTitle,
  });
}

export async function disableFolderMonitor(): Promise<FolderMonitorSettings> {
  return invoke<FolderMonitorSettings>("disable_folder_monitor");
}

export async function scanFolderMonitorNow(): Promise<FolderMonitorSettings> {
  return invoke<FolderMonitorSettings>("scan_folder_monitor_now");
}

/** Explicitly moves the current monitored-folder baseline through normal safe intake. */
export async function processExistingFolderFiles(): Promise<FolderMonitorSettings> {
  return invoke<FolderMonitorSettings>("process_existing_folder_files");
}

export async function loadConnectionSettings(): Promise<ConnectionSettings> {
  if (!isTauri) return { connected: false };
  return invoke<ConnectionSettings>("load_connection_settings");
}

/** Parses a downloaded Google Desktop OAuth JSON file only in Rust; its secret stays in OS-protected storage. */
export async function importDesktopOAuthClient(
  path: string,
): Promise<ConnectionSettings> {
  return invoke<ConnectionSettings>("import_desktop_oauth_client", { path });
}

export async function beginYoutubeConnection(): Promise<YouTubeConnectionStart> {
  return invoke<YouTubeConnectionStart>("begin_youtube_connection");
}

/** Cancels a pending ordinary Google connection without removing a saved connection. */
export async function cancelYoutubeConnection(attemptId: string): Promise<ConnectionSettings> {
  return invoke<ConnectionSettings>("cancel_youtube_connection", { attemptId });
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

export async function requestVideoDeletion(
  videoId: string,
  confirmation: string,
): Promise<DeletionRequest> {
  return invoke<DeletionRequest>("request_video_deletion", {
    videoId,
    confirmation,
  });
}

export async function cancelDeletionRequest(id: string): Promise<void> {
  return invoke<void>("cancel_deletion_request", { id });
}

export async function clearDeletionRequests(): Promise<number> {
  return invoke<number>("clear_deletion_requests");
}

/** Starts a fresh native Google re-authorization that includes deletion scope. */
export async function beginDeletionAuthorization(): Promise<YouTubeConnectionStart> {
  return invoke<YouTubeConnectionStart>("begin_deletion_authorization");
}

/** Writes compact dedupe metadata only; credentials, source paths, sessions, and media stay on this device. */
export async function exportPortableArchive(
  path: string,
): Promise<PortableArchiveReceipt> {
  return invoke<PortableArchiveReceipt>("export_portable_archive", { path });
}

/** Merges compact dedupe metadata from another install without importing media or OAuth credentials. */
export async function importPortableArchive(
  path: string,
): Promise<PortableArchiveReceipt> {
  return invoke<PortableArchiveReceipt>("import_portable_archive", { path });
}

/** Enables a 15-minute local deletion session after deletion scope has been granted. */
export async function enableDeletionSudoMode(): Promise<ConnectionSettings> {
  return invoke<ConnectionSettings>("enable_deletion_sudo_mode");
}

/** Ends the temporary local deletion session immediately. */
export async function disableDeletionSudoMode(): Promise<ConnectionSettings> {
  return invoke<ConnectionSettings>("disable_deletion_sudo_mode");
}

/** Executes one already-reviewed request after its video ID is typed again. */
export async function executeDeletionRequest(
  id: string,
  confirmation: string,
): Promise<DeletionRequest> {
  return invoke<DeletionRequest>("execute_deletion_request", {
    id,
    confirmation,
  });
}
