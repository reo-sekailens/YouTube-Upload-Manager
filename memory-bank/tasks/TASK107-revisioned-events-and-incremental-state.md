# TASK107: Revisioned events and incremental state

## Status

proposed

## Objective

Replace steady full-snapshot polling with compact, recoverable, channel-scoped
updates so active work does not repeatedly query, serialize, transfer, parse,
and render unchanged state.

## Scope

- Define revisioned Tauri events containing immutable channel ID, entity ID,
  revision, event kind, and the smallest safe delta.
- Coalesce upload progress to the latest acknowledged value while keeping every
  acknowledged range durable in SQLite.
- Separate compact preflight job status from paginated result, metadata, and
  activity reads.
- Publish invalidations or deltas for queue, inventory, deletion, monitor, quota,
  and post-processing state; retain paged snapshots for bootstrap and recovery.
- Wake folder and quota work from persisted state changes or deadlines instead
  of permanent fixed-interval loops.
- Reject stale or cross-channel events after an account switch and recover gaps
  by comparing durable revisions.

## Acceptance criteria

- Settled idle performs zero periodic webview invokes, and disabled monitoring
  performs zero recurring scans or database writes.
- Active progress transfers bounded deltas instead of the complete dashboard;
  event rate and payload stay within TASK103 budgets.
- Event loss, listener attachment after bootstrap, webview reload, and process
  relaunch converge to the SQLite source of truth without duplicate actions.
- Account switching cannot expose or apply another channel's event, cache,
  cursor, or projection.
- Polling remains only a bounded, backoff recovery fallback and does not rebuild
  duplicate or preflight projections.
- Existing cancellation, queue, monitor, quota, and crash-recovery behavior is
  covered by native and frontend tests.

## Dependencies

TASK104, TASK105, TASK106.

## Affected areas

Native commands/events, App state ownership, dashboard/preflight/monitor
controllers, bridge types, and recovery tests.
