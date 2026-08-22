# TASK098 — Clean-profile packaged certification

## Status

blocked

## Objective

Certify the signed Windows package under a separate Windows user profile, with
no existing YouTube Upload Manager application data, credentials, queue, or
WebView profile.

## Acceptance criteria

- Launch the exact signed release installer under a dedicated clean Windows
  user or disposable VM profile.
- Verify first-open setup, safe unconfigured states, and explicit exit without
  reading or changing the operator's existing local app data.
- Record OS/profile type, release hash, result, and any native console errors.

## Blocker

- The app's Tauri app-data path is resolved by the installed app identity, so
  process-level `APPDATA` and `LOCALAPPDATA` overrides did not create a clean
  profile. A separate Windows account or disposable VM is required.
- The current environment has no attached Android device, no macOS toolchain,
  and only the Windows Rust target; it cannot supply representative supported-
  platform clean-profile evidence.
