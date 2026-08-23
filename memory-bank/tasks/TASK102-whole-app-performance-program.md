# TASK102: Whole-app performance optimization program

## Status

completed

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
- TASK103 through TASK111 are implemented. The frozen final-v3 local checks
  passed TypeScript, 16 frontend files/87 tests, 140 native tests with zero
  failures and five intentionally
  ignored release-only benchmarks, the frontend payload gate, and 6/6 FFprobe
  preparation tests.
- Frozen source is
  `I:\YouTube-Upload-Manager-Certification-20260823\source-current-main-48fa-task114`
  at manifest commit `68a69935160d27e20c21d54b71922fbd92739a9a` and dirty-status
  SHA-256 `35F110ADC3F3A861CCB49DB191C462C497A465135A431F88920676A448704E09`;
  containing `main` commit `555b1fac5e8f89a7ab6bc53f6f3a83b8a3a54e77` was recorded later.
  Later Tailwind/action-icon migration work is outside this artifact.
- The authoritative unsigned Windows final-v3 performance harness measured 80
  cold and 80 warm empty-profile launches on a healthy-headroom SATA HDD.
  Safe-shell paint was 3,098/3,806 ms cold and 323/613 ms warm (p50/p95);
  first Batch paint was 4,628/5,270 ms and 1,652/2,983 ms. All six
  settled-idle counter maxima were zero.
- The final-v3 local-browser 10,000-row Batch harness passed 40 searches at
  33/67 ms p50/p95 and 40 clear interactions at 33/67 ms, with at most 32 mounted rows,
  zero interaction Long Tasks, and zero runtime errors.
- TASK112's authoritative final-v3 interrupted profile passed: cold
  safe-shell p50/p95 changed -2.23%/-9.14% and first-Batch changed only
  +5.12%/+4.08% versus final-v3 empty, within the
  10% materiality criterion, with zero missing safe-shell receipts and zero
  settled-idle violations across 160 launches.
- The final-v3 ordinary unsigned x64 production executable, NSIS installer, and
  portable ZIP were hash verified; ZIP readback matched the x64 executable,
  FFprobe, and license, and the runner rejected the ordinary executable as
  non-harness. The final-v3 ordinary executable was not launched. A prior
  exact-path 81-item smoke remains historical only.
- The program is complete for available local/source/browser and unsigned
  Windows evidence. Signed production, live Google/YouTube, non-Windows
  packages, and an equivalent healthy-headroom NVMe comparison remain separate
  unavailable external evidence boundaries, not implied results.
