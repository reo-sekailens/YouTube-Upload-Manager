import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ManualUploadDefaults, ManualUploadDefaultsPatch } from "./types";

/** Loaded with manual-upload surfaces, keeping preferences out of the startup chunk. */
export async function loadManualUploadDefaults(expectedChannelId?: string): Promise<ManualUploadDefaults> {
  if (!isTauri()) return { madeForKids: false, visibility: "private" };
  return invoke<ManualUploadDefaults>("load_manual_upload_defaults", { expectedChannelId });
}

export async function saveManualUploadDefaults(change: boolean | ManualUploadDefaultsPatch): Promise<ManualUploadDefaults> {
  return invoke<ManualUploadDefaults>("save_manual_upload_defaults", typeof change === "boolean" ? { madeForKids: change } : change);
}
