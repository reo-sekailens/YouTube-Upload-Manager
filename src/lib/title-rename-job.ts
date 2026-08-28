import type { VideoTitleRename } from "./types";

export type RenameActivity = VideoTitleRename & {
  status: "pending" | "running" | "completed" | "failed";
  detail?: string;
};

export type TitleRenameJobSnapshot = {
  activity: RenameActivity[];
  applying: boolean;
};

type RenameExecutor = (changes: VideoTitleRename[]) => Promise<unknown>;

function renameFailureDetail(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (typeof error === "object" && error && "message" in error && typeof error.message === "string")
    return error.message;
  return "YouTube did not confirm this title change.";
}

let snapshot: TitleRenameJobSnapshot = { activity: [], applying: false };
const listeners = new Set<() => void>();

function publish(next: TitleRenameJobSnapshot) {
  snapshot = next;
  listeners.forEach((listener) => listener());
}

function updateActivity(update: (activity: RenameActivity[]) => RenameActivity[]) {
  publish({ ...snapshot, activity: update(snapshot.activity) });
}

export function subscribeTitleRenameJob(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getTitleRenameJobSnapshot() {
  return snapshot;
}

/**
 * This job belongs to the app session rather than the Rename tab, so changing
 * workspaces cannot cancel a confirmed provider operation or discard its log.
 */
export async function startTitleRenameJob(
  changes: VideoTitleRename[],
  execute: RenameExecutor,
  onNotice: (notice: string) => void,
) {
  if (snapshot.applying) throw new Error("A title-rename job is already running.");
  publish({
    applying: true,
    activity: changes.map((change) => ({ ...change, status: "pending" })),
  });
  try {
    for (const change of changes) {
      updateActivity((activity) => activity.map((item) => item.videoId === change.videoId
        ? { ...item, status: "running", detail: "Sending reviewed title change to YouTube…" }
        : item));
      try {
        await execute([change]);
        updateActivity((activity) => activity.map((item) => item.videoId === change.videoId
          ? { ...item, status: "completed", detail: "YouTube confirmed the title change." }
          : item));
      } catch (error) {
        const detail = renameFailureDetail(error);
        updateActivity((activity) => activity.map((item) => item.videoId === change.videoId
          ? { ...item, status: "failed", detail }
          : item));
        onNotice(`${detail} The remaining title changes were left pending.`);
        return false;
      }
    }
    onNotice(`YouTube confirmed ${changes.length} reviewed video title change${changes.length === 1 ? "" : "s"}. A local audit receipt was saved.`);
    return true;
  } finally {
    publish({ ...snapshot, applying: false });
  }
}
