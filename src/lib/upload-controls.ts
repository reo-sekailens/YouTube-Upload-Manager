import { invoke, isTauri } from "@tauri-apps/api/core";

/** Days are Sunday=0 through Saturday=6; an overnight window starts on its chosen day. */
export type UploadLocalWindow = { days: number[]; start: string; end: string };
export type UploadTimedBudget = UploadLocalWindow & { bytesPerSecond: number };
export type UploadTransferSettings = { bytesPerSecond?: number; windows: UploadLocalWindow[]; timedLimits: UploadTimedBudget[] };
export type UploadPriorityChoice = { kind: "batch" | "folder"; id: string; priority: number };
export type UploadPriorityTarget = UploadPriorityChoice & { title: string; queuedItems: number };
export type UploadWindowStatus = { allowed: boolean; waitingReason?: string; nextWindowAt?: string; effectiveBytesPerSecond?: number };
export type UploadControlsOverview = {
  channelId: string; settings: UploadTransferSettings; priorities: UploadPriorityChoice[]; targets: UploadPriorityTarget[];
  timeZone: string; currentOffsetMinutes: number; reviewedOffsetMinutes?: number; timezoneChanged: boolean; updatedAt?: string; status: UploadWindowStatus;
};

export async function loadUploadControls(expectedChannelId: string): Promise<UploadControlsOverview> {
  if (!isTauri()) throw new Error("Upload controls are available in the desktop app.");
  return invoke("load_upload_controls", { expectedChannelId });
}
export async function saveUploadControls(expectedChannelId: string, settings: UploadTransferSettings, priorities: UploadPriorityChoice[], approved: boolean, expectedOffsetMinutes?: number): Promise<UploadControlsOverview> {
  return invoke("save_upload_controls", { expectedChannelId, settings, priorities, approved, ...(expectedOffsetMinutes !== undefined ? { expectedOffsetMinutes } : {}) });
}

export function uploadBudgetMbps(bytesPerSecond?: number): string { return bytesPerSecond === undefined ? "Unlimited" : `${Number((bytesPerSecond * 8 / 1_000_000).toFixed(3))} Mbps`; }
export function uploadBudgetFromMbps(value: string): number { return Math.round(Number(value) * 1_000_000 / 8); }
export function uploadBudgetInput(bytesPerSecond?: number): string { return bytesPerSecond === undefined ? "10" : String(Number((bytesPerSecond * 8 / 1_000_000).toFixed(6))); }
export const uploadWeekdays = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
export function uploadWindowLabel(window: UploadLocalWindow): string { return `${window.days.map((day) => uploadWeekdays[day]?.slice(0, 3) || "?").join(", ")} · ${window.start}–${window.end}${window.end < window.start ? " (ends next day)" : ""}`; }

export function uploadControlsValidation(settings: UploadTransferSettings, priorities: UploadPriorityChoice[]): string | undefined {
  const rateValid = (value: number) => Number.isSafeInteger(value) && value >= 1024 && value <= 1024 ** 3;
  if ((settings.bytesPerSecond !== undefined && !rateValid(settings.bytesPerSecond)) || settings.timedLimits.some((limit) => !rateValid(limit.bytesPerSecond))) return "Choose a budget between 0.009 and 8,589 Mbps, or use unlimited.";
  if (settings.windows.length > 14 || settings.timedLimits.length > 14) return "Use at most 14 upload windows and 14 timed budgets.";
  for (const window of [...settings.windows, ...settings.timedLimits]) {
    if (!window.days.length || window.days.some((day) => !Number.isInteger(day) || day < 0 || day > 6) || new Set(window.days).size !== window.days.length) return "Choose each weekday once, with at least one selected day per window.";
    if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(window.start) || !/^([01]\d|2[0-3]):[0-5]\d$/.test(window.end)) return "Choose valid local start and end times.";
    if (window.start === window.end) return "Window start and end must differ. Remove all windows to upload at any time.";
  }
  if (priorities.length > 200 || priorities.some((choice) => !["batch", "folder"].includes(choice.kind) || !choice.id || !Number.isInteger(choice.priority) || choice.priority < 0 || choice.priority > 5) || new Set(priorities.map((choice) => `${choice.kind}:${choice.id}`)).size !== priorities.length) return "Choose each batch or folder once, with a priority from 0 to 5.";
  return undefined;
}

export function uploadControlsMatchesChannel(result: UploadControlsOverview, expectedChannelId: string): boolean {
  return result.channelId === expectedChannelId && result.targets.every((target) => ["batch", "folder"].includes(target.kind) && Boolean(target.id)) && new Set(result.targets.map((target) => `${target.kind}:${target.id}`)).size === result.targets.length;
}
