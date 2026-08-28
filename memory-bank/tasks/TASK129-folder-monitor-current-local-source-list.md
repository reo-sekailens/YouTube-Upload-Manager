# TASK129: Current Folder Monitor local-source list

## Status

completed

## Scope

Keep the Folder Monitor's live-confirmed local-source deletion list current.
An uploaded YouTube record remains eligible only while its watched local file
is present; an absent source cannot be shown or selected for another deletion.

## Acceptance criteria

- The native overview reports local source availability without exposing paths.
- The UI excludes absent and already-deleted watched sources from all deletion
  actions while retaining the YouTube-confirmation gate.
- A repeated cleanup action cannot overwrite an absent source's completed
  deletion receipt with a linked-path retention result.

## Evidence

- The native overview now reports a path-free local-presence signal; the
  deletion display excludes absent and already-deleted local files even when
  their YouTube upload remains live-confirmed.
- Cleanup checks absence before the link/reparse guard, preserving the final
  local deletion receipt after an interrupted or repeated action.
- `cargo test --manifest-path src-tauri/Cargo.toml
  confirmed_source_cleanup_keeps_an_absent_source_marked_deleted_before_link_checks
  -- --test-threads=1` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml
  folder_monitor_overview_is_channel_scoped_bounded_and_hides_source_paths --
  --test-threads=1` passed.
- `npm run build` and `git diff --check` passed. The broader folder-monitor
  filter has a pre-existing scheduler race in its cancelled-file requeue test
  when the running app claims the requeued item (`running` versus `pending`).
