# TASK108: Provider transport and upload scheduler

## Status

proposed

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
