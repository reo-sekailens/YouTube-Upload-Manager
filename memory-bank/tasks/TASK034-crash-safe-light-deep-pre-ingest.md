# TASK034 — Crash-safe light and deep pre-ingest matching

## Status

completed

## Acceptance criteria

- The default pre-ingest check compares filenames without reading selected media.
- An explicit deep option streams SHA-256 locally and remains available for arbitrary file types.
- Each selected-file result is checkpointed device-locally so a relaunch resumes only unfinished work.
- A YouTube inventory refresh cannot erase the prior complete local inventory if interrupted.
- The UI distinguishes fast filename evidence from exact hash evidence.

## Implementation and evidence

- `preflight_scan_jobs` and `preflight_scan_files` store the device-local job, each selected native file locator, status, byte count, digest, and error. Paths remain native-only and are never returned to the webview.
- Light jobs checkpoint filename results without opening media. Deep jobs stream one file at a time and persist each completed result before moving on; queued/running jobs resume on app startup.
- Inventory pages are staged in SQLite and atomically replace `remote_videos` only after the complete remote refresh succeeds.
- Duplicate review exposes Light match files and Deep hash files, labels the current evidence, and shows checkpointed job progress.
- Local verification: `cargo check`, `npm run check`, `npm test`, `npm run build`, and `git diff --check`.

## Follow-up

- Physical Android and iOS interruption/resume checks remain required because platform-scoped picker access may expire after a process termination.
