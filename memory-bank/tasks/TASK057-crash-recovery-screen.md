# TASK057 — Crash recovery screen and issue handoff

**Status:** completed  
**Dependencies:** TASK035, TASK056

## Objective

Show a modern local recovery screen after a detected app crash or hard webview
error, and present the same recovery screen at the next startup when the prior
process could not render it.

## Acceptance criteria

- React rendering failures enter a full-screen recovery experience immediately.
- Native panic and webview-error markers persist only safe timestamps.
- Next launch displays recovery when a marker remains unacknowledged.
- The recovery screen can copy the redacted GitHub issue report and only clears
  the marker after the operator continues.
- Recovery identifies the safe category of the latest failure (native panic,
  webview error, unhandled promise rejection, or React render error) without
  displaying an untrusted error payload.
- No queue, source media, credentials, or provider operation is changed by
  recovery detection or display.

## Evidence

- Native panic and webview-error markers persist only RFC3339 timestamps. The
  app exposes safe load and acknowledgement commands; markers clear only after
  the operator leaves recovery.
- React render errors, global errors, and unhandled rejections enter the modern
  recovery screen immediately. A previous unacknowledged marker shows the same
  screen before the workspace on next launch.
- The screen can copy the existing redacted GitHub issue Markdown locally or
  use **Report to GitHub**, which opens a new issue with the complete report
  already filled in. No queue, credential, media, or provider action occurs in
  recovery mode.
- Native recovery status retains only the latest safe failure category and a
  timestamp. Raw exception messages are deliberately excluded because they can
  contain credentials, paths, account data, or provider responses.
- `npm run test` (31 tests), `npm run build`, and native focused validation
  (`cargo fmt --check`, `cargo test`, 37 tests) passed. Browser visual QA
  captured the recovery screen; no live provider activity was used.
