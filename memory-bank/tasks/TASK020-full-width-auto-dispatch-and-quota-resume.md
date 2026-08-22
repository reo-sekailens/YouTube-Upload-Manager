# TASK020 — Full-width workspace, automatic dispatch, and quota recovery

**Status:** completed  
**Dependencies:** TASK004, TASK008, TASK014

## Objective

Make every dashboard card occupy the available workspace width, automatically queue and begin an operator-reviewed manual import batch when a connected channel is available, and persist a safe 24-hour pause when YouTube reports its daily upload limit.

## Acceptance criteria

- The connection and duplicate-review cards render in a one-column, full-width workspace.
- A completed manual import batch is queued and its saved queue begins immediately when a channel is connected; importing without a channel keeps the managed copies local and explains the connection requirement.
- Watched-folder files continue from stability verification directly into the existing automatic uploader.
- A recognized YouTube daily/quota upload-limit response keeps the current item queued, records a device-local 24-hour pause, prevents further dispatch, and resumes saved queue work once the pause has elapsed while the app is running or when it next starts.
- No token, response body, local path, or provider payload is surfaced in operator text or audit details.

## Evidence

- `npm run check`, `npm run test` (16 tests), and `npm run build` passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `cargo test --manifest-path src-tauri/Cargo.toml -j 1` passed: 15 Rust tests, including persisted quota-pause expiry coverage.
- Browser preview confirmed the one-column, full-width dashboard with no console errors beyond the React DevTools information message; [full-width dashboard screenshot](../../output/playwright/full-width-dashboard.png).
- `npm run tauri -- build --bundles nsis` produced `YouTube Uploads Manager_0.1.8_x64-setup.exe`; SHA-256 `58E49390B165240882EE9D1B7D375F9E82578CD8022FDA25A61B48962792517C`.
