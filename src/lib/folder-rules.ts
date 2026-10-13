import { invoke, isTauri } from "@tauri-apps/api/core";

export type FolderRules = { includePatterns: string[]; excludePatterns: string[]; readyMarkerSuffix?: string; admitPreviouslyExcluded: boolean };
export type FolderRulesSettings = { monitorId: string; channelId: string; rules: FolderRules; revision: string };
export type FolderRulesExample = { fileName: string; disposition: "included" | "excluded" | "retained_excluded" | "waiting_ready_marker" | "invalid_ready_marker" | "changing" | "retained_queue"; detail: string };
export type FolderRulesPreview = { monitorId: string; channelId: string; folderName: string; rules: FolderRules; fingerprint: string; total: number; included: number; excluded: number; waitingReady: number; changing: number; retainedQueue: number; examples: FolderRulesExample[] };
export const emptyFolderRules: FolderRules = { includePatterns: [], excludePatterns: [], admitPreviouslyExcluded: false };
export const folderRulesDispositionLabels: Record<FolderRulesExample["disposition"], string> = { included: "Included", excluded: "Excluded by rules", retained_excluded: "Previously excluded", waiting_ready_marker: "Waiting for ready marker", invalid_ready_marker: "Invalid ready marker", changing: "Waiting for stability", retained_queue: "Existing queue or history" };

function nativeOnly() { if (!isTauri()) throw new Error("Folder rules are available in the desktop app."); }
export function loadFolderRules(monitorId: string, expectedChannelId: string) { nativeOnly(); return invoke<FolderRulesSettings>("load_folder_rules", { monitorId, expectedChannelId }); }
export function previewFolderRules(monitorId: string, expectedChannelId: string, rules: FolderRules) { nativeOnly(); return invoke<FolderRulesPreview>("preview_folder_rules", { monitorId, expectedChannelId, rules }); }
export function saveFolderRules(monitorId: string, expectedChannelId: string, rules: FolderRules, previewFingerprint: string, approved: boolean) { nativeOnly(); return invoke<FolderRulesSettings>("save_folder_rules", { monitorId, expectedChannelId, rules, previewFingerprint, approved }); }

export function parseFolderPatterns(text: string) { return text.split(/\r?\n/).map((line) => line.trim()).filter(Boolean); }
export function folderRulesError(rules: FolderRules): string | undefined {
  const patterns = [...rules.includePatterns, ...rules.excludePatterns];
  if (patterns.length > 32) return "Use at most 32 filename patterns in total.";
  if (patterns.some((pattern) => !pattern || Array.from(pattern).length > 256 || /[\u0000-\u001f\u007f/\\:[\]{}]/.test(pattern))) return "Use filename patterns with * or ? only; paths and bracket patterns are not supported.";
  if ([rules.includePatterns, rules.excludePatterns].some((list) => new Set(list.map((pattern) => pattern.toLowerCase())).size !== list.length)) return "Remove repeated patterns in each list.";
  if (rules.readyMarkerSuffix !== undefined && (!/^\.[A-Za-z0-9_.-]{1,31}$/.test(rules.readyMarkerSuffix) || rules.readyMarkerSuffix.includes("..") || /\.(3g2|3gp|avi|flv|m4v|mkv|mov|mp4|mpeg|mpg|webm|wmv)$/i.test(rules.readyMarkerSuffix))) return "Use a simple marker suffix such as .ready, starting with a dot, up to 32 characters and not a video extension.";
  return undefined;
}
