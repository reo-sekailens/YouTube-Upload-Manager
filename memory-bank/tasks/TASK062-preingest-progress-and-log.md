# TASK062: Pre-ingest progress and operation log

## Status

completed

## Scope

Show truthful, persistent pre-ingest progress for picker and desktop drops:
completed/total, the current filename, and a collapsed, one-click-copyable log.

## Evidence

- Native scan rows now persist an active `running` state and append safe
  filename-only activity events. The returned scan includes current filename,
  checkpoint counts, pending metadata count, and the bounded event log.
- The progress bar is backed by persisted `completedFiles`, not elapsed-time
  estimates. The UI continues polling after matching while background FFprobe
  metadata remains pending.
- The log is collapsed by default and uses a one-click clipboard action.
- Rust format, 40 native tests, TypeScript check, production build, and diff
  check passed. Browser UI smoke test showed the duplicate-review surface with
  no console warnings; native-only picker/drop state requires the signed app.
