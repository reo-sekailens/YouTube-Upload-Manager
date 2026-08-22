# TASK105: Two-phase fast startup and recovery

## Status

proposed

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
