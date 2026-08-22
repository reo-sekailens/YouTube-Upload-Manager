# Contributing to YouTube Upload Manager

Read the [Contributor Agreement](CONTRIBUTOR_AGREEMENT.md) before opening a pull request.

## Local setup

```bash
npm install
npm run dev
```

For the native desktop shell, run `npm run tauri -- dev`.

The app is React + Vite in `src/` and Tauri/Rust in `src-tauri/`. Do not add an application backend, telemetry service, remote database, or cloud media store.

## Working safely

- Keep OAuth tokens, authorization codes, JSON credentials, local media paths, and provider payloads out of the webview, source control, and logs.
- Put filesystem, SQLite, OAuth, and YouTube API work in Rust; keep React responsible for interaction and presentation.
- Preserve account/channel isolation and crash-safe, resumable operation state.
- Keep destructive actions reviewed, typed-confirmed, and auditable.
- Preserve cross-platform behavior: desktop drag/drop and mobile document picking share the native validation boundary.

## Before opening a pull request

```bash
npm test
npm run check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo check --manifest-path src-tauri/Cargo.toml
git diff --check
```

Update the applicable `memory-bank/tasks/` entry and `memory-bank/progress.md` when a workflow, boundary, or verified result changes. State which checks were local and which used a real provider account. Sign off every commit with `git commit -s`.
