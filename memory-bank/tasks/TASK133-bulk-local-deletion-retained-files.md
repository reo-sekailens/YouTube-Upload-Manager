# TASK133: Bulk local deletion retained-file handling

## Status

completed

## Objective

Continue a confirmed bulk local deletion when an individual source has become
unavailable or fails its native guard, and show the precise retained-file
reason rather than a generic zero-deletions message.

## Acceptance criteria

- One unavailable or guarded file does not stop deletion of independent later
  selections.
- Each retained filename and safe native error appears in the bulk result.
- No automatic retry, path relaxation, or deletion of a guarded file occurs.

## Evidence

- The supplied nightly.8 diagnostic report records 11 successful
  `preflight_local_duplicate_deleted` events before the next bulk run. This
  confirmed that the later failure was an individual-source guard failure, not
  a general inability to delete local duplicates.
- `PreIngestDuplicatePanel.tsx` now catches preparation/deletion errors per
  selected file, continues with independent later files, and reports each
  retained filename with the native safe-error message.
- `npm run check` and focused Vitest checks for the local duplicate UI/helpers
  passed. `npm run tauri -- build --bundles nsis` produced nightly.9.
