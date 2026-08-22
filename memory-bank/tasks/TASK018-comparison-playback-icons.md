# TASK018 — Comparison playback icons

**Status:** completed  
**Dependencies:** TASK012, TASK017  

## Objective

Replace comparison playback text actions with accessible icon controls for synchronized play/pause, back 10 seconds, and forward 10 seconds.

## Acceptance criteria

- Each shared comparison action has a recognizable icon, tooltip, and accessible label.
- Both players receive each playback or seek command.
- Ten-second seeks clamp to the supported range.
- Desktop and mobile controls remain usable.

## Evidence

- `npm run check`, `npm run test` (16 tests), and `npm run build` passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `cargo test --manifest-path src-tauri/Cargo.toml -j 1` passed: 14 Rust tests.
- `npm run tauri -- build --bundles nsis` produced `YouTube Uploads Manager_0.1.6_x64-setup.exe`; SHA-256 `CB87BB3181BB5CC0D013C0C7C103815EE7D318DF63B184B891451A714FBA63E9`.
