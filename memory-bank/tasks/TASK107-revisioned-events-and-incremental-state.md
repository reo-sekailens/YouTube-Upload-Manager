# TASK107: Revisioned events and incremental state

## Status

completed

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

## Implemented

- Added durable schema-v2 `state_changes` revisions and compact, immutable
  channel-scoped upload, preflight, connection, deletion, folder-monitor,
  dedupe, inventory, and quota surfaces. The native catch-up envelope retains
  the requested cursor, advances across intentionally non-contiguous global
  revisions, coalesces the latest entity state, and requests a snapshot reset
  when retained history cannot cover a cursor.
- Added one SQLite commit-hook-driven dispatcher with a fixed 100 ms coalescing
  window. It blocks without a timer while idle; 200 durable progress rows are
  covered by one sub-2 KiB upload delta in the regression fixture.
- Added a singleton frontend listener before catch-up, serialized batch
  application, incremental upload upsert/removal, compact preflight progress,
  event-coalesced invalidation refreshes, stale/cross-channel rejection, and
  snapshot recovery for gaps, reloads, reconnects, and listener races.
- Isolated the complete event bridge in a lazy component that mounts only after
  the safe startup fence and an immutable active channel are ready. The lazy
  module still attaches its listener before durable catch-up, so code splitting
  cannot introduce a missed-update window. Native video and preflight pickers
  are likewise loaded only after the corresponding operator action.
- Removed every frontend `setInterval`. OAuth and deletion receipt recovery use
  bounded exponential `setTimeout` only while authorization is active.
- Replaced permanent folder/quota polling workers with conditional deadline
  loops. Disabled monitoring starts no worker; quota recovery sleeps until its
  durable per-channel deadline and exits when no pause remains.
- Migrated preflight jobs, folder observations, audit records, ignored duplicate
  decisions, and quota pauses to immutable channel scope. The v1 migration
  preserves ignored candidates, and folder observation identity/indexes now use
  channel ID rather than a mutable display name.

## Evidence

- Final integrated native evidence passed 127 tests, failed zero, and ignored
  five release-only benchmarks. The focused state-event, quota-lifecycle, and
  daily-limit persistence regressions remain included.
- `cargo check --lib` passed.
- `npm test -- --run`: 75 passed, 0 failed, including the event-contract tests,
  listener/catch-up ordering, cross-workspace isolation, and native budget
  parser coverage.
- `npm run check`, `npm run build`, and
  `npm run performance:frontend:check`: passed. The initial production
  JavaScript is 230,478 bytes raw / 71,657 bytes
  gzip against budgets of 240,640 / 71,680 bytes; initial CSS is 38,470 bytes
  against a 40,960-byte budget.
- `npm run performance:native:check -- --configuration-only`: passed with one
  startup-resident dispatcher, one direct spawn wrapper, and exactly one
  conditional folder and quota deadline loop.
- Source inspection finds no `setInterval` in `src/`; remaining timers are the
  event-coalesced invalidation window, bounded authorization fallback, and the
  opt-in performance harness marker.
- Browser production-preview QA passed at `http://127.0.0.1:4173/`: the lazy
  Folder monitor workspace rendered and switched tabs with no console warning
  or error. Screenshot:
  `output/task107-event-driven-folder-workspace.png`.
- The authoritative unsigned packaged empty-profile run contains 80 cold and
  80 warm launches. Every run recorded zero settled-idle periodic invokes,
  database opens, SQLite statements, event messages, worker threads, and
  FFprobe processes during the bounded two-second idle window.

## Follow-ups

- TASK112 owns final interrupted-profile, standard unsigned-production,
  signed-production, and live-provider certification. The instrumented unsigned
  package closes the settled-idle counter slice only.
