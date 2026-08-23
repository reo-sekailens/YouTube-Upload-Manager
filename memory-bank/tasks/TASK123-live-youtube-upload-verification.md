# TASK123: Live YouTube upload verification

## Status

completed

## Scope

Do not finalize a watched-folder upload merely because the resumable upload
endpoint returned a video ID. Confirm the ID through YouTube's authenticated
video resource before recording `uploaded`; otherwise retain durable evidence
and enter reconciliation without deleting the source.

## Acceptance criteria

- A successful upload response is followed by an authenticated YouTube video
  lookup scoped to the bound channel before the item becomes `uploaded`.
- A missing, inaccessible, or mismatched result is durable reconciliation, not
  a completed receipt or source-cleanup candidate.
- Transient verification failures are retried safely and never cause a second
  upload.
- Tests cover verification success, missing-video reconciliation, and source
  retention.
- Existing falsely completed watched-folder records can be identified and
  safely repaired through the same verification boundary.

## Evidence

- Every terminal resumable-upload response now performs an authenticated
  `videos.list` lookup and requires the exact video ID, immutable channel ID,
  and requested visibility before the durable state becomes `uploaded`.
- A missing or mismatched lookup persists the provider ID only as
  `needs_reconciliation`, blocks playlist/source-cleanup work, retains the
  source, and removes the terminal resumable-session checkpoint so it cannot
  initiate another upload.
- A complete authenticated library read repairs older completed records whose
  video IDs are absent from YouTube, moving them to reconciliation without
  automatic retry.
- Direct YouTube checks repaired the four previously false-complete watched
  uploads on this device into `needs_reconciliation`; their original sources
  remain at `G:\360mp4\BO`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 137 passed, 0 failed, 5 ignored. `cargo fmt --check` and the
  release no-bundle build passed.
