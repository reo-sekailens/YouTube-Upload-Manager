# TASK111: Native performance module refactor

## Status

completed

## Objective

Complete an incremental ownership-based extraction of the optimized native
runtime so hot paths, locks, resource limits, and benchmarks remain reviewable
without a risky behavior-changing rewrite.

## Scope

- Extract persistence/migrations, provider transport, upload/scheduler,
  recovery, folder monitor, preflight/media, OAuth/secure storage, commands,
  diagnostics, and platform helpers as their performance tasks stabilize.
- Give each subsystem explicit state ownership, lock boundaries, cancellation,
  concurrency limits, errors, and test fixtures.
- Preserve public Tauri command names, serialized schemas, SQLite compatibility,
  audit records, and channel/account isolation.
- Move focused tests and benchmarks beside their owner module while retaining
  end-to-end recovery and command-contract coverage.
- Measure clean and incremental Rust compilation plus release binary changes;
  do not claim runtime improvement from file movement alone.
- Evaluate measured thin LTO, codegen-unit, stripping, and panic settings while
  retaining symbols and panic/crash evidence needed for support.

## Acceptance criteria

- TASK104 through TASK110 behavior and performance suites pass without changing
  the webview command contract or historical database migration outcome.
- No application subsystem retains an unbounded worker/process spawn or holds a
  coordination/database lock across provider, FFprobe, hash, or copy work.
- The existing native test inventory is retained or expanded, with focused
  module tests and packaged startup/recovery coverage.
- Release-profile choices include before/after startup, throughput, size, and
  prior small-stack regression evidence; unsupported settings are rejected.
- architecture.md, technical-notes.md, and task evidence reflect actual module
  ownership and validated behavior.

## Dependencies

TASK104, TASK105, TASK106, TASK107, TASK108, TASK109, TASK110.

## Affected areas

src-tauri/src/, native test/benchmark layout, Cargo release profile, and memory
bank architecture/technical documentation.

## Implemented slice

- Extracted the stabilized media runtime from the native command/orchestration
  file into `src-tauri/src/media_runtime.rs`. The module owns source-volume
  scheduling, active-upload priority guards, resumable copy plus BLAKE3,
  sequential hashing, bounded/cancellable FFprobe execution, ISO-BMFF duration
  parsing, and the stable-signature in-memory probe cache.
- Kept SQLite cancellation lookup, preflight/upload schema mapping, Tauri
  commands, audit writes, crash recovery, watched-source final integrity, and
  provider orchestration in `lib.rs`. This preserves the existing security,
  channel, database, and command-contract boundaries rather than introducing a
  cross-subsystem abstraction.
- Moved the focused scheduler/probe/cache/cancellation/copy fixtures beside the
  media owner. The end-to-end 512 KiB startup-recovery fixture remains in
  `lib.rs` because it verifies the integration seam and prior stack-overflow
  regression.
- Made successful JSON output a type-level requirement for cache insertion.
  Spawn, timeout, cancellation, oversized output, non-zero exit, and malformed
  JSON failures clear their in-flight key and remain retryable for the same
  stable file signature.

## Build-profile decision

- No Cargo release-profile setting was changed. Panic abort and symbol
  stripping were rejected because they would remove the panic/crash evidence
  required by local recovery and support diagnostics.
- Thin LTO/codegen-unit changes were not adopted without a complete comparable
  link, startup, throughput, size, and 512 KiB-stack result. An isolated default
  cold-build candidate exhausted the available drive before link (`os error
  112`), so it is recorded as invalid evidence, not as a performance result.
  Its verified temporary target was removed with `cargo clean --target-dir`
  (4,012 generated files, 1.6 GiB); the shared target was not cleaned.

## Validation evidence

- `cargo check --manifest-path src-tauri/Cargo.toml --lib` passed after the
  extraction cutover in 7.75 seconds.
- The final integrated performance-harness native suite passed 127 tests with
  0 failures and 5 intentionally ignored release-only benchmarks. This includes
  all nine media-runtime fixtures after the copy-resume fixture moved beside its
  owner.
- The interrupted-copy, 512 KiB small-stack startup recovery, and database-only
  startup classification fixtures each passed 1/1 after extraction.
- `npm run performance:native:check -- --configuration-only` passed with four
  uploads per volume maximum, one startup-resident worker, and one allowed
  direct spawn boundary.
- TASK108's quiet-window optimized local-loopback distribution passed at p50
  177.561 ms, p95 205.575 ms, 360.439 MiB/s, and 163.47% of its pooled-streaming
  reference. This is upload-loop evidence only; it is not packaged startup or
  installer evidence.
- The final production copy path was narrowed after TASK110's healthy-HDD
  comparison to a 1 MiB heap buffer with standard opens, while the independent
  read-only digest path retains its 8 MiB sequential strategy. The module seam,
  final `sync_all`, cancellation, resume, and bounded scheduling are unchanged.

## Follow-up boundaries

- TASK110 owns the retained healthy-HDD copy decision and unsigned harness
  sidecar provenance. TASK112 owns standard unsigned-production, signed,
  recovery, installer, portable, and provider certification; none is inferred
  from source/module extraction.
