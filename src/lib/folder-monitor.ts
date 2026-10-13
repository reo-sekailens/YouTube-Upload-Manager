import { invoke, isTauri } from "@tauri-apps/api/core";
import type { FolderMonitorCollection, FolderMonitorLocalDeleteResult, FolderMonitorOverview, FolderMonitorSettings, FolderMonitorVisibility } from "./types";

function scopedInvoke<T>(command: string, monitorId?: string, expectedChannelId?: string, parameters?: Record<string, unknown>): Promise<T> {
  const args = { ...parameters, ...(monitorId ? { monitorId } : {}), ...(expectedChannelId ? { expectedChannelId } : {}) };
  return Object.keys(args).length ? invoke<T>(command, args) : invoke<T>(command);
}

export async function loadFolderMonitors(expectedChannelId?: string): Promise<FolderMonitorCollection> {
  if (!isTauri()) return { monitors: [] };
  return scopedInvoke("load_folder_monitors", undefined, expectedChannelId);
}

export async function loadFolderMonitorSettings(monitorId?: string, expectedChannelId?: string): Promise<FolderMonitorSettings> {
  if (!isTauri()) return { enabled: false, visibility: "private", madeForKids: false, deleteSourceAfterUpload: false, status: "disabled", detail: "Folder monitoring is available only in the signed desktop app." };
  return scopedInvoke("load_folder_monitor_settings", monitorId, expectedChannelId);
}

export async function loadFolderMonitorOverview(monitorId?: string, expectedChannelId?: string): Promise<FolderMonitorOverview> {
  if (!isTauri()) return { settings: await loadFolderMonitorSettings(), files: [], logs: [] };
  return scopedInvoke("load_folder_monitor_overview", monitorId, expectedChannelId);
}

export async function enableFolderMonitor(path: string, visibility: FolderMonitorVisibility, madeForKids: boolean, deleteSourceAfterUpload: boolean, playlistId?: string, playlistTitle?: string, monitorId?: string, expectedChannelId?: string): Promise<FolderMonitorSettings> {
  return scopedInvoke("enable_folder_monitor", monitorId, expectedChannelId, { path, visibility, madeForKids, deleteSourceAfterUpload, playlistId, playlistTitle });
}

export async function disableFolderMonitor(monitorId?: string, expectedChannelId?: string): Promise<FolderMonitorSettings> {
  return scopedInvoke("disable_folder_monitor", monitorId, expectedChannelId);
}

export async function removeFolderMonitor(monitorId: string, expectedChannelId: string): Promise<void> {
  return scopedInvoke("remove_folder_monitor", monitorId, expectedChannelId);
}

export async function scanFolderMonitorNow(monitorId?: string, expectedChannelId?: string): Promise<FolderMonitorSettings> {
  return scopedInvoke("scan_folder_monitor_now", monitorId, expectedChannelId);
}

export async function processExistingFolderFiles(monitorId?: string, expectedChannelId?: string): Promise<FolderMonitorSettings> {
  return scopedInvoke("process_existing_folder_files", monitorId, expectedChannelId);
}

export async function requeueCancelledFolderMonitorFiles(itemIds: string[] = [], monitorId?: string, expectedChannelId?: string): Promise<number> {
  return scopedInvoke("requeue_cancelled_folder_monitor_files", monitorId, expectedChannelId, { itemIds });
}

export async function reconcileFolderMonitorUploads(monitorId?: string, expectedChannelId?: string): Promise<number> {
  return scopedInvoke("reconcile_folder_monitor_uploads", monitorId, expectedChannelId);
}

export async function deleteFolderMonitorUploadedSource(itemId: string, confirmation: string, monitorId?: string, expectedChannelId?: string): Promise<FolderMonitorLocalDeleteResult> {
  return scopedInvoke("delete_folder_monitor_uploaded_source", monitorId, expectedChannelId, { itemId, confirmation });
}

export async function deleteFolderMonitorUploadedSources(itemIds: string[], confirmation: string, monitorId?: string, expectedChannelId?: string): Promise<FolderMonitorLocalDeleteResult[]> {
  return scopedInvoke("delete_folder_monitor_uploaded_sources", monitorId, expectedChannelId, { itemIds, confirmation });
}
