# TASK106: Lazy webview and render isolation

## Status

completed

## Objective

Make the Batch shell fast by preventing inactive workspaces, large lists, and
unrelated state changes from loading or rendering until needed.

## Scope

- Mount only the active workspace; lazy-load monitor, dedupe, deletion,
  transfer, account, diagnostics, comparison players, and their CSS.
- Keep the crash boundary and minimal shell eager.
- Centralize connection/deletion authorization state and remove per-candidate
  settings reads and message listeners.
- Stabilize callbacks, especially FolderMonitorPanel onQueueRefresh, and split
  App state into ownership-specific controllers/stores.
- Page/window queue, deletion inventory, duplicate candidates, and preflight
  results; construct full metadata only when expanded.
- Use deferred searches and memoized rows where profiling shows expensive
  filtering or reconciliation.
- Load dialog, opener, and window APIs only on the interactions that require
  them; remove the mixed static/dynamic opener warning.
- Preserve draft/form state intentionally across tab changes through local
  persistence or explicit state ownership.

## Acceptance criteria

- Before first Batch interaction, inactive tabs create zero DOM, effects,
  invokes, timers, message listeners, or provider calls.
- Initial JavaScript is at most 235 KiB raw / 70 KiB gzip and initial CSS at
  most 40 KiB raw unless TASK103 evidence records a better justified budget.
- A 10,000-record fixture mounts fewer than 100 data rows/cards.
- Search key-to-paint p95 is below 100 ms and reference interactions have no
  task above 50 ms.
- Keyboard, focus, screen-reader, selection, and tab-state behavior remain
  correct.

## Dependencies

TASK103.

## Affected paths

src/App.tsx, src/main.tsx, src/components/, src/styles.css, src/lib/, Vite
configuration, and component/browser performance tests.

## Evidence

- Only the active workspace mounts. Feature components and feature CSS are lazy
  chunks; dialog, opener, and window APIs load only for their interactions. The
  mixed static/dynamic opener warning is gone.
- Queue, duplicate, deletion, preflight, and activity surfaces page at 32
  records. Component and browser fixtures prove at most 32 rendered records per
  single list from 10,000 inputs while retaining explicit
  all-results selection semantics.
- Initial production assets pass the fixed TASK103 gates: 230,478 B JavaScript
  raw, 71,657 B JavaScript gzip, and 38,470 B CSS raw.
- Frontend validation passed 75 tests, typecheck, and warning-free production
  build. In-app Browser QA at `http://127.0.0.1:1420/` proved one active
  tabpanel, inactive Batch DOM removal, lazy Duplicate/Transfer rendering,
  keyboard selection/focus movement, and zero console warnings/errors.
- The final local-browser 10,000-row Batch run measured 40 searches at 44/87 ms
  p50/p95 and 40 clear interactions at 42/85 ms, mounted at most 32 rows, and
  recorded zero interaction Long Tasks and runtime errors. This is browser
  evidence; signed packaged-Tauri validation remains part of TASK112.
