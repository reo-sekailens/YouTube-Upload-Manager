# TASK136: Playlist library management

## Status

completed

## Objective

Let the operator create/select a private playlist, sort the saved active-channel
library by video title, and explicitly add selected uploaded videos.

## Acceptance criteria

- Video title order supports ascending and descending order.
- The user can create a private playlist or select an existing one.
- Adding videos is channel-scoped and requires temporary management mode.

## Evidence

- The new lazy Playlists workspace loads the active channel's saved video
  inventory and playlists together, offers A-Z/Z-A title ordering, playlist
  creation, explicit selection, and adding the selected set.
- Native addition rechecks that every selected video belongs to the active
  channel and requires the existing temporary management mode before the
  YouTube playlist write.
- `npm run check`, `npm run build`, `cargo test --lib --no-run`, and
  `git diff --check` passed. Browser visual automation was unavailable because
  its local connection timed out before acquiring a page.
