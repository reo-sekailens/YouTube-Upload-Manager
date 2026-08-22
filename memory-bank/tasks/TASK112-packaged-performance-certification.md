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
