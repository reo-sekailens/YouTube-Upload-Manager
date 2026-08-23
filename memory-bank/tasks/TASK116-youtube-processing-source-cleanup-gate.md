# TASK116: YouTube processing source-cleanup gate

## Status

completed

## Scope

Prevent automatic or operator-confirmed original-source deletion until the
owner-visible YouTube video record reports that upload processing completed.
Keep the source when YouTube reports failed, terminated, rejected, or missing
processing state, including the Studio-facing “Processing abandoned” case.

## Acceptance criteria

- A persisted upload receipt alone cannot delete the source file.
- The existing channel-inventory `videos.list` read includes
  `processingDetails` and does not create one extra provider request per file.
- Only `uploadStatus = processed` plus `processingStatus = succeeded` permits
  source cleanup.
- Failed/terminated processing, failed/rejected uploads, unavailable state,
  and omitted inventory records retain the original source.
- The queue names the safety outcome rather than displaying opaque status text.

## Evidence

- `source_cleanup_processing` maps owner-visible `status` and
  `processingDetails` values to an explicit fail-closed cleanup decision.
- Inventory reconciliation persists a retained state and audit event for a
  processing failure/abandonment; only verified success transitions to the
  existing race-safe SHA-256 cleanup path.
- `abandoned_youtube_processing_keeps_the_original_source` proves a retained
  source is not deleted when YouTube reports failed processing.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 131 passed, 0 failed, 5 ignored.
