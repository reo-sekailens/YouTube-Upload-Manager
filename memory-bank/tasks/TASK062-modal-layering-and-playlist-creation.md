# TASK062 — Modal layering and playlist creation

**Status:** completed  
**Dependencies:** TASK014, TASK019

## Objective

Keep modal content fully undimmed while its page backdrop is dimmed, and let
operators create and select a private YouTube playlist while configuring
single-file, batch, or watched-folder uploads.

## Acceptance criteria

- Every modal backdrop is a sibling layer below its dialog, never a pseudo
  element that can dim dialog content.
- Upload review and folder-monitor configuration can create a private playlist,
  select it immediately, and preserve that selection for queued uploads.
- Playlist creation stays in the native command layer, is scoped to the active
  YouTube connection, and requires explicit operator action.

## Evidence

- `npm run test` (32 tests), `npm run build`, and focused native playlist tests
  (2 tests) passed. Browser visual QA verified the modal content is the topmost
  layer above its sibling backdrop and captured the new playlist control.
- The standard connection now requests `youtube.force-ssl`, which is needed for
  playlist creation. Existing connections must reconnect before creating their
  first playlist. No live playlist was created during verification.
