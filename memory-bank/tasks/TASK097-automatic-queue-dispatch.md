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
- Recovered items that were queued while disconnected are re-kicked after a
  successful OAuth connection and after a successful manual library refresh.
  This closes the gap where startup had already run before the user connected.
- The recovery-dispatch repair was packaged as unsigned x64 installers on
  2026-08-23:
  - NSIS `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,481,246 bytes;
    SHA-256 `3051B3A2E7A580908B5D49C14E16DAF1A0BFB37116342C3A5E06A116B480C1E4`.
  - MSI `YouTube Upload Manager_0.1.9_x64_en-US.msi` — 36,327,424 bytes;
    SHA-256 `AAD39651352E57028A452B7C54DC4E6A01CCDF45EB48544638036C6373BC4287`.
