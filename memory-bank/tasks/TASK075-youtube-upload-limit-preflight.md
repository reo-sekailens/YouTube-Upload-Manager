# TASK075: YouTube upload-limit preflight

## Status

completed

## Scope

Reject files that exceed YouTube's published 256 GB size or 12-hour duration
limits before manual import or watched-folder managed copying begins.

## Acceptance criteria

- Native manual and watched-folder intake reject over-limit videos before a
  managed copy or upload queue record exists.
- Desktop checks duration through the bundled FFprobe sidecar; mobile uses the
  available ISO-BMFF metadata path without a sidecar.
- The UI explains the limits and shows actionable native error text.

## Evidence

- Manual intake validates size and duration before inserting a copy checkpoint.
- Watched-folder intake validates after the two-scan stability gate and records
  an over-limit file as rejected rather than retrying it every poll.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 50 tests.
- `npm test` passed: 35 tests; `npm run build` passed.
