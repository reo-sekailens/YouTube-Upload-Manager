# TASK083 — Folder-monitor fast handoff

## Status

completed

## Objective

Prevent a watched file from remaining in `processing` while desktop FFprobe performs an unbounded metadata read before it can be queued.

## Implementation

- Bounded desktop FFprobe metadata collection to 15 seconds, killing a stalled child process and safely continuing without optional duration metadata.
- Prefer the fast ISO-BMFF duration read for MP4/MOV/M4V/INSV/LRV upload-limit checks; FFprobe is now a timed fallback only when that metadata is unavailable.
- Size limits remain enforced before queueing. The native monitor worker continues to own all blocking work.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1` — 51 passed.
- `npm test` — 35 passed.
- `npm run build` — passed; Vite retained its pre-existing mixed static/dynamic `@tauri-apps/plugin-opener` chunk warning.
- `git diff --check` — passed.
