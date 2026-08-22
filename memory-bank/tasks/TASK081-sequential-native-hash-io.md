# TASK081: Sequential native hash I/O

## Status

completed

## Scope

Improve sustained desktop content-hash reads for large files, especially files
on HDD, USB, and removable storage, without weakening exact BLAKE3 evidence or
mobile picker support.

## Acceptance criteria

- Desktop file paths use a native direct file handle for deep hashing.
- Windows sets its sequential-scan file hint for one-pass hashes.
- Hash read blocks are large enough to reduce filesystem call overhead while
  remaining heap-backed and safe for the native worker.
- Android/iOS picker handles retain their platform-aware filesystem path.

## Evidence

- Desktop paths now bypass the mobile-handle abstraction and use a direct native
  file handle. Windows applies `FILE_FLAG_SEQUENTIAL_SCAN`; the streamed buffer
  increased from 1 MiB to 8 MiB while staying on the heap.
- Android/iOS URLs still use the platform-aware filesystem handle and retain
  iOS scoped-resource cleanup.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 51 tests; `npm test` passed: 35 tests; `npm run build` passed.
