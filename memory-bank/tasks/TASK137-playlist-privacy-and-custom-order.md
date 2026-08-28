# TASK137: Playlist privacy and custom title order

## Status

completed

## Objective

Create public, unlisted, or private playlists and apply an explicit A-Z or Z-A
playlist-item order when the selected YouTube playlist is configured for Manual
ordering in YouTube Studio.

The workspace separates creation and video additions from existing-playlist
management and custom ordering, so the two workflows cannot be confused.

## Evidence

- Native playlist creation validates and sends the selected privacy status.
- Native ordering loads the selected playlist's item identities, sorts titles,
  updates zero-based positions through `playlistItems.update`, and reloads the
  playlist to prove that YouTube retained the requested order.
- Native progress events supply an accessible item-count progress bar and an
  in-page activity log; each provider failure is shown at its failing item.
- A YouTube 400 response gives the operator the Manual-ordering prerequisite.
- The Playlists workspace has distinct Create & add videos and Manage & sort
  playlist tabs, each with only its relevant controls.
- `npm run check`, `cargo fmt --check`, and `cargo test --lib --no-run` passed.
