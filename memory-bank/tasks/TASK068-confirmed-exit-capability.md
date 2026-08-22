# TASK068: Confirmed app exit capability

## Status

completed

## Scope

Repair the explicit exit-confirmation action for the main Tauri window without
weakening the close confirmation shown for an unconfirmed title-bar close.

## Acceptance criteria

- Selecting **Exit app** after confirmation destroys the main window.
- The main webview alone has the narrowly required native window permission.
- A normal close request still opens the confirmation instead of exiting.

## Evidence

- The existing confirmation calls Tauri's explicit `Window.destroy()` path so
  it cannot be intercepted again by the normal close-request handler.
- The main-window capability now grants only `core:window:allow-destroy`, which
  was absent from `core:default` and caused the reported rejection.
- `npm run check`, `cargo check --manifest-path src-tauri/Cargo.toml`, JSON
  parsing, and `git diff --check` passed locally.
