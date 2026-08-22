# Whole-app performance audit and optimization program

Date: 2026-08-23

Status: planning complete; no application code changed

## Outcome

The largest speed limits are structural, not cosmetic. The app repeatedly runs
schema setup on ordinary database access, performs heavy recovery before the
window can render, mounts every workspace at startup, transports full snapshots
during active work, rebuilds duplicate candidates quadratically, and recreates
HTTP clients and upload buffers for every 8 MiB chunk.

The work is split into TASK102 through TASK112. The first implementation wave
must establish measurements, then remove repeated database/bootstrap work and
inactive-workspace work before lower-impact dependency or package tuning.

## Audit coverage

The audit covered every tracked application surface:

- 10,234-line native core with 315 Rust functions, 51 Tauri commands, SQLite
  schema and queries, OAuth, YouTube inventory/upload/deletion, recovery,
  watched folders, hashing, FFprobe, archive transfer, diagnostics, and tests.
- 17 React TSX files, the 1,249-line app shell, all seven workspace tabs,
  shared bridge/types/helpers, 26 effects, seven JavaScript intervals, lists,
  search, media embeds, drag/drop, dialogs, and styles.
- Vite, TypeScript, npm and Cargo manifests/locks, Tauri desktop/mobile
  configuration, FFprobe provisioning, release workflow, assets, and existing
  performance/recovery task history.

This was a static and build-output audit. Runtime timing is not yet instrumented,
so findings are marked measured or source-derived. TASK103 creates the missing
packaged-app baselines before optimization code begins.

## Measured baseline

| Measure | Current result |
| --- | ---: |
| Production build wall time in this audit | 11.226 s |
| Vite transform/build portion | 2.75 s |
| Main JavaScript | 334,910 B raw / 96,394 B gzip |
| Main CSS | 60,454 B raw / 10,689 B gzip |
| Feature/tab chunks | 0 |
| WebviewWindow chunk | 4,799 B raw / 1,419 B gzip |
| Windows release executable already present | 19,154,944 B |
| Bundled FFprobe | 82,668,032 B |
| Current NSIS installer already present | 26,474,492 B |
| Cached FFprobe read and SHA-256 verification | about 827 ms |
| Native database() references | 152 total; 92 before the test module |
| Native source threads / blocking-worker calls | 10 / 18 |
| Frontend tests | 35; no component or performance tests |
| Native embedded unit tests | 74 |

The frontend payload is not enormous, but all of it is parsed and every
workspace is mounted. Native database and recovery work are expected to dominate
startup until TASK103 measures the packaged path.

## Ranked findings

### P0 — repeated schema work contaminates nearly every operation

The database() helper at src-tauri/src/lib.rs:439-693 does all of the following
for every short-lived connection:

- selects WAL mode and enables foreign keys;
- executes the full table/index creation batch;
- attempts 35 ALTER TABLE statements whose errors are ignored on a current
  schema;
- performs two PRAGMA table_info scans.

This path is reached by startup, initial hydration, one-second and 450 ms
polling, five-second and 30-second native loops, upload cancellation checks,
progress checkpoints, inventory, and ordinary settings reads.

Improvement: run ordered, transactional PRAGMA user_version migrations exactly
once, then use a cheap connection/service path with query-plan-backed indexes
and prepared statement reuse where measurements justify it. SQLite reserves
user_version for application use, and WAL persists after activation:
[SQLite pragmas](https://www.sqlite.org/pragma.html) and
[SQLite WAL](https://www.sqlite.org/wal.html).

### P0 — startup can hash or copy arbitrarily large media before rendering

initialize_state and Tauri setup at src-tauri/src/lib.rs:6063-6078 and
7535-7549 synchronously initialize the database and run queue/deletion
reconciliation before setup returns. reconcile_queue_impl can BLAKE3-read a
complete managed asset or resume a partial multi-gigabyte copy at
src-tauri/src/lib.rs:7365-7431.

Startup also resumes one thread per pending watched hash and one metadata worker
per persisted preflight job. Each metadata worker can launch FFprobe and another
stdout-reader thread. A large recovered history can create a launch-time
thread/process storm.

Improvement: commit a cheap transactional recovery classification before
actionable queue controls, render the safe shell, then resume expensive file
verification/copy/probe work through bounded class-aware workers. No item may
dispatch until its recovery fence is satisfied.

### P0 — all seven workspaces mount on startup

src/App.tsx:37-50 statically imports every workspace. The render at
src/App.tsx:955-1237 keeps all tabs mounted and only applies hidden. This starts
inactive effects, builds inactive DOM, loads local inventory/settings, and can
contact YouTube for playlist data from the hidden monitor panel.

A connected startup has at least about 13 native invokes before repeated monitor
reloads, plus one connection-settings invoke for every uploaded-title duplicate
card. The hidden FolderMonitorPanel effect is worse: its dependency includes an
inline callback created at src/App.tsx:1046, so ordinary App renders tear down
the timer and immediately reload the overview.

Improvement: one bootstrap envelope; only the active workspace mounted; lazy
workspace code and CSS; centralized connection/authorization state; stable
callbacks; no network request caused by an inactive surface. React.lazy defers
component code until first render, while Vite supports split dynamic imports and
parallel preload:
[React lazy](https://react.dev/reference/react/lazy) and
[Vite features](https://vite.dev/guide/features).

### P0 — full polling snapshots rebuild expensive projections

Active uploads replace the complete dashboard snapshot every second
(src/App.tsx:507-518). Preflight sends its complete files, nested metadata, and
up to 512 events every 450 ms (src/App.tsx:255-289). Folder overview reloads
every five seconds and also on unstable callback identity.

dashboard_snapshot at src-tauri/src/lib.rs:6145-6179 recomputes every duplicate
candidate. uploaded_title_duplicates compares every processed remote video pair
at src-tauri/src/lib.rs:4050-4088, which is O(V squared). Preflight result
assembly still compares every selected file against every remote title on every
snapshot.

Improvement: persist revisioned projections when inventory/items change, emit
small channel-scoped deltas, page heavy results, and retain polling only as a
bounded recovery fallback.

### P0 — upload hot loop discards connection pooling and copies each chunk

For every 8 MiB upload chunk at src-tauri/src/lib.rs:2328-2366, the native loop:

- calls the heavyweight database() path to check cancellation;
- creates a new reqwest blocking Client, forfeiting keep-alive/TLS pooling;
- clones the chunk into a new Vec;
- calls database() again to save the acknowledged progress.

Reqwest explicitly recommends creating and reusing Client because it owns a
connection pool:
[reqwest blocking Client](https://docs.rs/reqwest/latest/reqwest/blocking/struct.Client.html).

Improvement: reusable policy-specific clients in AppState, streaming/reusable
request bodies, cheap prepared cancellation/checkpoint operations, and measured
chunk sizing. YouTube says chunking adds request overhead, larger chunks are
more efficient, and non-final chunks must be equal multiples of 256 KiB:
[YouTube resumable uploads](https://developers.google.com/youtube/v3/guides/using_resumable_upload_protocol).

### P0/P1 — claimed upload concurrency is not reliably parallel

The scheduler claims multiple items at src-tauri/src/lib.rs:6004-6010, then one
spawned thread loops over those IDs sequentially at src-tauri/src/lib.rs:
2456-2507. Separate queue actions can accidentally create parallel workers, but
recovered or already-populated queues do not receive the intended bounded
parallelism.

Improvement: one durable bounded scheduler with explicit global and per-volume
permits, one worker per permit, fair handoff, and no claim held while waiting
behind a different item.

### P1 — provider and inventory work repeats avoidable setup

youtube_json builds a new HTTP client for each API page
(src-tauri/src/lib.rs:2513-2536). Inventory refresh inserts each video as an
individual autocommit statement at src-tauri/src/lib.rs:2736. Title matching
reloads all remote and local titles for each item.

Improvement: reusable transport; refresh-token singleflight; one prepared
transaction for staged inventory; persisted normalized/canonical/numeric title
keys; channel/revision-scoped duplicate projections; one fresh inventory
generation shared across a dispatch wave without weakening pre-upload checks.

### P1 — success acknowledgement is delayed by post-processing

After YouTube returns a video ID, upload_item performs playlist insertion before
persisting uploaded state, then synchronously performs guarded source cleanup,
which can rehash a multi-gigabyte original, before the worker hands off the next
item (src-tauri/src/lib.rs:2369-2418).

Improvement: atomically persist the provider video ID and success receipt first,
then schedule playlist insertion and guarded cleanup as durable lower-priority
post-processing. Cleanup validation and deletion confirmation remain unchanged.

### P1 — folder and quota loops do work while idle

The native folder thread scans immediately and every five seconds even when the
monitor is disabled (src-tauri/src/lib.rs:5934-5940). The quota thread opens the
database immediately and every 30 seconds even when no pause exists
(src-tauri/src/lib.rs:6040-6060).

The folder scan also holds one global mutex across directory enumeration,
inventory synchronization, file snapshot/hash/probe work, and writes, and
queries the observation table once per file.

Improvement: sleep indefinitely until enabled, wake at persisted deadlines or
state events, coalesce filesystem discoveries, bulk-load observations, and keep
slow provider/file work outside the short coordination critical section.

### P1 — large UI surfaces are not bounded

Queue, deletion inventory, duplicate cards, preflight files/logs/metadata, and
search results render all matching rows. Collapsed metadata is still constructed
in the DOM. Duplicate comparisons install one global message listener and one
initial settings read per card.

Improvement: pagination/windowing, at most 100 mounted data rows, lazy expanded
metadata, deferred search values, memoized row boundaries, and one shared player
message router active only for loaded embeds.

### P1 — batch actions are bridge waterfalls

Manual imports and queue transitions are submitted one item at a time from
src/App.tsx:360-419. A 100-file batch therefore produces O(N) IPC round trips
before native scheduling.

Improvement: one native batch command that persists every item independently,
uses bounded source-volume-aware workers, returns a batch receipt, and publishes
per-item deltas. Permanent deletion remains deliberately sequential.

### P1/P2 — FFprobe preparation and packaging waste work

scripts/prepare-ffprobe.mjs reads the entire cached 82.7 MB binary to hash it and
always downloads/rewrites the license. Mobile target triples can fall through to
host desktop provisioning. macOS fallback prepares both architectures. Tauri
runs preparation before every dev and build session.

Improvement: stream verification, cache a verified provenance receipt, make
cached preparation network-free, skip mobile entirely, select one architecture
unless universal packaging is explicit, bound runtime FFprobe processes, and
include the sidecar/license in the Windows portable artifact.

### P2 — configuration and module shape lack performance controls

vite.config.ts has no bundle analysis or budget. Cargo.toml has no explicit
measured release profile. src-tauri/src/lib.rs is a 10k-line subsystem monolith,
src/App.tsx owns around twenty unrelated states, and the global stylesheet has
3,403 lines. Moving files alone does not improve runtime, but the shape makes
hot paths, ownership, and focused benchmarks difficult to isolate.

Improvement: extract modules along optimized ownership boundaries after hot-path
behavior stabilizes, add local benchmarks beside each module, evaluate release
profiles against the prior small-stack regression, and retain symbols needed for
crash diagnosis.

## Target performance architecture

1. Tauri performs one versioned database bootstrap and commits a lightweight
   recovery classification.
2. The safe shell renders from one bootstrap envelope; inactive workspaces have
   no code, effects, DOM, or provider calls.
3. A bounded native scheduler resumes recovery, disk, probe, and provider work
   by resource class.
4. SQLite remains the source of truth. Native events carry immutable channel ID,
   entity ID, revision, and a small delta; missed events recover from a paged
   snapshot.
5. Duplicate/title projections rebuild only when their channel inventory or
   upload revision changes.
6. Upload workers reuse transport and database resources, persist every
   provider acknowledgement, and publish coalesced progress.
7. Provider success is durable before playlist and source-cleanup post-work.

## Performance budgets

TASK103 must measure the reference machine before these become gates. The
provisional program targets are:

- at least 50% improvement in cold and warm packaged startup p50/p95;
- no media hashing, FFprobe, or network before first safe-shell render;
- zero periodic webview invokes and negligible database opens in settled idle;
- at most one bootstrap invoke before the initial Batch workspace is usable;
- initial JS at or below 235 KiB raw / 70 KiB gzip and initial CSS at or below
  40 KiB raw;
- fewer than 100 mounted data rows/cards with 10,000-record fixtures;
- search key-to-paint p95 below 100 ms and no reference-flow task above 50 ms;
- dashboard and paged-query p95 budgets recorded at 0, 100, 1,000, and 10,000
  records, with query plans that use intended indexes;
- upload throughput at least 90% of the local mock server/file-read baseline,
  bounded memory per worker, and one durable checkpoint per acknowledged range;
- explicit maximums for upload, hash, copy, and FFprobe concurrency;
- repeated cached FFprobe preparation performs no network request.

## Delivery sequence

| Wave | Tasks | Purpose |
| --- | --- | --- |
| 0 | TASK103 | Instrument and freeze comparable baselines/budgets. |
| 1 | TASK104, TASK106, TASK110 | Remove repeated DB work, inactive UI work, and build/sidecar waste in parallel. |
| 2 | TASK105, TASK107, TASK108, TASK109 | Reshape startup, updates, upload throughput, and heavy algorithms. |
| 3 | TASK111 | Finish ownership-based module extraction without changing behavior. |
| 4 | TASK112 | Certify packaged/runtime speed and safety on each available platform. |

TASK102 is the master ledger and is complete only when TASK103 through TASK112
meet their evidence requirements.

## Invariants

- No application backend, cloud telemetry, remote database, or third-party speed
  test.
- Every cache, projection, page, event, scheduler permit, and receipt is scoped
  to the immutable channel/account identity.
- Tokens and resumable-session URIs remain in OS-protected storage.
- SQLite checkpoints remain authoritative; events are presentation hints.
- Every acknowledged YouTube range is durably checkpointed.
- No automatic deletion, weakened typed confirmation, or parallel destructive
  action.
- A faster startup may show only a safe readiness/recovery shell until
  classification is committed; it may not expose dispatchable work early.
- Local/mock/package results stay distinct from live Google/YouTube proof.

## Task files

- [TASK102](tasks/TASK102-whole-app-performance-program.md)
- [TASK103](tasks/TASK103-performance-baseline-and-budgets.md)
- [TASK104](tasks/TASK104-fast-sqlite-lifecycle-and-queries.md)
- [TASK105](tasks/TASK105-two-phase-fast-startup-recovery.md)
- [TASK106](tasks/TASK106-lazy-webview-and-render-isolation.md)
- [TASK107](tasks/TASK107-revisioned-events-and-incremental-state.md)
- [TASK108](tasks/TASK108-provider-transport-and-upload-scheduler.md)
- [TASK109](tasks/TASK109-inventory-dedupe-and-batch-pipelines.md)
- [TASK110](tasks/TASK110-media-probe-and-build-pipeline.md)
- [TASK111](tasks/TASK111-native-performance-module-refactor.md)
- [TASK112](tasks/TASK112-packaged-performance-certification.md)
