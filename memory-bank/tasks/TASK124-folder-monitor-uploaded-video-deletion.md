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

- The bulk confirmation now uses the visible label `Delete (n) files`, while
  the bulk action now requires one exact `DELETE n FILES` phrase before native
  revalidation and deletion of every selected local source.
- Reconciliation records are separated from the normal queue. The explicit
  reconciliation action refreshes YouTube first, completes present IDs, and
  requeues only IDs absent from that authenticated result.
- Legacy watched uploads whose durable digest is present but whose older record
  did not retain `source_path` now safely rebind that source only to the
  channel-scoped observed watched path before the existing digest/reparse-point
  cleanup gate. A batch continues after an individual retained file and returns
  a per-file outcome instead of failing silently at the first item.
- The UI renders a native per-file deletion progress bar and a collapsed local
  deletion log. It reports each deleted or retained local filename and never
  includes source paths or deletes YouTube videos.

- The native projection joins watched items to the active channel's authenticated
  `remote_videos` inventory and exposes `videoId` only with `liveConfirmed`.
- The folder-specific local deletion command rechecks watched-item ownership,
  terminal `uploaded` status, active channel, and current remote inventory. It
  requires the exact local filename and validates the stored source digest before
  deleting the file.
- `cargo test --manifest-path src-tauri/Cargo.toml folder_monitor -- --test-threads=1`
  passed: 8 tests, including the legacy-source rebind and live-confirmation
  deletion gates.
- `npm run build` and `git diff --check` passed.
- Browser preview was rendered at `http://127.0.0.1:1420/`; without a native
  channel and authenticated YouTube inventory, its populated list cannot be
  exercised in browser-preview mode.
