# TASK039 — README and GitHub Wiki

## Status

blocked

## Completed locally

- Expanded README coverage for first-open OAuth setup, multi-file intake,
  managed local uploads, watched folders, light/deep pre-ingest matching,
  duplicate comparison, deletion safeguards, cancellation, crash recovery,
  background workers, export/import, and the Windows installer command.
- Added a GitHub Wiki link and a concise boundary between local verification and
  live YouTube testing.

## Blocker

- GitHub CLI authentication for `sekailens` is invalid (`HTTP 401: Bad
  credentials`) and the Wiki Git endpoint cannot be accessed. Reauthenticate
  with `gh auth refresh -h github.com`, then create/enable the repository Wiki
  and publish the prepared operational pages.
