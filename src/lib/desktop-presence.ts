import { invoke, isTauri } from "@tauri-apps/api/core";
export type DesktopPreferences = { batchComplete: boolean; needsAttention: boolean; quotaPause: boolean; trayEnabled: boolean; keepRunningOnClose: boolean; runOnLogin: boolean };
export type DesktopPresenceSettings = { preferences: DesktopPreferences; revision: string; pauseNewWork: boolean; desktopSupported: boolean; trayReady: boolean; notificationPermission: "desktop_os_controls_delivery" | "denied" | "unknown" | "unsupported"; startupStatus: string; startupEnabled?: boolean; clickSupport: string };
export type DesktopActivity = { routeId: string; kind: "batch_complete" | "needs_attention" | "quota_pause"; count: number; outcome: string; createdAt: string };
export type DesktopRoute = { routeId: string; channelId: string; kind: string; itemIds: string[]; count: number; needsChannelSwitch: boolean };
export const defaultDesktopPreferences: DesktopPreferences = { batchComplete: false, needsAttention: false, quotaPause: false, trayEnabled: false, keepRunningOnClose: false, runOnLogin: false };
function nativeOnly() { if (!isTauri()) throw new Error("Desktop activity controls are available in the native app."); }
export function loadDesktopPresence() { nativeOnly(); return invoke<DesktopPresenceSettings>("load_desktop_presence"); }
export function saveDesktopPresence(preferences: DesktopPreferences, expectedRevision: string, approved: boolean, startupApproved: boolean) { nativeOnly(); return invoke<DesktopPresenceSettings>("save_desktop_presence", { preferences, expectedRevision, approved, startupApproved }); }
export function setDesktopPresencePause(paused: boolean) { nativeOnly(); return invoke<void>("set_desktop_presence_pause", { paused }); }
export function handleDesktopClose() { nativeOnly(); return invoke<boolean>("handle_desktop_close"); }
export function loadPendingDesktopRoute() { nativeOnly(); return invoke<DesktopRoute | null>("load_pending_desktop_route"); }
export function acknowledgeDesktopRoute(routeId: string, expectedChannelId: string) { nativeOnly(); return invoke<void>("acknowledge_desktop_route", { routeId, expectedChannelId }); }
export function dismissDesktopRoute(routeId: string) { nativeOnly(); return invoke<void>("dismiss_desktop_route", { routeId }); }
export function loadDesktopActivity(expectedChannelId: string) { nativeOnly(); return invoke<DesktopActivity[]>("load_desktop_activity", { expectedChannelId }); }
export function openDesktopActivity(routeId: string, expectedChannelId: string) { nativeOnly(); return invoke<void>("open_desktop_activity", { routeId, expectedChannelId }); }
export function desktopPreferencesError(preferences: DesktopPreferences, startupEnabled: boolean | undefined, startupApproved: boolean): string | undefined { if (preferences.keepRunningOnClose && !preferences.trayEnabled) return "Enable the tray before keeping the app running when its window closes."; if (preferences.runOnLogin && startupEnabled !== true && !startupApproved) return "Separately approve launching this app when you sign in."; return undefined; }
export const desktopActivityLabels: Record<DesktopActivity["kind"], string> = { batch_complete: "Batch uploads finished", needs_attention: "Needs attention", quota_pause: "Quota paused" };
export function desktopDeliveryLabel(outcome: string) { return ({ submitted: "Submitted to the desktop", permission_denied: "Blocked by notification permission", permission_unknown: "Notification availability unknown", submission_failed: "Desktop submission failed", submission_unknown: "Desktop outcome unknown", unsupported: "Unavailable on this platform", suppressed: "Suppressed by saved notification choices", pending: "Waiting for desktop submission" } as Record<string, string>)[outcome] || "Desktop outcome unknown"; }
