# TASK124: Folder-monitor uploaded local-source deletion

## Status

completed

## Scope

Expose a bounded Folder Monitor section for local watched files whose linked
YouTube video is live-confirmed and fully uploaded. Provide per-file and bulk
local-source deletion actions; YouTube videos and managed app copies stay intact.

## Acceptance criteria

- The Folder Monitor shows only channel-scoped watched files with a linked,
  live-confirmed YouTube video in `uploaded` state.
- The section labels the YouTube confirmation and never displays incomplete or
  reconciliation-bound records as deletable.
- Single and bulk actions require an exact local-filename confirmation and delete
  only the local watched source; neither action deletes a YouTube video.
- Native selection is channel-scoped and revalidates live ownership before
  deleting the local source.
- Tests cover the bounded native projection and local deletion gate.

## Evidence

- The native projection joins watched items to the active channel's authenticated
  `remote_videos` inventory and exposes `videoId` only with `liveConfirmed`.
- The folder-specific local deletion command rechecks watched-item ownership,
  terminal `uploaded` status, active channel, and current remote inventory. It
  requires the exact local filename and validates the stored source digest before
  deleting the file.
- `cargo test --manifest-path src-tauri/Cargo.toml folder_monitor -- --test-threads=1`
  passed: 7 tests, including the live-confirmation/deletion-gate test.
- `npm run build` and `git diff --check` passed.
- Browser preview was rendered at `http://127.0.0.1:1420/`; without a native
  channel and authenticated YouTube inventory, its populated list cannot be
  exercised in browser-preview mode.
