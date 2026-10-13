import type { PlaylistAddReceipt } from "./types";

export const PLAYLIST_SELECTION_LIMIT = 200;

/** Add IDs in the reviewed display order without losing earlier selections. */
export function addPlaylistSelection(selected: ReadonlySet<string>, ids: readonly string[]) {
  const next = new Set(selected);
  let omitted = 0;
  for (const id of new Set(ids)) {
    if (next.has(id)) continue;
    if (next.size < PLAYLIST_SELECTION_LIMIT) next.add(id);
    else omitted += 1;
  }
  return { selected: next, omitted };
}

/** The anchor may be on another page, but must remain in the filtered order. */
export function playlistRangeIds(orderedIds: readonly string[], anchorId: string | undefined, targetId: string) {
  const target = orderedIds.indexOf(targetId);
  const anchor = anchorId ? orderedIds.indexOf(anchorId) : -1;
  if (target === -1) return [];
  if (anchor === -1) return [targetId];
  return orderedIds.slice(Math.min(anchor, target), Math.max(anchor, target) + 1);
}

/** Only a complete, channel-bound partition may remove confirmed selections. */
export function isPlaylistAddReceiptForSelection(receipt: PlaylistAddReceipt, channelId: string, playlistId: string, requestedIds: readonly string[]) {
  if (receipt.channelId !== channelId || receipt.playlistId !== playlistId
    || !Array.isArray(receipt.requestedVideoIds) || receipt.requestedVideoIds.length !== requestedIds.length
    || receipt.requestedVideoIds.some((id, index) => id !== requestedIds[index])) return false;
  const groups = [receipt.addedVideoIds, receipt.skippedVideoIds, receipt.remainingVideoIds];
  if (groups.some((ids) => !Array.isArray(ids))) return false;
  const returned = groups.flat();
  const expected = new Set(requestedIds);
  return expected.size === requestedIds.length && returned.length === expected.size
    && new Set(returned).size === returned.length && returned.every((id) => expected.has(id));
}
