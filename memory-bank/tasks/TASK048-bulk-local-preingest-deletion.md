# TASK048: Bulk local pre-ingest deletion

## Status

completed

## Scope

Add the same selectable bulk-review controls used for remote duplicate deletion to eligible local pre-ingest duplicate cards.

## Acceptance criteria

- Eligible local-match cards can be selected individually or all at once.
- Bulk deletion has an explicit permanent-action confirmation phrase, item list, progress, and per-file guarded native delete preparation.
- Each file still gets its own native match validation, external-path exclusion, fresh hash, typed batch confirmation, and re-hash immediately before removal.
- Successfully deleted review cards are removed; an error stops the remaining batch and reports the result.

## Evidence

- Eligible review cards now have individual selection checkboxes and a Select all local duplicates control.
- The bulk permanent-deletion review lists every selected filename, requires a `DELETE N LOCAL FILE(S)` phrase, and presents progress while it works.
- The UI processes the selected sources sequentially. For every source, native code revalidates the persisted match, protects managed media paths, hashes before issuing a fresh opaque token, and hashes again immediately before removal. A failure stops the remaining files and leaves them selected for explicit retry or deselection.
- Stable persisted ordinals ensure a completed deletion removes only its matching review card even when a batch changes the result list during processing.
- Local verification: Prettier, `npm test` (31 tests), `npm run check`, `npm run build`, `cargo fmt`, `cargo test --lib` (30 tests), `cargo check`, and `git diff --check` all passed. Browser preview rendered the Duplicate review entry state without console warnings/errors; populated native local-match cards require a signed-app fixture and remain unverified.
