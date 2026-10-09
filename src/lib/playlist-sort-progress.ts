export type PlaylistSortProgress = {
  channelId: string;
  playlistId: string;
  completed: number;
  total: number;
  currentTitle: string;
  status: "running" | "completed" | "error";
  detail: string;
};

/** Native progress is useful only for the exact playlist this operator started. */
export function isPlaylistSortProgressForScope(payload: unknown, channelId: string, playlistId: string): payload is PlaylistSortProgress {
  if (typeof payload !== "object" || !payload) return false;
  const progress = payload as PlaylistSortProgress;
  return progress.channelId === channelId && progress.playlistId === playlistId
    && Number.isSafeInteger(progress.completed) && progress.completed >= 0
    && Number.isSafeInteger(progress.total) && progress.total >= progress.completed
    && ["running", "completed", "error"].includes(progress.status)
    && typeof progress.currentTitle === "string" && typeof progress.detail === "string";
}
