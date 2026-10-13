export type PlaylistAddProgress = {
  channelId: string;
  playlistId: string;
  requestId?: string;
  completed: number;
  total: number;
  added: number;
  skipped: number;
  currentTitle: string;
  status: "running" | "completed" | "error";
  detail: string;
};

/** Ignore late or malformed events from another channel, destination or request. */
export function isPlaylistAddProgressForScope(
  payload: unknown, channelId: string, playlistId: string, requestId: string,
): payload is PlaylistAddProgress {
  if (!payload || typeof payload !== "object") return false;
  const progress = payload as PlaylistAddProgress;
  return progress.channelId === channelId && progress.playlistId === playlistId
    && progress.requestId === requestId
    && Number.isSafeInteger(progress.total) && progress.total >= 0 && progress.total <= 200
    && Number.isSafeInteger(progress.completed) && progress.completed >= 0 && progress.completed <= progress.total
    && Number.isSafeInteger(progress.added) && progress.added >= 0
    && Number.isSafeInteger(progress.skipped) && progress.skipped >= 0
    && progress.added + progress.skipped === progress.completed
    && ["running", "completed", "error"].includes(progress.status)
    && typeof progress.currentTitle === "string" && typeof progress.detail === "string";
}
