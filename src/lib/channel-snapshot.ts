import type { DashboardSnapshot } from "./types";

export type ChannelSnapshotCursor = {
  channelId?: string;
  generation: number;
  revision: number;
};

/** A response belongs to the connection that requested it, including reconnects. */
export function canAcceptChannelSnapshot(
  requested: ChannelSnapshotCursor,
  current: ChannelSnapshotCursor,
  snapshot: DashboardSnapshot,
): boolean {
  return requested.generation === current.generation &&
    requested.channelId === current.channelId &&
    snapshot.activeChannelId === current.channelId &&
    snapshot.revision >= current.revision;
}
