# TASK046: Confirmed-upload source cleanup

## Status

completed

## Scope

Add an explicit per-batch and per-upload option to remove an original local source file only after YouTube returns a confirmed upload receipt. Apply the same opt-in choice to watched-folder uploads.

## Acceptance criteria

- Manual batch intake applies the opt-in choice to every imported item, and each draft item can change its own choice before queueing.
- Watched-folder configuration visibly persists its choice and applies it to newly discovered files.
- Cleanup is persisted as pending after YouTube returns a video ID, resumes after interruption, re-hashes the external source immediately before deletion, and never removes managed workspace media.
- A missing or changed source is retained with an audit event; a transient filesystem removal failure remains pending for retry.
- Native and web checks cover the new behavior.

## Evidence

- `npm run check`, `npm test` (30 tests), `npm run build`, `cargo fmt --check`, `cargo test --lib` (29 tests), `cargo check`, and `git diff --check` passed.
- Browser preview confirmed the watched-folder cleanup control and safety copy are visible. Native provider confirmation remains unverified without an authorized YouTube canary.
