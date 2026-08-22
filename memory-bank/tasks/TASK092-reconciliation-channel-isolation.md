# TASK092 — Reconciliation channel isolation

## Status

completed

## Objective

Ensure native queue recovery neither returns another channel's queue nor runs
automatic source cleanup for a channel that is not active.

## Acceptance criteria

- Reconciliation results are scoped to the active immutable channel ID.
- Offline recovery returns only legacy unbound drafts.
- Deferred source cleanup is processed only for the active channel.
- A two-channel regression passes.

## Evidence

- `reconcile_queue_impl` now uses the current immutable channel ID for both
  its returned queue and post-upload source-cleanup selection; when offline it
  accepts only unbound legacy drafts.
- The `reconciliation_returns_only_the_active_channel_queue` Rust regression
  inserts two queued channel records and proves recovery returns only the
  active channel record.
