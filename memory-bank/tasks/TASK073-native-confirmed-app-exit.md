# TASK073: Native confirmed app exit

## Status

completed

## Scope

Replace the webview-owned confirmed-close operation with a narrowly scoped
native application exit command. Retain normal title-bar close confirmation and
recovery-mode close behavior.

## Acceptance criteria

- Confirmed **Exit app** ends the Tauri application rather than depending on a
  webview window-destroy permission.
- Title-bar close still opens the confirmation in the normal workspace.
- Recovery-mode title-bar close remains unblocked and preserves its marker.
- The webview receives a safe error only if the native request itself fails.

## Evidence

- `Exit app` now invokes the local `exit_application` Tauri command, which
  calls `AppHandle::exit(0)` and ends the application rather than asking the
  window to close again.
- The ordinary `onCloseRequested` listener remains responsible only for the
  unconfirmed title-bar close and recovery-mode exception.
- Full native test suite (48), full frontend test suite (34), TypeScript check,
  Rust formatting/check, and whitespace diff check passed.
- Rebuilt unsigned x64 NSIS installer:
  `src-tauri/target/release/bundle/nsis/YouTube Upload Manager_0.1.9_x64-setup.exe`
  (SHA-256 `CA0C6F05F4C767FBD8A4E5AE52C6187D506A9A031E6887D66291866E801329A2`).
  The action is intentionally not invoked in automated testing because it
  terminates the native process.
