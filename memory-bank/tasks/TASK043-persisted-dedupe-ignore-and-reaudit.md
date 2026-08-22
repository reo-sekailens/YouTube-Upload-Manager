# TASK043 — Persisted duplicate-review exclusions

## Status

Completed

## Objective

Let an operator hide a reviewed false-positive duplicate candidate without
deleting media, retain that choice across restarts, and restore all ignored
candidates for an explicit re-audit.

## Acceptance criteria

- Every duplicate candidate has an Ignore match action.
- Ignore decisions are stored locally and excluded from later snapshots.
- Re-audit ignored matches restores every ignored candidate and runs normal
  dedupe review again.
- Decisions and re-audits are append-only audited.
- Native and frontend boundary tests cover the behavior.

## Evidence

- `ignored_duplicate_candidates` persists device-local candidate IDs and their
  review timestamps. Dashboard snapshots exclude these candidates without
  changing media, YouTube inventory, or deletion state.
- Each candidate card now offers **Ignore match**. **Re-audit ignored matches**
  restores all ignored entries, refreshes local candidates without a
  connection, and performs the normal inventory refresh when a channel is
  connected.
- Both operations are append-only audited as operator decisions.
- Passed: `cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `cargo
  test --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast` (27 passed),
  `cargo check --manifest-path src-tauri/Cargo.toml`, `npm run check`, `npm
  test` (29 passed), `npm run build`, and `git diff --check`.
