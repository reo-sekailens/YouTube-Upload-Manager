# TASK108: Provider transport and upload scheduler

## Status

completed

## Objective

Maximize resumable-upload throughput with pooled transport and true bounded
parallelism while preserving acknowledged checkpoints, channel binding,
cancellation, and crash recovery.

## Scope

- Store reusable, timeout-configured HTTP clients in native state by transport
  policy; remove per-page and per-chunk client construction.
- Add expiry-aware access-token caching in Rust memory with refresh
  singleflight. Tokens and session URIs remain outside the webview, logs, and
  plaintext files.
- Reuse bounded chunk buffers or streaming bodies and benchmark equal chunk
  sizes that are multiples of 256 KiB; persist every provider acknowledgement
  before the next range.
- Make cancellation and checkpoint reads/writes use prepared hot paths from
  TASK104.
- Replace batch-claim-then-serial execution with a durable worker scheduler that
  enforces global and per-source-volume permits and fair handoff.
- Persist the YouTube video ID and success receipt immediately, then run
  playlist insertion and guarded source cleanup as durable, lower-priority jobs.
- Keep destructive remote deletion deliberately sequential and operator
  confirmed.

## Acceptance criteria

- A capacity of two or four produces genuinely overlapping transfers in a
  barrier-based local mock-server test without exceeding either permit limit.
- Normal resumable sessions reuse pooled connections and avoid an extra full
  chunk allocation/copy per request.
- Mock 308, token expiry, cancellation, interruption, provider success, playlist
  failure, cleanup failure, and relaunch tests preserve exact durable state.
- Aggregate upload throughput reaches at least 90% of the reference local mock
  transport/file-read baseline with bounded memory per worker.
- No item remains claimed behind another serial transfer, starts a duplicate
  session, or crosses its immutable channel/account binding.
- Provider success becomes visible and the next worker can run before optional
  playlist and source-cleanup work completes.
- Worker creation, retry delays, and transport timeouts are explicitly bounded.

## Dependencies

TASK103, TASK104.

## Affected areas

Native OAuth/YouTube transport, resumable upload loop, queue scheduler,
post-processing persistence, local mock provider, and concurrency tests.

## Implemented

- Added two lazily constructed, rustls-backed `reqwest` clients in native
  provider state: a 10 s connect/45 s control policy and a 15 s connect/30 min
  upload policy, both with a 90 s idle pool and eight idle connections per
  host. Native state construction builds zero clients, so the pre-shell startup
  path remains database-only; the first request builds one client per policy.
- Added expiry-aware access-token caches for upload and deletion grants. Their
  refresh locks provide one refresh for concurrent callers, their 60 s expiry
  skew avoids handing out near-expiry credentials, and connection/revocation
  boundaries invalidate only the applicable cache. Refresh tokens remain in
  OS secure storage and access tokens remain process memory only.
- Replaced per-chunk client construction and the persistent-buffer-plus-clone
  body path with one pooled client handle per resumable transfer and one exact,
  request-owned bounded chunk buffer per worker. Each `308` range is committed
  before the next chunk; cancellation reads reuse one prepared SQLite statement
  and interrupted requests retain their durable session/reconciliation state.
- Added a scheduler over durable SQLite claims with four global upload permits,
  cached per-volume limits, round-robin volume handoff, duplicate-safe claims,
  and one native worker per claimed item. Claims are synchronized from SQLite after
  relaunch and released before the next dispatch, so optional post-processing
  cannot hold an upload permit.
- Provider video ID, confirmed bytes, immutable channel binding, upload state,
  and audit receipt now commit in one immediate transaction before playlist or
  source-cleanup work. Schema v3 adds durable playlist state and a pending-work
  index. A singleton lower-priority worker resumes eligible work for only the
  active channel; playlist failure cannot erase the provider receipt and source
  cleanup remains guarded and retryable.
- Remote YouTube deletion remains deliberately sequential, typed-ID confirmed,
  and independently authorized. It reuses the control client/token cache but
  gains no parallel destructive path.

## Evidence

- The final integrated native performance-harness suite passed **127 tests**, failed zero,
  and ignored five release-only benchmarks. It includes lazy startup, token
  expiry/singleflight/invalidation, mock `308` and interruption, late
  cancellation, cleanup retry, successive fair handoff, rejected over-capacity
  registration, real HTTP overlap, durable provider success, channel isolation,
  playlist failure, relaunch, and post-process permit-release regressions.
- The loopback overlap fixture holds received HTTP bodies at a provider-side
  barrier. Observed maximum concurrency was exactly two and four respectively,
  while each of two source volumes remained at or below limits one and two.
- `cargo check --manifest-path src-tauri/Cargo.toml` passed on the integrated
  tree. `npm run performance:native:check -- --configuration-only` passed with
  four maximum per-volume uploads, one startup-resident worker, and one direct
  thread-spawn wrapper.
- The final integrated release suite passed all **5/5** benchmarks. In the
  64 MiB loopback upload fixture, seven samples of eight 8 MiB chunks, optimized
  request-owned transport measured p50 **204.516 ms**, p95 **239.985 ms**, and
  p50 **312.933 MiB/s**, using one 8 MiB application buffer per worker and zero
  extra full-chunk copies. The pooled streaming reference measured p50
  **417.810 ms** and p95 **447.743 ms**; optimized throughput was **2.0429x
  (204.29%)** of reference against the executable 0.90x budget. An earlier quiet
  window also passed at 1.6347x and remains corroborating, not canonical,
  evidence.
- Evidence is local source, SQLite, and loopback-provider certification. No
  Google OAuth or live YouTube upload timing was exercised or inferred.

## Follow-ups

- TASK112 owns standard/signed packaged transfer measurements and an explicitly approved
  non-production Google/YouTube canary. The local benchmark does not establish
  internet, provider-processing, quota, or production-channel throughput.
