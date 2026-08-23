# TASK103: Performance baseline and budgets

## Status

completed

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

## Evidence

- `memory-bank/performance-baseline-2026-08-23.md` records the redacted
  reference workstation, packaged cold/warm p50/p95, bundle sizes, and native
  0/100/1,000/10,000 scale results.
- The frozen 10,000-record dashboard/dedupe baseline took 177,330.510 ms for a
  single release sample; final optimized certification must restore the normal
  seven-sample distribution.
- `scripts/performance/frontend-baseline.mjs` and
  `tests/performance-baseline.test.ts` produce deterministic local bundle
  reports and explicit budgets.
- The final Windows runner records two reversed 40-run blocks plus five untimed
  warmups, nearest-rank p50/p90/p95/max, raw chronological receipts, isolated
  WebView2 profiles, storage headroom/provenance, SQLite integrity/cardinality,
  and all six settled-idle deltas. No outlier is removed.
- `src-tauri/src/upload_performance_benchmarks.rs` passed a seven-session
  loopback `308 Range` benchmark for file-read, current-shaped, and pooled
  streaming modes. The tracked baseline records its redacted p50/p95 and copy/
  memory characteristics; TASK108 owns the final throughput gate.
- The frozen final-v3 local checks passed TypeScript, 16 frontend files/87
  tests, 140 native tests with zero failures and five ignored release-only
  benchmarks, 6/6 FFprobe tests, and deterministic bundle/query/worker gates.
  Initial assets passed at 228,995 B JavaScript raw, 71,496 B gzip, and
  38,470 B CSS. TASK108's upload
  fixture passed at 204.516/239.985 ms p50/p95 and 2.0429x its pooled-streaming
  reference.
- `memory-bank/certification/performance-certification-2026-08-23.md` records
  the authoritative final-v3 unsigned empty/interrupted matrices and browser
  interaction result. The runner correctly rejected one preceding empty attempt
  when a settled-idle duration fell outside 1,900–2,200 ms and emitted no
  aggregate. TASK112 adds standard unsigned production artifact integrity;
  final-v3 ordinary-executable launch remains unexercised.
- The historical startup reference remains retained, but it used a different
  first-Batch marker, shared/low-headroom storage conditions, and non-equivalent
  WebView profile semantics. It is therefore descriptive baseline evidence,
  not a valid denominator for the provisional 50% target.
