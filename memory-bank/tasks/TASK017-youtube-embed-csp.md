# TASK017 — YouTube embed content-security policy

**Status:** completed  
**Dependencies:** TASK012  

## Objective

Allow operator-requested duplicate-comparison frames to load from YouTube's standard embed host without broadening unrelated network permissions.

## Acceptance criteria

- The desktop CSP permits frames only from `https://www.youtube.com` for comparison playback.
- Existing OAuth, YouTube API connection, script, image, and device-local boundaries remain unchanged.
- The packaged Windows app contains the CSP correction.

## Evidence

- `npm run check`, `npm run test` (14 tests), and `npm run build` passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `cargo test --manifest-path src-tauri/Cargo.toml -j 1` passed: 14 Rust tests.
- `npm run tauri -- build --bundles nsis` produced `YouTube Mass Uploader_0.1.5_x64-setup.exe`; SHA-256 `54ACD25B6641222DEEA0F3A1D241A7743D61150A57B77C4BB4614DE56D2FA2D2`.
- The supplied screenshot showed the pre-fix `youtube-nocookie.com` frames blocked by the desktop CSP. Standard-origin frames now load only after Play, so an operator's direct YouTube WebView sign-in can be available where the platform permits it. Live embed playback remains subject to YouTube's video-level and cookie availability.
