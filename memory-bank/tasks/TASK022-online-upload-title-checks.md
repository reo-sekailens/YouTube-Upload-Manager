# TASK022 — Online title checks before upload

## Status

in-progress — implementation complete; native test archive verification is blocked by a full system drive.

## Outcome

Before a manual upload, batch dispatch, or watched-folder automatic upload proceeds, the native layer refreshes the active channel inventory and compares normalized exact titles plus trailing `(2)` or higher copy variants. A detected match is retained locally and requires an operator to upload anyway or skip it. The same decision can apply to all affected items.

## Evidence

- `npm run build` passed.
- `npm run test -- --run` passed: 17 tests.
- `cargo fmt -- --check` passed.
- `cargo test` reached native compilation but could not create its archive because drive C: had 0 free bytes (`os error 112`).
