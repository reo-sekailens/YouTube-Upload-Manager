# TASK047: Light-match local-file deletion

## Status

completed

## Scope

Offer the existing confirmed local source-file deletion flow for a pre-ingest local match found by either filename-first light matching or SHA-256 deep matching.

## Acceptance criteria

- A light filename match against a saved local upload exposes a delete action in its comparison card when the selected desktop source is eligible.
- The native layer revalidates the persisted scan match, canonical external source, managed-workspace exclusion, and a fresh SHA-256 before issuing the same short-lived confirmation token.
- The existing typed-filename deletion and re-hash-at-delete safeguards remain mandatory.
- Tests cover the light-match preparation path and the webview command boundary.

## Evidence

- `cargo test --lib` (30 tests), `npm test` (31 tests), `npm run check`, `npm run build`, and `git diff --check` passed.
- Browser preview confirms the duplicate-review entry controls and no console warnings/errors. A signed desktop fixture with a real local filename match is required to render the native-only deletion action end to end.
