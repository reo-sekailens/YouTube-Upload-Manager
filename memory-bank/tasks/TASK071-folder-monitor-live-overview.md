# TASK071: Folder-monitor live overview

## Status

completed

## Scope

Expose a bounded, folder-scoped live view of watched-file activity, queued and
uploading items, and safe scan/audit history while keeping automatic polling
native and retaining a manual refresh scan action.

## Acceptance criteria

- The watched folder shows currently uploading/dispatching files and its queue.
- The panel renders a bounded per-folder observation list and collapsed safe
  scan log.
- Monitoring continues automatically while enabled; the manual button is named
  **Refresh scan**.
- No source paths, OAuth data, or provider payloads reach the webview.

## Evidence

- Native `load_folder_monitor_overview` returns at most 200 channel-scoped
  observations and 200 folder-monitor audit entries, with each observation
  reduced to a filename before it crosses into the webview.
- The folder panel polls that native overview every five seconds while running,
  displays active and queued watched-folder files, and keeps scan logs and the
  broader observed-file list collapsed until opened.
- `Refresh scan` still explicitly starts a background scan. The native polling
  loop records a safe error state if an automatic scan fails, then continues
  polling rather than stopping permanently.
- A YouTube inventory-refresh failure after a stable file is claimed now keeps
  that file in the retryable `observed` state, records a safe
  `folder_monitor_inventory_sync_failed` audit event, and shows a bounded,
  actionable failure category in the monitor detail and GitHub-ready report.
  It cannot leave the file stuck in `processing` or bypass the fresh-inventory
  duplicate gate on a later retry.
- All local inventory-refresh callers now share a lock before using the common
  SQLite staging table. A new refresh clears stale rows from an interrupted
  prior run, preventing overlapping scans from repeatedly failing while
  replacing the saved YouTube library.
- Live diagnosis on 2026-08-23 found the running installed executable predated
  the repaired package: the installed binary was written at 02:19, while the
  repaired release binary and NSIS installer were built at 02:20. The local
  database held one stranded 495-row staging run and zero saved inventory rows,
  which is the expected old-binary failure signature. The repaired installer
  must be installed and the app restarted before provider-path verification.
- Follow-up live diagnosis confirmed the repaired binary fetched all 495
  records but failed only while committing their atomic local replacement. The
  same transaction succeeds against a disposable copy of the database, which
  isolates a rollback-journal reader/writer collision rather than YouTube,
  OAuth, or a schema constraint. Database opens now enable WAL and a 30-second
  busy wait; the replacement uses an immediate transaction and manual-refresh
  failures leave a safe `youtube_inventory_sync_failed` diagnostic receipt.
- A second live attempt confirmed WAL staging succeeded but the original
  connection still failed during its final transaction. The replacement now
  explicitly reopens a fresh database connection after staging, before the
  short channel-scoped atomic replacement. It leaves other channel snapshots
  untouched and gives a safe stage-specific diagnostic if the commit fails.
- Live diagnostics then identified duplicated video IDs in the YouTube uploads
  playlist. Staging now uses an idempotent `(sync_id, video_id)` upsert, so a
  repeated provider entry refreshes its metadata rather than aborting the
  entire library save; displayed counts are derived from the deduplicated
  staged rows.
- Local validation: focused native overview test, focused frontend command test,
  TypeScript check, and whitespace diff check passed. Visual browser preview
  remains limited to its safe disabled-mode state; a signed desktop session is
  required to exercise populated activity rows.
- Automatic folder dispatch now claims only the files for which the scheduler
  has capacity. Remaining observations stay honestly queued rather than being
  prematurely marked `dispatching`; each worker completion or reconciliation
  immediately schedules the next eligible item.
