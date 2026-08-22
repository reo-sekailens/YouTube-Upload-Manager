export const comparisonPositionMaximum = 86_400;
export const youtubeComparisonOrigin = "https://www.youtube.com";

/** Uses YouTube's standard embed host so an operator's Tauri WebView session can be available to the frame. */
export function youtubeComparisonEmbedUrl(videoId: string): string {
  return `${youtubeComparisonOrigin}/embed/${encodeURIComponent(videoId)}?enablejsapi=1&playsinline=1&rel=0`;
}

/** Frames mount only after the operator starts playback, and remain mounted until explicitly closed. */
export function shouldLoadComparisonPlayers(playing: boolean, alreadyLoaded: boolean): boolean {
  return playing || alreadyLoaded;
}

export function sharedComparisonDuration(durations: number[]): number {
  const knownDurations = durations.filter((duration) => Number.isFinite(duration) && duration > 0);
  return knownDurations.length > 0 ? Math.min(...knownDurations) : comparisonPositionMaximum;
}

export function clampComparisonPosition(seconds: number, maximum = comparisonPositionMaximum): number {
  return Math.min(maximum, Math.max(0, seconds));
}

export function moveComparisonPosition(position: number, seconds: number, maximum = comparisonPositionMaximum): number {
  return clampComparisonPosition(position + seconds, maximum);
}
