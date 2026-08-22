# TASK019 — Watched-folder visibility and queue layout

**Status:** completed  
**Dependencies:** TASK008, TASK014  

## Objective

Let the operator select private or unlisted visibility for a watched-folder authorization and improve the queue intake layout's readability and spacing.

## Acceptance criteria

- Watched-folder setup permits only private or unlisted uploads; public remains unavailable.
- The saved folder setting controls all new watched-folder items and remains visible when enabled.
- Existing monitors migrate safely to private visibility.
- Queue drop-zone title and supporting copy render on separate, comfortably spaced lines.

## Evidence

- `npm run check`, `npm run test` (16 tests), and `npm run build` passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `cargo test --manifest-path src-tauri/Cargo.toml -j 1` passed: 14 Rust tests.
- `npm run tauri -- build --bundles nsis` produced `YouTube Uploads Manager_0.1.7_x64-setup.exe`; SHA-256 `E11D11F59E9DCC56D6F1F2DBE32BDFAE28ABEABE1B644FB8F4C654D201142034`.
