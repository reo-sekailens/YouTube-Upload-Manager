# TASK120: Queue-clear retention boundary

## Status

completed

## Objective

Make clearing the visible queue distinct from source deletion and preserve all
local evidence needed for dedupe, recovery, and audit.

## Requirements

- Clear only unfinished items in the dashboard's active-channel scope (or only
  unbound items before a channel is connected).
- Do not delete upload rows, BLAKE3 digests, source/workspace references,
  resumable evidence, remote inventory, or audit records.
- Retain the separate typed-filename original-file deletion flow.
- Explain the retention boundary in the clear control's accessible hover text
  and completion notice.

## Completion evidence (2026-08-23)

- `clear_upload_queue` now delegates to a scoped implementation that changes
  only status/detail/timestamp for unfinished visible-scope items, then writes
  a scoped local audit event. It executes no `DELETE`, filesystem, or YouTube
  operation.
- The Batch icon tooltip and completion notice explicitly state that local
  records, BLAKE3 hashes, and original files are retained.
- The clear-queue toolbar visibly groups those three guarantees as icon-plus-
  text chips: Records kept, BLAKE3 kept, and Originals kept.
- The toolbar is lazy-loaded with the Batch workspace; `npm run
  performance:frontend:check` remains within the fixed initial budget at
  223.60 KiB raw / 69.97 KiB gzip JavaScript and 35.24 KiB CSS.
- Native regression coverage verifies retention, channel isolation, dashboard
  hiding, and audit recording.
- Passed `cargo fmt --check`, the focused native retention test, `npm run
  check`, the 29 focused bridge/workspace tests, and `git diff --check`.
