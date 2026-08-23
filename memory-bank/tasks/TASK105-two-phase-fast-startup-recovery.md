# TASK105: Two-phase fast startup and recovery

## Status

completed

## Objective

Render a safe, useful shell quickly while preserving automatic crash recovery
and preventing any item from dispatching before its durable classification is
safe.

## Scope

- Limit synchronous setup to directories, one database bootstrap, crash marker,
  and a transactional classification of interrupted records.
- Move full-media hashing, partial-copy continuation, source cleanup, metadata
  enrichment, and other large I/O into bounded recovery workers after the shell
  can render.
- Combine crash status, connection summary, queue readiness, active channel,
  and initial Batch data into one bootstrap envelope.
- Start folder/quota/provider workers only after classification is committed and
  only when their persisted state requires them.
- Gate actionable queue controls by explicit readiness; recovery mode must not
  briefly mount normal workspaces.
- Bound recovered hash, copy, metadata, FFprobe, and provider-independent jobs
  by resource class instead of one thread/process per persisted row.

## Acceptance criteria

- No full media read, FFprobe process, or provider request occurs before first
  safe-shell render.
- Empty and large-interrupted profiles meet TASK103 startup budgets.
- No queued item can dispatch before its recovery fence and channel binding are
  confirmed.
- Interrupted imports/uploads/deletions and watched/preflight work still recover
  automatically from SQLite and secure storage.
- The existing 512 KiB stack regression and packaged Windows startup test pass.
- architecture.md and technical-notes.md describe the new two-phase boundary.

## Dependencies

TASK103, TASK104.

## Rollback

Retain a feature-gated synchronous recovery path until packaged recovery
fixtures prove parity.

## Evidence

- Native setup now performs only directory/bootstrap work plus one database-only
  `IMMEDIATE` recovery-classification transaction. It does not read media, run
  FFprobe, contact Google/YouTube, or read protected resumable sessions.
- The first React render is a safe holding shell. After two animation frames,
  one bounded recovery coordinator resolves interrupted imports, sessions,
  watched hashes, and preflight work before the four-part dispatch fence opens.
- Startup returns connection, crash, channel-scoped dashboard, and readiness
  state in one envelope from one SQLite connection. Queue dispatch additionally
  filters by the active immutable channel ID.
- Focused native startup tests passed 6/6. The frozen final-v3 isolated release
  suite passed 140 tests with zero failures and five intentionally ignored,
  including the 512 KiB stack regression.
- TypeScript, the production frontend build, 16 files/87 frontend tests, and
  the frontend payload budget passed. Browser-preview QA proved safe-first and
  crash-first rendering, recovery unlock, one active lazy workspace, and zero
  console warnings/errors.
- The authoritative unsigned final-v3 empty harness recorded the actual shell
  receipt before allowing deferred startup completion. Across 80 cold and 80
  warm runs, safe-shell paint was 3,098/3,806 ms cold and 323/613 ms warm
  (p50/p95); native readiness was 2,588/3,155 ms and 1,060/2,419 ms; first Batch
  paint was 4,628/5,270 ms and 1,652/2,983 ms.
- The first current-source interrupted matrix failed honestly: first-Batch
  p50/p95 was +14.10%/+16.79% versus the bracketing empty mean. Recovery was
  then narrowed to one database connection for interrupted rows, no secure-
  store lookup for deliberately unscoped rows, and a legacy-UUID media pre-scan
  before opening SQLite.
- The authoritative post-repair interrupted final-v3 package recorded all 160
  safe-shell receipts and zero cardinality or idle violations. Cold safe-shell
  p50/p95 changed -2.23%/-9.14% and first-Batch p50/p95 changed +5.12%/+4.08%
  versus final-v3 empty, passing the 10% materiality gate with zero media bytes.
  Signed production and live-provider behavior are not inferred.
