# TASK055: Local deletion reuses the opt-in duplicate review

## Status

completed

## Scope

Remove repeated SHA-256 work when an operator permanently deletes a reviewed local duplicate. Hashing for deep pre-ingest duplicate detection remains opt-in.

## Acceptance criteria

- Creating a local deletion token does not hash the source again.
- Confirming an individual or bulk local deletion does not hash the source again.
- The accepted persisted review, exact typed filename, short-lived token, external-path/managed-workspace restriction, and audit receipt remain.

## Evidence

- The deletion target records only the canonical external source path, filename, and short-lived creation time; it does not retain or calculate a digest.
- Token preparation reloads the persisted local or remote-title duplicate evidence without calling the hashing helper, and permanent deletion removes the guarded path after exact filename confirmation without hashing.
- The bulk and single confirmation copy explicitly state that the accepted opt-in review is reused and no re-hash runs.
- `cargo fmt -- --check` and `cargo test` passed with 33 Rust tests, including a regression that changes the source after token creation and confirms the deletion path does not re-hash it. `npx tsc --noEmit` and `git diff --check` passed.
