# TASK166 — Keep approved watched uploads dispatching

**Status:** completed
**Owner:** Codex
**Date:** 2026-10-19, Asia/Singapore
**Dependencies:** TASK097, TASK108, TASK147

## Request

Watched-folder activity shows zero uploading and two queued files indefinitely.
Approved eligible work should start automatically and continue handing off while
the app runs. Real quota/account/transfer/source/duplicate holds remain enforced.

## Acceptance and ownership

- [x] T166-A (root): diagnose and fix native dispatch liveness with a failing
  regression and passing normal-flow evidence. Preserve immutable channel,
  idempotent claim, protected checkpoint and original-source boundaries.
- [x] T166-B (queue_hold_ui): show existing safe native waiting detail in folder
  queue rows; preserve mounted refresh state and cover the rendered behavior.
- [x] T166-C (root): integrate narrow/native and rendered checks, update source
  docs/evidence, and commit/push the authorized change to main.

Native scheduler audit is read-only; root owns native edits and docs. The UI
packet owns FolderMonitorPanel and its tests only. Read-only local diagnosis may
inspect status/hold metadata but never tokens, protected sessions, browser
cookies or media. No test should dispatch personal queued uploads to YouTube.

## Evidence

The personal queue was inspected read-only with SQLite query-only mode. The two
large watched uploads were genuinely held by a Google Cloud API quota record;
no transfer window/bandwidth hold was configured. The old rolling deadline was
2026-10-19 22:09 UTC, although the next Pacific reset from the saved observation
was 2026-10-19 07:00 UTC (19 October 15:00 Asia/Singapore).

Both Cloud pause recorders now use the shared Pacific/DST boundary. Housekeeping
transactionally shortens legacy oversized deadlines from their saved observation,
preserves that timestamp, and records a scoped audit event. A separate channel
upload hold remains rolling/conservative. Queue admission performs housekeeping
before checking holds; quota-resume scheduling reuses retained queue identities.

The original failure was reproduced by a native deadline regression. Ten focused
Cloud/reset tests passed, including normal eligible claim after reset, mixed
channel hold retention, DST/season boundaries, legacy idempotence and audit rollback.
The integrated suites and final release receipt are recorded below when complete.

The watched queue shows each safe native waiting reason with wrapping and no
blank label. Eight panel and eight refresh tests pass. An actual-component
Playwright fixture passed 12 desktop/mobile checks, including preserved focus/row
identity through 40 refresh events and a synthetic queued-to-uploading transition.
Screenshots were inspected under `task166-queue-dispatch-20261019` in the thread
artifact folder: `queue-hold-desktop.png`, `queue-hold-mobile.png`, and
`queue-resumed-desktop.png`. Browser plugin unavailable; cached Playwright CLI used.

No personal media, tokens or protected sessions were read, no personal queue was
mutated, and no real upload or installed-app acceptance is claimed.

Final integrated suite: **512 native tests pass** (nine opt-in ignored),
**576 frontend tests pass**, production frontend build/TypeScript, formatting,
Tailwind and unchanged frontend/native configuration budgets pass. The shared
request reduction and live quota work is tracked in TASK167–168. Production
native compilation passes (locked/offline, production features). The complete
reviewed patch is committed to main and published through the existing GitHub
workflow; hosted packaging is separate from the local acceptance evidence.
