# TASK080: Visible upload queue clearing

## Status

completed

## Scope

Make the global and per-item upload-queue controls actually remove unfinished
items from the dashboard while retaining local media and resumable evidence.

## Acceptance criteria

- Clear upload queue hides every unfinished upload state after it is cancelled.
- Every draft, queued, failed, importing, reconciling, dispatching, or uploading
  item has an individual remove/cancel action.
- An active transfer observes cancellation at its next persisted chunk boundary.
- Completed uploads cannot be removed through queue controls.

## Evidence

- The native full-queue and per-item commands now persist `cancelled` for every
  unfinished upload state; the dashboard intentionally excludes those records
  while retaining media and resumable evidence.
- Active upload workers already read the persisted status before each chunk and
  stop at that checkpoint when cancellation is observed.
- The local bridge test verifies the individual cancellation command arguments.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 51 tests; `npm test` passed: 35 tests; `npm run build` passed.
