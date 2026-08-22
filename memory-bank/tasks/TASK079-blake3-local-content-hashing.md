# TASK079: BLAKE3 local content hashing

## Status

completed

## Scope

Replace local media SHA-256 content hashing with the official optimized BLAKE3
Rust implementation while retaining SHA-256 for OAuth PKCE as required by the
protocol.

## Acceptance criteria

- Local file hashing, resumed managed-copy hashing, watched-folder hashing,
  source-cleanup verification, and deep pre-ingest checks use BLAKE3.
- BLAKE3 digests remain deterministic, streamed, and exact-match safe across
  all supported platforms.
- OAuth PKCE remains SHA-256.
- Existing test coverage validates BLAKE3 results for complete and resumed
  streams.

## Evidence

- `blake3` 1.8.7 is locked from the official implementation and uses its
  runtime-selected optimized native paths.
- Streamed full and resumed-copy unit tests compare against BLAKE3's own
  deterministic reference output. OAuth PKCE still calls SHA-256.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 51 tests; `npm test` passed: 35 tests; `npm run build` passed.
