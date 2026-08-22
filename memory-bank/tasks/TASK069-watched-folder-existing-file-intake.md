# TASK069: Watched-folder existing-file intake

## Status

completed

## Scope

Make the watched-folder baseline explain visible existing videos accurately and
let the operator explicitly process those files without weakening the stable
file, duplicate-review, channel-binding, or source-cleanup safeguards.

## Acceptance criteria

- A baseline scan reports the count of existing eligible videos instead of
  saying none were found.
- The operator can explicitly move the current baseline into normal intake.
- Baseline files still pass stability, title/digest duplicate checks, native
  ingestion, resumable queueing, and active-channel validation.
- Files remain baseline-only unless the operator chooses the new action.

## Evidence

- Baseline scans now report the number of eligible existing direct-child videos
  and explain that they await an explicit intake action.
- **Process existing files** promotes only the recorded baseline to the normal
  stable-file workflow, then rechecks the authorized active channel, refreshes
  the YouTube inventory once per scan, performs the existing duplicate checks,
  and dispatches only queued items through the native worker.
- `cargo fmt`, 46 native tests including the existing-baseline regression,
  TypeScript check, diff check, and local rendered UI verification passed.
