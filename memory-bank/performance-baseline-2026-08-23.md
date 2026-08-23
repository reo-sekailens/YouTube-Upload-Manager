# TASK103 reference performance baseline

## Evidence boundary

These results are device-local synthetic evidence from the reference Windows
workstation. They contain no tokens, OAuth responses, channel identifiers,
source paths, filenames, media bytes, or provider payloads. Packaged results use
an unsigned `performance-harness` build, a compile-time isolated secure-store
service, and disposable empty profile roots. They are not live YouTube evidence.

## Reference environment

- Application version: 0.1.9.
- Operating system: Microsoft Windows NT 10.0.26200.0, x64.
- Logical processors: 16.
- Physical memory: 33,660,211,200 bytes.
- Harness executable: 19,184,128 bytes.
- Packaged iterations: 10 cold, 2 unmeasured warmups, 10 warm.
- Settled sample window: 2 seconds after startup.

The ignored raw JSON and Markdown outputs are written under
`output/performance/`. The tracked tables below retain only redacted aggregate
evidence needed for before/after certification.

## Packaged empty-profile startup

| Metric | Cold p50 | Cold p95 | Warm p50 | Warm p95 |
| --- | ---: | ---: | ---: | ---: |
| Window handle visible | 113.69 ms | 983.63 ms | 109.76 ms | 180.35 ms |
| Native ready | 486 ms | 861 ms | 480 ms | 775 ms |
| First Batch paint | 667 ms | 1,089 ms | 641 ms | 983 ms |
| Database opens | 14 | 17 | 14 | 14 |
| SQLite statements | 332 | 401 | 332 | 332 |
| Idle private bytes | 6,737,920 | 7,503,872 | 6,455,296 | 6,647,808 |
| CPU during 2-second idle window | 234 ms | 344 ms | 250 ms | 406 ms |

The window-handle result is not the usable-startup metric: the handle can exist
before native initialization and Batch rendering complete. TASK112 compares
native-ready and first-Batch-paint milestones.

## Frontend payload

| Metric | Reference | Provisional budget | Status |
| --- | ---: | ---: | --- |
| Initial JavaScript, raw | 335,073 B | 240,640 B | over |
| Initial JavaScript, gzip | 96,246 B | 71,680 B | over |
| Initial JavaScript, Brotli | 82,605 B | observation only | recorded |
| Initial CSS, raw | 60,454 B | 40,960 B | over |
| Initial CSS, gzip | 10,541 B | observation only | recorded |
| Lazy JavaScript chunks | 1 shared Tauri chunk | feature tabs must be split | over |

The provisional raw/gzip/CSS budgets are retained because the reference bundle
is a single eager feature payload; TASK106 must meet them or record comparable
render evidence that justifies an adjustment.

## Synthetic native scale fixtures

Release-only fixtures use new temporary SQLite databases and generated records.
Seven samples include one warmup unless stated otherwise.

| Surface | Fixture | Samples | Minimum | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Dashboard plus duplicate reconciliation | 0 | 7 | 3.344 ms | 3.718 ms | 5.349 ms |
| Dashboard plus duplicate reconciliation | 100 | 7 | 15.849 ms | 18.381 ms | 24.563 ms |
| Dashboard plus duplicate reconciliation | 1,000 | 7 | 1,584.748 ms | 1,859.469 ms | 1,931.484 ms |
| Dashboard plus duplicate reconciliation | 10,000 | 1 | 177,330.510 ms | 177,330.510 ms | 177,330.510 ms |
| Preflight snapshot | 1,000 | 7 | 7.433 ms | 8.021 ms | 243.513 ms |
| Folder monitor overview | 10,000 | 7 | 7.160 ms | 8.195 ms | 9.249 ms |
| BLAKE3 read | 64 MiB | 7 | 31.522 ms | 47.896 ms | 70.882 ms |
| Copy plus BLAKE3 | 64 MiB | 7 | 291.593 ms | 335.852 ms | 368.178 ms |

The 10,000-record dashboard/dedupe path was intentionally sampled once because
the unoptimized all-pairs implementation took 177.3 seconds. Optimized
certification must restore seven samples and prove bounded scaling.

## Loopback resumable-upload fixture

The release-only fixture sends generated 64 MiB media to an IPv4-loopback HTTP
server in eight 8 MiB chunks. It verifies every byte and every acknowledged
`308 Range` before advancing. Seven sessions were measured per transfer mode.

| Surface | p50 | p95 | p50 throughput | Full chunk copies | App chunk memory |
| --- | ---: | ---: | ---: | ---: | ---: |
| File-read reference | 16.685 ms | 145.979 ms | 3,835.850 MiB/s | 0 | 8 MiB |
| Current-shaped resumable transfer | 212.025 ms | 226.182 ms | 301.850 MiB/s | 1/request | 16 MiB |
| Pooled streaming reference | 371.430 ms | 397.322 ms | 172.307 MiB/s | 0 | 0 |

The zero-copy streaming reference is slower on this Windows loopback fixture,
so TASK108 must benchmark pooled reusable buffers as well as streaming bodies;
it must not assume fewer copies automatically means higher throughput. The
current-shaped path reaches 7.87% of file-read throughput and constructs a new
HTTP client for every chunk, leaving substantial scheduler/transport headroom.

## Optimized checkpoints before packaged certification

These source/release/browser checkpoints are not a substitute for TASK112's
fresh packaged comparison, but they are deterministic gates for subsequent
work.

| Metric | Reference | Current checkpoint | Change |
| --- | ---: | ---: | ---: |
| Initial JavaScript raw | 335,073 B | 227,210 B | -32.19% |
| Initial JavaScript gzip | 96,246 B | 70,741 B | -26.50% |
| Initial CSS raw | 60,454 B | 38,470 B | -36.36% |
| Dashboard/dedupe at 10,000 | 177,330.510 ms, 1 sample | 140.579 ms p50 / 156.155 ms p95, 7 samples | about 1,135x faster at p95 comparison |
| Current-schema hot open | repeated schema batch | one `foreign_keys` statement; zero schema/journal transitions | schema churn removed |

## Confirmed optimization gates

- Cold and warm first-Batch-paint p50 and p95 improve by at least 50%.
- A current-schema steady-state open performs zero schema DDL, `table_info`, or
  journal-mode transition statements.
- Initial JavaScript is at most 240,640 B raw and 71,680 B gzip; initial CSS is
  at most 40,960 B raw.
- A 10,000-record dashboard/dedupe fixture is sampled seven times and no longer
  performs an all-pairs comparison.
- Settled idle performs zero periodic webview invokes when monitoring is
  disabled.
- Query/worker/bundle limits are deterministic CI gates; timing distributions
  remain reported but non-gating until cross-run variance is characterized.
- Upload final throughput and acknowledged-range durability remain pending
  TASK108. Cached FFprobe preparation now has a no-network provenance fast path;
  runtime probe/copy scheduling and full final comparison remain under TASK110.
