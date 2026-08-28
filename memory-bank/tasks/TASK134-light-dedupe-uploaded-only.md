# TASK134: Light dedupe uploaded-only boundary

## Status

completed

## Objective

Make light duplicate detection use only the active channel's uploaded YouTube
inventory. Local queued, failed, draft, and cancelled items must never create a
light duplicate match.

## Acceptance criteria

- A local upload queue item with a matching title produces no light match.
- An active-channel uploaded YouTube video with a matching title still does.
- The light-match scope is always `youtube` when a match is returned.

## Evidence

- The local-queue regression now creates matching current-batch and
  same-channel draft records and proves light dedupe returns no match.
- The existing channel-scoped uploaded-title test continues to cover a
  matching `remote_videos` record.
- `cargo test --lib light_dedupe_ -- --test-threads=1` passed.
