# TASK086 — Immutable manual-upload channel binding

## Status

completed

## Objective

Bind every manual upload to the immutable channel selected during review and
block dispatch, retry, or resume when another channel is active.

## Acceptance criteria

- Queueing requires and persists the active channel ID.
- Dispatch and recovery enforce the same channel ID and leave mismatches safe.
- A regression test covers account switch between queueing and dispatch.

## Evidence

- Manual queueing now requires a connected channel and saves its display name
  plus immutable channel ID on the upload item.
- Upload dispatch pauses every item whose saved channel ID is absent or differs
  from the current connection.
- Focused Rust regressions cover active-channel persistence at queue time and a
  channel switch between queueing and dispatch.
