# TASK066: Local duplicate deletion worker isolation

## Status

completed

## Scope

Move guarded local duplicate deletion—especially bulk deletion file removal and
verification—off the UI command path into the native blocking worker pool.

## Acceptance criteria

- The webview thread only schedules and renders deletion work.
- Every local-delete filesystem check/removal runs in a native worker.
- Existing token, filename confirmation, managed-media exclusion, and audit
  boundaries remain unchanged.

## Evidence

- `delete_preflight_duplicate_file` is now an async native command that moves
  the guarded deletion implementation into Tauri's blocking worker pool.
- The bulk UI remains responsible only for selection and progress rendering.
- Rust format, 41 native tests, TypeScript check, and diff check passed.
