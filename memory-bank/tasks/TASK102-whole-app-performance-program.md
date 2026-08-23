# TASK102: Whole-app performance optimization program

## Status

in_progress

## Objective

Deliver a measured, app-wide speed refactor covering startup, idle behavior,
interaction latency, local persistence, large libraries, intake, duplicate
review, watched folders, resumable uploads, build preparation, and packaged
runtime behavior.

## Non-goals

- No optimization by weakening crash recovery, channel isolation, durable
  checkpoints, OAuth storage, duplicate review, or deletion confirmation.
- No application backend, telemetry service, remote database, or third-party
  speed test.
- No claim that source/build success proves packaged or live-provider speed.

## Work breakdown

- TASK103 records local-only baselines and budgets.
- TASK104 removes repeated schema/query overhead.
- TASK105 splits cheap startup classification from bounded recovery work.
- TASK106 lazy-loads and isolates webview workspaces and large lists.
- TASK107 replaces steady full polling with revisioned native deltas.
- TASK108 reuses provider transport and implements real bounded upload workers.
- TASK109 makes inventory, duplicate, preflight, and batch work scale linearly.
- TASK110 optimizes media probing and FFprobe/build preparation.
- TASK111 completes ownership-based native module extraction.
- TASK112 certifies the resulting packaged behavior.

## Acceptance criteria

- TASK103 through TASK111 are completed with their required local evidence;
  TASK112 closes the available packaged boundary and records unavailable
  platform/provider boundaries explicitly.
- Cold/warm packaged startup p50 and p95 are compared only on equivalent
  hardware, storage, profile, marker, and WebView isolation semantics. The
  frozen reference's pre-safe-shell Batch marker and low-headroom system-drive
  profile are not equivalent to the final harness and cannot support a valid
  50% claim.
- Settled idle has zero periodic webview invokes and no disabled monitor scan.
- Upload throughput reaches at least 90% of the local mock transport/file-read
  baseline with bounded memory and durable acknowledged-range checkpoints.
- 10,000-record fixtures remain responsive and never create unbounded DOM,
  worker, process, or query fan-out.
- Existing safety, account isolation, recovery, and destructive-action tests
  remain green.

## Dependencies

TASK037, TASK079, TASK095, TASK100

## Affected areas

src/, src-tauri/, scripts/, package.json, vite.config.ts, CI, and memory-bank
performance evidence.

## Evidence

- Planning audit: memory-bank/performance-audit-2026-08-23.md.
- Frozen reference evidence: memory-bank/performance-baseline-2026-08-23.md.
- Implementation began only after the audit and scoped task program were
  recorded. TASK103 instrumentation is isolated from the production profile and
  provider surfaces.
- TASK103 through TASK111 are implemented. The final local checks passed 75/75
  frontend tests, 127 native tests with zero failures and five intentionally
  ignored release-only benchmarks, the frontend payload gate, and 6/6 FFprobe
  preparation tests.
- The authoritative unsigned Windows performance harness measured 80 cold and
  80 warm empty-profile launches on a healthy-headroom SATA HDD. Safe-shell
  paint was 2,970/3,286 ms cold and 158/406 ms warm (p50/p95); first Batch paint
  was 4,103/4,378 ms cold and 967/1,198 ms warm. All six settled-idle delta
  counters were zero in every run.
- The local-browser 10,000-row Batch harness passed 40 searches at 44/87 ms
  p50/p95 and 40 clear interactions at 42/85 ms, with at most 32 mounted rows,
  zero interaction Long Tasks, and zero runtime errors.
- The master task remains `in_progress` until TASK112 adds the final
  interrupted-profile result and a standard unsigned package/smoke result. A
  signed-production package, live Google/YouTube canary, non-Windows packages,
  and equivalent healthy-headroom NVMe comparison remain separate unavailable
  evidence boundaries, not implied failures of the implemented local paths.
