# TASK088 — Channel-scoped inventory and deletion records

## Status

completed

## Objective

Scope persisted remote inventory, deletion requests, views, dedupe, and archive
data to immutable channel identity.

## Acceptance criteria

- Account changes cannot reveal or act on another channel's local records.
- Archive/import preserves channel scope.
- Cross-channel regression coverage passes.

## Evidence

- Remote inventory staging, saved rows, portable archives, deletion requests,
  saved-library views, and deletion-request views now carry or filter on the
  immutable channel ID.
- Portable archive import now rejects inventory records without a channel ID.
- Inventory replacement retains other channel snapshots while atomically
  replacing only the active channel's completed staging set. Remote-title and
  exact-local duplicate review use immutable channel IDs, and dashboard queue
  rows are scoped to the active connection (or unbound drafts while offline).
- Focused Rust regressions cover snapshot replacement and duplicate-review
  isolation across two channel IDs.
- `TASK092` additionally scopes native startup/command reconciliation results
  and automatic source cleanup to the active immutable channel ID.
