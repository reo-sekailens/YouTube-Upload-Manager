# TASK077: Windows installer for upload-limit and light-dedupe release

## Status

in-progress

## Scope

Build and hash-verify the Windows x64 NSIS installer containing the current
upload-limit preflight and universal light-dedupe changes.

## Acceptance criteria

- Tauri release build completes.
- NSIS installer exists under the release bundle directory.
- SHA-256 is recorded for handoff.
