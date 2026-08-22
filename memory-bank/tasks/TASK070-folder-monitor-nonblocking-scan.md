# TASK070: Non-blocking watched-folder scans

## Status

completed

## Scope

Ensure manual watched-folder scans and their UI refreshes never hold the
webview in a busy state while folder, hashing, OAuth, or YouTube work is in
progress.

## Acceptance criteria

- Manual scan returns a persisted `scanning` receipt immediately and runs
  native work independently.
- UI controls are released after the receipt; queue refresh cannot retain the
  operation busy state.
- Background scan errors become a safe persisted monitor status.
- OAuth token refresh used by monitor inventory sync has connection and total
  time limits.

## Evidence

- Manual **Scan now** immediately persists and returns a `scanning` receipt,
  then runs folder, inventory, and dispatch work in a native background thread.
- The panel no longer waits for its post-scan queue refresh before releasing
  controls; an isolated refresh failure becomes a visible status error instead.
- Unexpected background failures persist a safe retryable monitor error, while
  Google refresh-token requests used during monitor inventory sync now use a
  10-second connection and 45-second total timeout.
- `cargo fmt`, 47 native tests including the non-blocking receipt regression,
  32 frontend tests, TypeScript check, and diff check passed locally.
