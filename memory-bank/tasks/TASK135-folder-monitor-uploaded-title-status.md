# TASK135: Folder-monitor uploaded-title status

## Status

completed

## Objective

Make folder-monitor duplicate status accurately state that light dedupe checks
the active channel's uploaded YouTube inventory only.

## Acceptance criteria

- A watched file skipped for a light title match is described as matching an
  already-uploaded active-channel YouTube video.
- The audit wording no longer claims local queue matching.

## Evidence

- The native audit now identifies the match as an already-uploaded title in
  the active YouTube channel, matching the uploaded-only light-dedupe boundary.
- `cargo test --lib folder_monitor -- --test-threads=1` passed (8 tests),
  including the watched-file title-duplicate regression.
