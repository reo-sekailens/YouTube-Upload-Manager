# TASK131: Fast, readable local duplicate deletion

## Status

completed

## Objective

Make accepted local-duplicate deletion avoid redundant content reads while
keeping its cryptographic and no-follow deletion boundary, and make the active
bulk-deletion state readable and safely stoppable.

## Acceptance criteria

- A deep duplicate review reuses its persisted BLAKE3 result when preparing a
  delete target; the final staged-file digest comparison remains mandatory.
- A changed, linked, or managed file remains retained.
- Bulk deletion clearly shows count, current activity, and a readable action
  to stop after the current file without deleting remaining selections.
- Focused native and frontend checks pass. Rendered UI evidence is recorded
  separately from provider verification.

## Evidence

- Deep scan preparation now reuses the reviewed persisted BLAKE3 digest. Final
  deletion still moves the source to a safe staging name and BLAKE3-verifies
  those staged bytes before permanent removal; a regression proves changed
  post-review bytes are retained.
- The bulk panel now uses a high-contrast progress card with an explicit count,
  clear current action, and an enabled **Stop after current file** control.
  It keeps the current guarded deletion atomic and retains all remaining files.
- `cargo test --lib -- --test-threads=1` passed: 142 passed, 0 failed, 5
  release-only benchmarks ignored. `npm run check`, focused Vitest (30 tests),
  and `npm run build` passed. Browser-preview testing reached the application
  but cannot activate the native-only local-source deletion flow.
