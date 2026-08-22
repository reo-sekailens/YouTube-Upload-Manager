# TASK106: Lazy webview and render isolation

## Status

proposed

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
