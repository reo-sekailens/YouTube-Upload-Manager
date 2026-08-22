# TASK100: Pre-ingest duplicate scan responsiveness

## Status

completed

## Scope

Keep the pre-ingest duplicate-review UI responsive while light or deep native
scan work is running.

## Acceptance criteria

- The UI poll reads persisted scan state without hashing files, probing media,
  or doing per-file query fan-out.
- Expensive BLAKE3 and metadata work remains on native background workers.
- A deletion content-binding hash happens only after the operator explicitly
  asks to prepare deletion for a reviewed match.

## Evidence

- `load_preflight_scan` now returns unavailable metadata until the dedicated
  metadata worker has persisted it, rather than traversing media containers on
  every poll.
- Local upload evidence, source-drop groups, and processed YouTube records are
  prefetched once per snapshot; result rows reuse those in-memory indexes.
- Eager deletion-token generation was removed. The explicit deletion-prep
  command retains the content-binding hash at the destructive-action boundary.
- A native regression test verifies a deep matching result is delete-eligible
  without receiving a token during normal polling.
