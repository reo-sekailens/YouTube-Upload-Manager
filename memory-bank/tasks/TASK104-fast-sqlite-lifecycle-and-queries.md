# TASK104: Fast SQLite lifecycle and query paths

## Status

proposed

## Objective

Remove schema setup from steady-state database access and make high-frequency,
channel-scoped queries predictable at large local-library sizes.

## Scope

- Replace opportunistic ALTER-on-open behavior with ordered, transactional
  PRAGMA user_version migrations.
- Set persistent database mode during initialization; keep required per-
  connection foreign-key and busy handling without rerunning DDL.
- Measure and select either cheap per-operation connections, a bounded pool, or
  a dedicated writer plus concurrent readers. Do not introduce one long-held
  global connection lock.
- Add EXPLAIN QUERY PLAN-backed composite/partial indexes for channel/status/
  updated-time snapshots, queued claims, recovery, audit, deletion, remote
  inventory, duplicate lookup, and pending jobs.
- Use transactions and prepared/cached statements for repeated inventory,
  progress, and batch writes.
- Keep filesystem, FFprobe, and provider work outside database transactions and
  short coordination locks.

## Acceptance criteria

- A current-schema steady-state open executes zero CREATE, ALTER, table-info
  scan, or journal-mode transition statements.
- Every historical fixture migrates exactly once, transactionally, without
  losing queue, checkpoint, audit, or channel data.
- Query plans use the intended indexes at 10,000 records.
- Dashboard, progress checkpoint, cancellation, monitor overview, and recovery
  queries meet TASK103 p95 budgets.
- WAL, foreign keys, durability, channel isolation, and busy-contention tests
  remain explicit; the current timeout is not reduced merely to hide stalls.

## Dependencies

TASK103.

## Affected paths

src-tauri/src/lib.rs initially; extracted persistence/migrations modules as part
of the implementation; native migration/query tests.
