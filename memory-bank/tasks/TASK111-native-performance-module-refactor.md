# TASK111: Native performance module refactor

## Status

proposed

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
