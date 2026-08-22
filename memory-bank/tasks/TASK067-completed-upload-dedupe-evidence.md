# TASK067: Completed-upload dedupe evidence

## Status

completed

## Scope

Keep incomplete local queue entries and incomplete YouTube processing records out
of every duplicate-candidate path. Persist YouTube upload processing state with
the synchronized inventory so interrupted or still-processing uploads can be
distinguished from completed evidence after restart.

## Acceptance criteria

- YouTube inventory synchronization persists each video's `status.uploadStatus`.
- Only `processed` YouTube videos can create title-based duplicate candidates.
- Only locally confirmed `uploaded` records can create local hash candidates.
- Pre-ingest, queued-upload, and folder-monitor checks share the completed-only
  rule and remain crash-safe.
- Existing portable metadata carries the processing state without accepting an
  unknown state as completed evidence.

## Evidence

- Synchronized YouTube inventory now persists `status.uploadStatus` atomically
  alongside the current complete inventory snapshot.
- Remote title and remote-to-remote candidate queries accept only
  `upload_status = 'processed'`; unknown legacy/imported state is deliberately
  not treated as completed evidence until a refresh records it.
- Local hash and filename pre-ingest candidates, as well as guarded local
  duplicate deletion eligibility, accept only `upload_items.status =
  'uploaded'`.
- Compact portable metadata preserves this processing state while remaining
  compatible with older archives; absent state stays ineligible by design.
- `cargo fmt`, 42 native tests (including incomplete-upload regression), and
  `npm run check` passed locally.
