# TASK077: Windows installer for upload-limit and light-dedupe release

## Status

completed

## Scope

Build and hash-verify the Windows x64 NSIS installer containing the current
upload-limit preflight and universal light-dedupe changes.

## Acceptance criteria

- Tauri release build completes.
- NSIS installer exists under the release bundle directory.
- SHA-256 is recorded for handoff.

## Evidence

- Unsigned x64 NSIS installer built successfully on 2026-08-23 after the
  current queue-clearing and BLAKE3 changes:
  `YouTube Upload Manager_0.1.9_x64-setup.exe`.
- Size: 26,442,441 bytes. SHA-256:
  `D60D7093BFA4DD5A3612F5AE5E293C40C132AF0A7D6D5F8A9AB64C287A04F73D`.
