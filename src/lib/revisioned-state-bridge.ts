import { useEffect } from "react";
import {
  loadConnectionSettings,
  loadPreflightDuplicateScan,
  loadSnapshot,
  loadStateChanges,
} from "./local";
import {
  classifyStateChangeBatch,
  reduceDashboardState,
  subscribeLocalStateChanges,
} from "./state-events";
import type {
  ConnectionSettings,
  DashboardSnapshot,
  PreIngestDuplicateScan,
  StateChange,
  StateChangeBatch,
} from "./types";

type StateSetter<T> = (value: T | ((current: T) => T)) => void;

export type RevisionedStateBridgeOptions = [
  string,
  DashboardSnapshot,
  { current: string | undefined },
  { current: number },
  StateSetter<ConnectionSettings | undefined>,
  StateSetter<DashboardSnapshot>,
  StateSetter<PreIngestDuplicateScan | undefined>,
  StateSetter<number>,
  (settings: ConnectionSettings) => void,
  (jobId: string) => Promise<void>,
];

type PreflightStatePayload = {
  jobId: string;
  status: string;
  totalFiles: number;
  completedFiles: number;
  pendingMetadataFiles: number;
};

function preflightStatePayload(
  change: StateChange,
): PreflightStatePayload | undefined {
  if (
    change.surface !== "preflight" ||
    typeof change.payload !== "object" ||
    change.payload === null
  )
    return undefined;
  const payload = change.payload as Record<string, unknown>;
  const jobId =
    typeof payload.jobId === "string" ? payload.jobId : change.entityId;
  if (
    !jobId ||
    typeof payload.status !== "string" ||
    typeof payload.totalFiles !== "number" ||
    typeof payload.completedFiles !== "number" ||
    typeof payload.pendingMetadataFiles !== "number"
  )
    return undefined;
  return {
    jobId,
    status: payload.status,
    totalFiles: payload.totalFiles,
    completedFiles: payload.completedFiles,
    pendingMetadataFiles: payload.pendingMetadataFiles,
  };
}

/** Installs the listener first, then closes the lazy-module/bootstrap gap from SQLite. */
export function installRevisionedStateBridge([
  channelId,
  snapshot,
  channelIdRef,
  revisionRef,
  setConnectionSettings,
  setSnapshot,
  setPreflightScan,
  setPreflightFileCount,
  updateConnection,
  loadFinalPreflightResult,
]: RevisionedStateBridgeOptions): () => void {
  let active = true;
  let unsubscribe: (() => void) | undefined;
  let invalidationTimer: number | undefined;
  let sequence = Promise.resolve();

  if (channelIdRef.current !== channelId) {
    channelIdRef.current = channelId;
    revisionRef.current =
      snapshot.activeChannelId === channelId ? snapshot.revision : 0;
  }

  const replaceFromSourceOfTruth = async () => {
    const [nextSnapshot, nextConnection] = await Promise.all([
      loadSnapshot(),
      loadConnectionSettings(),
    ]);
    if (!active) return;
    if (nextConnection.activeChannelId !== channelId) {
      updateConnection(nextConnection);
      return;
    }
    if (nextSnapshot.activeChannelId !== channelId) return;
    if (nextSnapshot.revision < revisionRef.current) return;
    channelIdRef.current = channelId;
    revisionRef.current = nextSnapshot.revision;
    setConnectionSettings(nextConnection);
    setSnapshot(nextSnapshot);
  };

  const scheduleInvalidationRefresh = () => {
    if (invalidationTimer !== undefined) return;
    invalidationTimer = window.setTimeout(() => {
      invalidationTimer = undefined;
      void replaceFromSourceOfTruth().catch(() => undefined);
    }, 75);
  };

  const applyPreflightChanges = (changes: StateChange[]) => {
    for (const change of changes) {
      const payload = preflightStatePayload(change);
      if (!payload) continue;
      setPreflightScan((current) =>
        current?.id === payload.jobId
          ? {
              ...current,
              status: payload.status,
              totalFiles: payload.totalFiles,
              completedFiles: payload.completedFiles,
              pendingMetadataFiles: payload.pendingMetadataFiles,
            }
          : current,
      );
      setPreflightFileCount(
        Math.max(0, payload.totalFiles - payload.completedFiles),
      );
      if (
        ["complete", "cancelled"].includes(payload.status) &&
        payload.pendingMetadataFiles === 0
      )
        void loadFinalPreflightResult(payload.jobId);
    }
  };

  const applyBatch = async (
    batch: StateChangeBatch,
    recoverGap: boolean,
  ): Promise<void> => {
    if (!active) return;
    if (batch.changes.some((change) => change.surface === "connection")) {
      void loadConnectionSettings()
        .then((settings) => {
          if (active) updateConnection(settings);
        })
        .catch(() => undefined);
    }
    let classified = classifyStateChangeBatch(
      revisionRef.current,
      channelId,
      batch,
    );
    if (
      classified.disposition === "stale" ||
      classified.disposition === "cross_channel"
    )
      return;
    if (classified.disposition === "reset") {
      await replaceFromSourceOfTruth();
      return;
    }
    if (classified.disposition === "gap") {
      if (!recoverGap) {
        await replaceFromSourceOfTruth();
        return;
      }
      const catchUp = await loadStateChanges(channelId, revisionRef.current);
      await applyBatch(catchUp, false);
      if (!active) return;
      classified = classifyStateChangeBatch(
        revisionRef.current,
        channelId,
        batch,
      );
      if (classified.disposition === "stale") return;
      if (classified.disposition !== "apply") {
        await replaceFromSourceOfTruth();
        return;
      }
    }

    revisionRef.current = batch.toRevision;
    setSnapshot(
      (current) => reduceDashboardState(current, channelId, batch).snapshot,
    );
    applyPreflightChanges(classified.changes);
    if (
      classified.changes.some(
        (change) =>
          change.surface !== "upload" && change.surface !== "preflight",
      )
    )
      scheduleInvalidationRefresh();
  };

  const enqueue = (batch: StateChangeBatch) => {
    sequence = sequence
      .then(() => applyBatch(batch, true))
      .catch(() => replaceFromSourceOfTruth().catch(() => undefined));
  };

  void subscribeLocalStateChanges(enqueue)
    .then((stop) => {
      if (!active) {
        stop();
        return;
      }
      unsubscribe = stop;
      return loadStateChanges(channelId, revisionRef.current).then(enqueue);
    })
    .catch(() => {
      if (active) void replaceFromSourceOfTruth().catch(() => undefined);
    });

  return () => {
    active = false;
    unsubscribe?.();
    if (invalidationTimer !== undefined)
      window.clearTimeout(invalidationTimer);
  };
}

/** Lazy post-start owner for the complete revision listener and recovery effect. */
export default function RevisionedStateBridge({
  options,
}: {
  options: RevisionedStateBridgeOptions;
}) {
  const channelId = options[0];
  const updateConnection = options[8];
  const loadFinalPreflightResult = options[9];
  useEffect(
    () => installRevisionedStateBridge(options),
    [channelId, updateConnection, loadFinalPreflightResult],
  );
  return null;
}
