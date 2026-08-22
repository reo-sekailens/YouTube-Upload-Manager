# TASK065: Recovery-mode window close

## Status

completed

## Scope

Allow the native window close button to close the app from recovery mode. The
crash marker and all resumable local work must remain intact for the next open.

## Acceptance criteria

- Recovery mode bypasses the normal close-confirmation interception.
- Normal workspace close behavior remains unchanged.
- Closing recovery mode does not acknowledge or clear the marker.

## Evidence

- The window close listener reads the current recovery state via a ref. In
  recovery it does not prevent the OS close request; otherwise it retains the
  normal explicit exit-confirmation behavior.
- The recovery marker is not acknowledged or removed on close.
- TypeScript check, 41 native tests, and diff check passed.
- Updated unsigned Windows x64 NSIS installer built at
  `src-tauri/target/release/bundle/nsis/YouTube Upload Manager_0.1.9_x64-setup.exe`
  (26,389,531 bytes; SHA-256
  `07FCD9B4A6FA0C6AABD0E714A75854ECFEA2214686C4B78F4DFFA92EF466A446`).
