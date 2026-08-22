# TASK037 — Native worker isolation

## Status

completed

## Acceptance criteria

- The webview never waits on filesystem streaming, hashing, SQLite rebuilds, compression, or YouTube HTTP calls.
- Tauri command handlers only schedule background work and return the native result when complete.
- Existing persisted recovery behavior is retained.

## Evidence

- Folder scans, archive import/export, YouTube inventory/title checks, playlist loading, deletion execution, local media import, and queue reconciliation now use `spawn_blocking` workers.
- Upload dispatch, watched-folder polling, OAuth callbacks, and pre-ingest scanning already run in dedicated native workers.
- `cargo check`, focused Rust preflight tests, TypeScript check, web tests, production build, and diff check passed.
