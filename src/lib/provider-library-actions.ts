import type { ConnectionSettings, DashboardSnapshot } from "./types";
import type { DedupeActivityState, DedupeProgressPhase } from "./dedupe-activity";

/** Explicit library refresh shares the same lazy provider-workflow boundary. */
export async function refreshChannelLibrary({ channel, current, sync, refresh, connection, accept, invalidate, notice }: {
  channel: string; current: () => boolean; sync: () => Promise<number>;
  refresh: () => Promise<DashboardSnapshot>; connection: () => Promise<ConnectionSettings>;
  accept: (settings: ConnectionSettings) => void; invalidate: () => void; notice: (message: string) => void;
}) {
  if (!current()) return;
  notice(`Refreshing ${channel}'s YouTube library…`);
  const synced = await sync();
  if (!current()) return;
  await refresh();
  if (!current()) return;
  const settings = await connection();
  if (!current()) return;
  accept(settings);
  invalidate();
  notice(`Library refreshed: ${synced} YouTube video${synced === 1 ? "" : "s"} saved locally for ${channel}. Eligible uploads resume automatically.`);
}

/** Loaded on explicit duplicate review, keeping provider-workflow copy out of startup. */
export async function refreshDedupeInventory({ channel, current, sync, refresh, phase, log, notice, report }: {
  channel: string;
  current: () => boolean;
  sync: () => Promise<number>;
  refresh: () => Promise<DashboardSnapshot>;
  phase: (value: DedupeProgressPhase) => void;
  log: (state: DedupeActivityState, message: string) => void;
  notice: (message: string) => void;
  report: (error: unknown) => Promise<string>;
}) {
  if (!current()) return;
  log("running", `Started duplicate detection for ${channel}.`);
  log("running", "Synchronizing this channel's uploaded-video inventory from YouTube…");
  notice(`Checking ${channel}'s uploaded-video titles for duplicates…`);
  try {
    const synced = await sync();
    if (!current()) return;
    log("success", `Synced ${synced} uploaded video${synced === 1 ? "" : "s"} into this device's channel inventory.`);
    phase("rebuilding");
    log("running", "Rebuilding normalized exact-title and numbered-copy candidates locally…");
    const refreshed = await refresh();
    if (!current()) return;
    const candidateCount = refreshed.duplicates.length;
    log("success", `Ready for review: ${candidateCount} duplicate candidate${candidateCount === 1 ? "" : "s"}. No videos were removed.`);
    phase("complete");
    notice(`Dedupe complete for ${channel}: ${synced} uploaded video${synced === 1 ? "" : "s"} checked and ${candidateCount} candidate${candidateCount === 1 ? "" : "s"} ready for review.`);
  } catch (error) {
    if (!current()) return;
    const message = await report(error);
    if (!current()) return;
    phase("error");
    log("error", `Dedupe stopped: ${message}`);
    notice(`Dedupe could not run: ${message}`);
  }
}
