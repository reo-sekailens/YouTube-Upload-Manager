# TASK049: Local deletion for remote-title pre-ingest matches

## Status

completed

## Scope

Allow a reviewed desktop source whose filename matches the active channel's persisted YouTube title inventory to use the same guarded local-file deletion and bulk-selection flow as a saved local-copy match.

## Acceptance criteria

- Uploaded-title match cards show the local deletion selection and action.
- The native command rechecks either a saved local match or the active channel's normalized uploaded-title match before issuing a deletion token.
- The external-path protection, typed filename confirmation, short-lived token, and pre-delete re-hash remain unchanged.

## Evidence

- An eligible desktop source is now deletable when it has either a saved local match or an active-channel uploaded-title match. The existing selection, Select all, bulk typed confirmation, and sequential progress controls apply to both evidence types.
- Before issuing a token, native code reloads the persisted source and rechecks the local match or the active channel's normalized uploaded-title match. It then keeps the existing external-path-only restriction, managed-workspace exclusion, fresh digest, short-lived token, exact filename confirmation, and immediate pre-delete re-hash.
- Added a Rust regression test for the filename normalization shown in the report (`VID_20251218_195343_00_005.mp4` vs `VID 20251218 195343 00 005`).
- Verification passed: Rust format, focused regression test, `cargo test --lib` (31 tests), `npm test` (31 tests), `npm run check`, `npm run build`, `cargo check`, and `git diff --check`. Browser preview rendered the Duplicate review entry state with no console warnings/errors; native populated review cards require a signed-app fixture for visual end-to-end confirmation.
