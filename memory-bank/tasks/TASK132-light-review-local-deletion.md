# TASK132: Light-review local deletion

## Status

completed

## Objective

Make it explicit that a locally matched file can be permanently deleted from a
light filename/title review without starting a deep duplicate-review scan.

## Acceptance criteria

- Light-match local deletion remains available without a deep scan.
- Confirmation text distinguishes light and deep reviews accurately.
- Light review does not read the full file during deletion. Deep review retains
  its final cryptographic staged-file comparison.
- No-follow, staged-file, managed-workspace, and typed-confirmation safeguards
  remain required for both review modes.

## Evidence

- A light-review token intentionally contains no BLAKE3 digest, so selected
  files are not read end-to-end before deletion. A deep-review token retains
  the persisted digest and still rejects changed staged bytes.
- The single and bulk confirmations distinguish the faster light path from the
  deep cryptographic-content verification path.
- Focused native light/deep safety regressions and `npm run check` passed. The
  packaged local installer is recorded in TASK121.
- `npm run check`, focused Vitest (30 tests), and `cargo test --lib --
  --test-threads=1` passed (142 passed, 0 failed, 5 ignored). The current
  unsigned Windows installer is recorded in TASK121.
