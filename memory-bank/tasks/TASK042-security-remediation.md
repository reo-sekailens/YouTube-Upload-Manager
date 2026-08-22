# TASK042 — Security scan remediation

## Status

Completed

## Scope

Remediate the three validated findings from the 2026-08-22 security report:
immutable channel bindings for automation, exclusive upload dispatch claims, and
resilient state-bound OAuth callbacks.

## Acceptance criteria

- Watched-folder dispatch compares immutable YouTube channel IDs and pauses
  legacy title-only bindings.
- Concurrent queue starts yield one durable claim per upload item.
- Invalid loopback requests do not consume the PKCE verifier or end the
  authorization listener.
- Focused Rust coverage proves each regression boundary.

## Evidence

- Watched folders now persist the YouTube channel ID captured from the
  authenticated `channels.list` response. Dispatch requires that ID to match
  the active connection; prior title-only monitor rows pause and require the
  operator to reconnect and enable the monitor again.
- Queue dispatch uses an atomic `queued` to `dispatching` compare-and-set.
  Competing start requests receive only their own successful claims.
- The loopback listener now retains the PKCE verifier for every request except
  a `GET /oauth2/callback` carrying the expected OAuth state.
- Local verification passed: `cargo fmt --manifest-path src-tauri/Cargo.toml
  --check`, `cargo test --manifest-path src-tauri/Cargo.toml --lib
  --no-fail-fast` (26 passed), `cargo check --manifest-path
  src-tauri/Cargo.toml`, `npm run check`, `npm test` (28 passed), `npm run
  build`, and `git diff --check`.

## Follow-up

- Live Google OAuth, watched-folder, and resumable-upload canaries remain
  unrun because they require an operator-authorized YouTube test channel.
