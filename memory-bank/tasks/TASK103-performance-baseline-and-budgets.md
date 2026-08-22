# TASK103: Performance baseline and budgets

## Status

ready

## Objective

Create reproducible, device-local performance measurements before optimization
so every later speed claim has comparable p50/p95 evidence.

## Scope

- Instrument process start, window visibility, recovery classification,
  bootstrap readiness, first Batch render, and first interaction.
- Count native invokes, database opens/DDL, queries, event messages, threads,
  FFprobe processes, and React commits/long tasks.
- Add deterministic 0, 100, 1,000, and 10,000-record fixtures plus a large
  interrupted import/upload profile.
- Benchmark dashboard/preflight/folder reads, BLAKE3/copy, inventory staging,
  resumable chunks against a local mock 308 server, and cold/cached FFprobe
  preparation.
- Record bundle, executable, sidecar, installer, RSS, idle CPU/wakeups, and
  debug/release differences.

## Acceptance criteria

- Results are emitted locally as redacted JSON and Markdown; no telemetry or
  remote upload is introduced.
- Cold/warm packaged runs report p50 and p95 with the reference hardware,
  profile state, app version, and fixture size.
- Benchmark data contains no tokens, OAuth responses, channel IDs, source
  paths, filenames, media bytes, or provider payloads.
- CI receives deterministic bundle/query/worker-count budgets; timing history
  remains non-gating until variance is understood.
- Provisional budgets from the performance audit are confirmed or adjusted with
  a recorded reason before TASK104 onward can be called complete.

## Dependencies

None.

## Validation

- Run benchmark fixtures repeatedly in release mode.
- Run current frontend/native tests and git diff --check.
- Record packaged Windows evidence separately from browser and mock evidence.
