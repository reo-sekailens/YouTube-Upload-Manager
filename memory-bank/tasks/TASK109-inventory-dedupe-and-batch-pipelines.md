# TASK109: Inventory, dedupe, and batch pipelines

## Status

proposed

## Objective

Make large-channel inventory, duplicate review, preflight, folder observation,
and multi-file intake scale with changes and results instead of repeated
all-record or pairwise work.

## Scope

- Stage inventory pages with prepared statements in one bounded transaction and
  atomically promote a complete channel generation.
- Persist normalized, canonical-copy, and numeric-sequence title keys; replace
  all-pairs duplicate comparison with grouped candidate generation.
- Rebuild revisioned duplicate projections only when the channel inventory or
  local-upload generation changes.
- Share one sufficiently fresh inventory generation across a dispatch wave
  without weakening the final native pre-upload gate.
- Materialize preflight match evidence on worker completion; page files and
  activity, and load deep metadata only on expansion.
- Bulk-load folder observations and upload state rather than querying once per
  discovered file.
- Replace sequential frontend import/queue IPC waterfalls with batch commands
  that return receipts and schedule per-item disk work through bounded workers.

## Acceptance criteria

- Existing exact, canonical trailing-copy `(2)`, numeric-sequence, and exact
  digest semantics remain explainable and are protected by golden tests.
- A 10,000-video duplicate rebuild and unchanged dashboard read meet TASK103
  CPU/query budgets without quadratic pair comparison.
- A 1,000-file preflight against 10,000 inventory rows has a status-read p95
  below 50 ms and a bounded payload below 256 KiB on the reference fixture.
- A 100-file import/queue submission uses O(1) webview/native round trips while
  reporting independent per-item validation failures.
- Inventory promotion is transactional; partial provider pages never replace a
  previously complete generation.
- Every table, key, projection, batch receipt, and result remains scoped to the
  immutable channel/account identity.

## Dependencies

TASK103, TASK104, TASK108.

## Affected areas

Remote inventory, title matching, dashboard projections, preflight, watched
folders, batch bridge commands, persistence indexes, and scale fixtures.
