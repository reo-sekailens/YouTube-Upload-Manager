# TASK082 — Fast-start watched uploads with resumable deep verification

## Status

completed

## Objective

Start an eligible watched-folder upload as soon as the persisted light title-dedupe gate passes, while verifying its BLAKE3 content digest on a separate native worker.

## Acceptance criteria

- A stable watched source that has no light duplicate starts its resumable upload without waiting for the full content hash.
- Deep verification records `pending`, `running`, `complete`, `duplicate`, or `failed` state on the local upload record and resumes pending/running work after relaunch.
- An exact digest match against a completed upload cancels an unfinished provider upload at its next checkpoint.
- A changed, missing, or unreadable watched source cancels an unfinished provider upload without deleting anything.
- A duplicate discovered after YouTube completion is retained for explicit deletion review; automatic remote deletion remains forbidden.

## Implementation

- Added persisted background-hash status and watched-source modification key fields to `upload_items`.
- Replaced the watched-folder pre-queue full hash with a light-gated upload record plus a native BLAKE3 verification worker.
- Restart recovery changes interrupted verification to `pending` and restarts it from its durable item/source record.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1` — 51 passed.
- `npm test` — 35 passed.
- `npm run build` — passed; Vite retained its pre-existing mixed static/dynamic `@tauri-apps/plugin-opener` chunk warning.
- `git diff --check` — passed.

## Follow-up

- BLAKE3's standard streaming state is not portable for safe byte-level serialization. The worker resumes durable per-file verification after a restart, but an interrupted individual file is safely re-read from its stable source rather than serializing private crate internals.
