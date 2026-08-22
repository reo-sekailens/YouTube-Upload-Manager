# TASK076: Universal upload light dedupe

## Status

completed

## Scope

Require native normalized filename/title light dedupe before every manual,
batch, queued, and watched-folder upload can dispatch.

## Acceptance criteria

- Every native upload dispatch re-syncs the active YouTube library and blocks
  matching processed titles for review.
- Manual batches and queues also compare titles inside the active local batch
  or queue, using the same separator/capture-sequence normalization.
- Watched folders reject a light match before copying into managed storage.
- A false-positive remains an explicit, auditable Upload anyway choice.

## Evidence

- Every transfer worker re-syncs inventory and fails closed when the light
  duplicate check cannot complete.
- Added local batch/queue normalized-title matching and verified channel/batch
  filtering in `light_dedupe_catches_matching_titles_in_the_current_batch_and_active_queue`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 51 tests; `npm test` passed: 35 tests; `npm run build` passed.
