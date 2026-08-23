import { listen } from "@tauri-apps/api/event";
import type {
  DashboardSnapshot,
  StateChange,
  StateChangeBatch,
  UploadItem,
} from "./types";

export const LOCAL_STATE_CHANGE_EVENT = "local-state-change";

export type StateBatchDisposition =
  | "apply"
  | "stale"
  | "cross_channel"
  | "gap"
  | "reset";

export type ClassifiedStateChangeBatch = {
  disposition: StateBatchDisposition;
  changes: StateChange[];
};

type StateChangeSubscriber = (batch: StateChangeBatch) => void;

const subscribers = new Set<StateChangeSubscriber>();
let nativeUnlisten: (() => void) | undefined;
let installPromise: Promise<void> | undefined;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isFiniteRevision(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}

function parseStateChange(value: unknown): StateChange | undefined {
  if (!isRecord(value)) return undefined;
  if (
    !isFiniteRevision(value.revision) ||
    typeof value.channelId !== "string" ||
    typeof value.surface !== "string" ||
    value.surface.length === 0 ||
    typeof value.entityId !== "string" ||
    typeof value.eventKind !== "string" ||
    value.eventKind.length === 0
  )
    return undefined;
  // A disconnect has no active provider channel. Native emits that one
  // connection invalidation with an empty channel ID so mounted workspaces can
  // discard their old channel projection; no entity data may use this form.
  if (value.channelId.length === 0 && value.surface !== "connection")
    return undefined;
  return {
    revision: value.revision,
    channelId: value.channelId,
    surface: value.surface,
    entityId: value.entityId,
    eventKind: value.eventKind,
    payload: value.payload,
  };
}

/** Validates the native boundary and also accepts a single change for forward compatibility. */
export function normalizeStateChangeBatch(
  payload: unknown,
): StateChangeBatch | undefined {
  const single = parseStateChange(payload);
  if (single) {
    return {
      fromRevision: Math.max(0, single.revision - 1),
      toRevision: single.revision,
      resetRequired: false,
      changes: [single],
    };
  }
  if (!isRecord(payload)) return undefined;
  if (
    !isFiniteRevision(payload.fromRevision) ||
    !isFiniteRevision(payload.toRevision) ||
    payload.toRevision < payload.fromRevision ||
    typeof payload.resetRequired !== "boolean" ||
    !Array.isArray(payload.changes)
  )
    return undefined;
  const changes = payload.changes.map(parseStateChange);
  if (changes.some((change) => change === undefined)) return undefined;
  return {
    fromRevision: payload.fromRevision,
    toRevision: payload.toRevision,
    resetRequired: payload.resetRequired,
    changes: changes as StateChange[],
  };
}

async function ensureNativeListener() {
  if (nativeUnlisten || installPromise) return installPromise;
  installPromise = listen<unknown>(LOCAL_STATE_CHANGE_EVENT, (event) => {
    const batch = normalizeStateChangeBatch(event.payload);
    if (!batch) return;
    for (const subscriber of [...subscribers]) {
      try {
        subscriber(batch);
      } catch {
        // One workspace must not prevent other mounted workspaces from converging.
      }
    }
  })
    .then((unlisten) => {
      if (subscribers.size === 0) unlisten();
      else nativeUnlisten = unlisten;
    })
    .finally(() => {
      installPromise = undefined;
    });
  return installPromise;
}

/** Shares exactly one native event listener across every mounted workspace. */
export async function subscribeLocalStateChanges(
  subscriber: StateChangeSubscriber,
): Promise<() => void> {
  subscribers.add(subscriber);
  try {
    await ensureNativeListener();
  } catch (error) {
    subscribers.delete(subscriber);
    throw error;
  }
  let subscribed = true;
  return () => {
    if (!subscribed) return;
    subscribed = false;
    subscribers.delete(subscriber);
    if (subscribers.size === 0 && nativeUnlisten) {
      const unlisten = nativeUnlisten;
      nativeUnlisten = undefined;
      unlisten();
    }
  };
}

/** Classifies one envelope against the current channel cursor without touching React state. */
export function classifyStateChangeBatch(
  currentRevision: number,
  expectedChannelId: string,
  batch: StateChangeBatch,
): ClassifiedStateChangeBatch {
  if (
    batch.changes.some((change) => change.channelId !== expectedChannelId)
  )
    return { disposition: "cross_channel", changes: [] };
  if (batch.resetRequired) return { disposition: "reset", changes: [] };
  if (batch.toRevision <= currentRevision)
    return { disposition: "stale", changes: [] };
  // Native revisions are globally monotonic. Channel-filtered/coalesced batches
  // can therefore skip revision numbers, but must start at the exact cursor the
  // webview requested or last applied.
  if (batch.fromRevision !== currentRevision)
    return { disposition: "gap", changes: [] };
  const changes = batch.changes.filter((change) => change.revision > currentRevision);
  return { disposition: "apply", changes };
}

function uploadFromPayload(payload: unknown): UploadItem | undefined {
  let parsed = payload;
  if (typeof parsed === "string") {
    try {
      parsed = JSON.parse(parsed) as unknown;
    } catch {
      return undefined;
    }
  }
  if (
    !isRecord(parsed) ||
    typeof parsed.id !== "string" ||
    typeof parsed.title !== "string" ||
    typeof parsed.fileName !== "string" ||
    typeof parsed.status !== "string" ||
    typeof parsed.updatedAt !== "string" ||
    typeof parsed.sizeBytes !== "number" ||
    typeof parsed.confirmedBytes !== "number" ||
    typeof parsed.totalBytes !== "number" ||
    typeof parsed.visibility !== "string" ||
    typeof parsed.madeForKids !== "boolean"
  )
    return undefined;
  return parsed as UploadItem;
}

export type DashboardStateReduction = {
  snapshot: DashboardSnapshot;
  invalidatedSurfaces: string[];
};

/** Applies upload deltas while retaining full-snapshot invalidations for other projections. */
export function reduceDashboardState(
  snapshot: DashboardSnapshot,
  expectedChannelId: string,
  batch: StateChangeBatch,
): DashboardStateReduction {
  if (
    snapshot.activeChannelId !== undefined &&
    snapshot.activeChannelId !== expectedChannelId
  )
    return { snapshot, invalidatedSurfaces: [] };
  const classified = classifyStateChangeBatch(
    snapshot.revision,
    expectedChannelId,
    batch,
  );
  if (classified.disposition !== "apply")
    return { snapshot, invalidatedSurfaces: [] };

  let items = snapshot.items;
  const invalidated = new Set<string>();
  for (const change of classified.changes) {
    if (change.surface !== "upload") {
      if (change.surface !== "preflight") invalidated.add(change.surface);
      continue;
    }
    if (["delete", "deleted", "remove", "removed"].includes(change.eventKind)) {
      items = items.filter((item) => item.id !== change.entityId);
      continue;
    }
    const upload = uploadFromPayload(change.payload);
    if (!upload || upload.id !== change.entityId) {
      invalidated.add("upload");
      continue;
    }
    const index = items.findIndex((item) => item.id === upload.id);
    if (index < 0) items = [upload, ...items];
    else {
      items = [...items];
      items[index] = upload;
    }
  }
  return {
    snapshot: {
      ...snapshot,
      activeChannelId: expectedChannelId,
      items,
      revision: batch.toRevision,
    },
    invalidatedSurfaces: [...invalidated],
  };
}

/** Test-only reset for module singleton isolation. */
export function resetStateChangeSubscriptionForTests() {
  subscribers.clear();
  nativeUnlisten?.();
  nativeUnlisten = undefined;
  installPromise = undefined;
}
