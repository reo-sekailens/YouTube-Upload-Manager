# TASK097: Automatic queue dispatch

## Status

completed

## Scope

Remove the global **Start uploads** action and make saving a reviewed item to
the queue the final operator action before native automatic dispatch.

## Acceptance criteria

- The global upload-start button and webview command are removed.
- Manual, batch, and duplicate-review queue decisions invoke the native
  capacity-aware scheduler automatically.
- Existing startup, watched-folder, quota-resume, and worker-handoff dispatch
  paths remain automatic.
- UI messaging explains that queued work starts when capacity is available.

## Evidence

- `queue_item` persists the channel-bound queue transition, then invokes
  `start_queued_uploads_impl`; manual and duplicate-review flows use that one
  command.
- The browser-facing `start_queued_uploads` command and header button were
  removed, so users cannot needlessly intervene after a queue decision.
- The existing capacity scheduler limits automatic work safely and is also
  invoked from startup, watched-folder scanning, quota resume, and worker
  completion.
