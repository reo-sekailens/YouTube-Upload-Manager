# TASK112: Packaged performance certification

## Status

proposed

## Objective

Prove the optimized app starts and operates quickly in real packaged conditions
without conflating source, browser, mock-provider, signed-build, or live-provider
evidence.

## Scope

- Run repeatable cold/warm Windows packaged startup, first-interaction, idle,
  large-library, interrupted-recovery, import, monitor, preflight, and mock
  resumable-upload scenarios.
- Capture p50/p95 timings, RSS, CPU/wakeups, disk/network volume, database opens,
  worker/process peaks, bundle/binary/installer size, and long tasks.
- Add deterministic CI gates for bundles, query plans, fixture algorithms,
  worker bounds, idle behavior, and regressions; keep variable wall-clock gates
  non-blocking until stable.
- Re-run safety, account isolation, recovery, secure-store, resumable checkpoint,
  duplicate review, and destructive-action certification.
- Record macOS, Linux, Android, and iOS results only when the required hardware,
  toolchain, and package are actually available.
- Keep an explicitly authorized non-production YouTube canary separate from
  local mock throughput and local package certification.

## Acceptance criteria

- The completed report compares TASK103 and final p50/p95 results on equivalent
  hardware/profile fixtures and explains material variance.
- Cold and warm startup meet the final TASK102 target; a simulated interrupted
  256 GB fixture size does not materially affect first safe-shell time.
- Settled idle, large-list responsiveness, upload throughput, bounded memory,
  query, event, and worker/process budgets all pass.
- Current frontend/native suites, recovery fixtures, and packaged Windows smoke
  tests pass with no weakened privacy, security, or deletion boundary.
- Source/build, unpackaged runtime, packaged unsigned, signed production, and
  live-provider evidence are reported as distinct certification levels.
- No benchmark artifact contains credentials, channel IDs, filenames, source
  paths, media contents, or provider payloads.

## Dependencies

TASK103, TASK104, TASK105, TASK106, TASK107, TASK108, TASK109, TASK110, TASK111.

## Evidence destinations

memory-bank/certification/, TASK102, TASK112, progress.md, and release evidence
for each actually exercised platform.

## Implemented harness fixture slice

- The Windows packaged runner requires an explicit `empty` or
  `interrupted-256gb` fixture. The interrupted template contains one synthetic
  pre-existing `uploading` row with a declared size of 256,000,000,000 bytes,
  empty local path fields, no channel, credential, secure-store session,
  provider identifier, or media, and a reported media footprint of zero bytes.
- Fixture insertion exists only in a `performance-harness` build. A second
  seed-only environment gate inserts the row transactionally and idempotently
  into a marker-protected isolated profile, writes redacted cardinality/size
  metadata, and exits before startup recovery. Regular builds neither read nor
  react to either fixture environment variable.
- The seed process is untimed. After it exits, every cold, warmup, and warm
  launch receives a separate clone of the closed template. The measured process
  explicitly removes both seed variables, begins with the pre-existing
  `uploading` row, and exercises normal database-only interrupted-upload
  classification without creating or reading a 256 GB file.
- Clone and template cleanup requires both containment beneath the explicitly
  empty disposable root and the performance-profile marker. The output contains
  fixture ID, counts, declared bytes, zero media bytes, and booleans only; it
  does not copy the synthetic SQLite profile into the report directory.
- The settled-idle interval is fixed at two seconds. A run fails if its final
  native snapshot omits any delta for periodic invokes, database opens, SQLite
  statements, event messages, worker threads, or FFprobe processes.
- The Windows harness builder invokes the installed Tauri JavaScript CLI with
  `process.execPath` and no command shell. This avoids Node 24's Windows
  `spawnSync` failure on `npm.cmd` while preserving feature, bundle, signing,
  environment, and caller-supplied arguments.

## Harness fixture evidence

- PowerShell parser validation passed, and `-Help` exposes only the two accepted
  fixture names.
- A validate-only interrupted-fixture proof returned `valid: true`,
  `declaredTotalBytes: 256000000000`, `mediaBytesWritten: 0`, cloned-template
  and measured-environment-removal receipts, plus all six settled-idle delta
  dimensions. The temporary fake-signature executable/profile used only for
  argument validation was removed afterward.
- `node --check scripts/performance/build-windows-harness.mjs` passed, and the
  focused launcher/bundle-baseline test file passed 3/3. Its launcher regression
  asserts the Node executable, installed Tauri CLI path, complete argument and
  environment propagation, plus the absence of `npm.cmd` and shell execution.
- Native feature compilation, seed-template execution with the real packaged
  harness, and measured empty-versus-interrupted startup remain pending. This
  validate-only result is script/safety-contract evidence, not packaged timing
  or recovery certification.
