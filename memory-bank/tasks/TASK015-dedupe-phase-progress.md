# TASK015 — Dedupe phase progress

**Status:** completed  
**Dependencies:** TASK013  

## Objective

Show an accessible determinate progress bar for the three actual manual dedupe phases without presenting synthetic per-video or time estimates.

## Acceptance criteria

- The activity section shows a three-step progress bar while dedupe runs and after it completes.
- Step descriptions map only to native command boundaries: inventory synchronization, local candidate rebuild, and review readiness.
- Failure visibly communicates that the run stopped and does not claim completion.
- The progress control exposes correct accessible range and value metadata.

## Evidence

- `npm run check`, `npm run test` (13 tests), and `npm run build` passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `cargo test --manifest-path src-tauri/Cargo.toml -j 1` passed: 14 Rust tests.
- `npm run tauri -- build --bundles nsis` produced `YouTube Mass Uploader_0.1.4_x64-setup.exe`; SHA-256 `BFE150376466BBCF068C2FC0547006FD908B56E4825E7A6953DA4FBF03E8A3AE`.
