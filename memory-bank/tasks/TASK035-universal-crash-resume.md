# TASK035 — Universal crash-safe operation recovery

## Status

completed

## Objective

Every state-changing local or YouTube operation must record a durable checkpoint before its irreversible boundary and recover from the checkpoint after an application interruption. Recovery must resume a known operation or place it in an explicit reconciliation state; it must never silently repeat from the beginning or claim an unconfirmed remote result.

## Acceptance criteria

- Interrupted managed imports, queued dispatch, resumable uploads, watched-folder intake, light/deep pre-ingest work, and inventory sync resume from durable state.
- Remote deletion records an execution checkpoint before contacting YouTube and is reconciled rather than blindly retried after interruption.
- Portable archive export publishes atomically; import stays transactional.
- Startup recovery runs before normal dispatch and keeps remote ambiguity explicit.

## Evidence

- Upload recovery returns only uploads with a protected resumable-session checkpoint to the queued worker; the worker re-queries the provider for the confirmed range before sending bytes. Items without that checkpoint stay in explicit reconciliation.
- Interrupted remote deletions retain an `executing` checkpoint and become `needs_reconciliation` at restart. The operator must confirm the exact video ID again; no blind retry or false success is recorded.
- Archive export writes and syncs a temporary file, then publishes it only if the requested destination does not already exist. Import remains a SQLite transaction.
- Inventory sync is staged then atomically swapped; pre-ingest jobs checkpoint each file; watched-folder observations and managed import/upload queue states already persist before their side effects.
- Local verification: `cargo check`, focused Rust recovery tests, `npm run check`, `npm test`, `npm run build`, and `git diff --check`.
