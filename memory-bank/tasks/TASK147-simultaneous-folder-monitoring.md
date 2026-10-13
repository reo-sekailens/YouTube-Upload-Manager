# TASK147 — Simultaneous folder monitoring

**Status:** completed

**Dependencies:** TASK071, TASK074, TASK082, TASK142, TASK146

## Objective

Keep multiple approved folders monitored at once, with independent choices,
status and controls while sharing the existing bounded upload scheduler.

## Acceptance criteria

- Adding a folder uses Folder → Upload choices → Review recurring approval and
  preserves every previously configured folder. Each has a stable native ID and
  immutable channel binding, visibility, audience, playlist and cleanup choice.
- All enabled folders for the connected channel remain monitored independent of
  the selected UI folder. Failure in one scan does not stop discovery in another.
- Preserve TASK074 automatic intake of existing and newly added supported
  direct-child files after two unchanged scans; do not restore a starting baseline.
- Pause/resume/remove apply to one folder. Removal retains queue, originals,
  observation/history and resumable evidence. Duplicate physical roots are rejected.
- Activity, baseline processing, cancelled retries, reconciliation, source cleanup
  and progress are scoped to the correct monitor and channel. No-copy source and
  existing final byte-integrity/destructive guards remain authoritative.
- Migration retains the old folder configuration and associates existing history
  without reintake/reupload. Restart restores all approved enabled monitors.
- Local normal two-folder discovery, independent mutation/failure, migration and
  stale-scope cases plus responsive UI screenshots are verified. Live/provider/
  packaged runtime evidence remains separately bounded.

## Evidence

- Existing TASK143–146 changes remain intact.
- 214 frontend tests passed, including four scoped bridge cases and 16 folder
  card/setup cases. Strict TypeScript, production build, Tailwind ownership and
  whitespace checks pass. Startup budgets remain unchanged and pass at
  223.19 KiB raw JS, 69.88 KiB gzip JS and 39.22 KiB raw CSS.
- Chrome/Playwright CLI controlled bridge fixtures passed 26 interaction checks:
  normal two-folder setup, recurring approval, Back choice retention, independent
  pause/resume/edit/remove, scoped scan/retry/reconciliation/single and bulk
  cleanup, cross-monitor progress rejection, collection retry, Escape/focus,
  duplicate-root failure and late cross-channel response suppression.
  StrictMode and desktop/mobile light/dark screenshots are verified; the mobile
  final approval action is reachable and has contrast 5.28:1. The browser plugin
  was unavailable. Scripts/screenshots/receipts remain external in
  `multi-folder-20261013`; no real uploads or source deletions occurred.
- 218 integrated native tests passed, zero failed and five release benchmarks
  ignored. Eleven focused multi-folder cases and one new schema 6→7 migration
  case cover normal two-folder discovery/restart without copies, failure
  isolation, independent mutation, frozen queue choices, stale channel/playlist
  resolution, scoped retries/reconciliation/cleanup, delayed scan/removal,
  retained-history re-add, paused legacy-path resume and worker restart handoff.
  Offline/locked native check, formatting and whitespace pass. The frozen source
  hashes match both the independent review and final validation receipt.
- Independent review identified and verified fixes for remove/re-add reintake,
  legacy raw-path resume and worker shutdown/enable races. Same-channel roots
  reuse their archived ID/observations; exact-key lookup precedes bounded alias
  checks and truncated misses fail closed. Resume retains original locators,
  worker shutdown rechecks concurrent admission, and pause/remove run outside
  the GUI thread. No remaining concrete review findings.

## Remaining verification gates

- Packaged/native folder watching, actual provider upload/playlist reads and
  non-Windows runtime remain unverified by the controlled local fixtures.
- Schema 7 requires a pre-migration database backup for rollback to an older
  binary. No personal profile migration was performed during verification.
