# TASK161 — Single and matching upload retries

**Status:** completed

**Owner:** Codex

**Dependencies:** TASK142, TASK145, TASK148

## Request

The upload queue shows 0% and “YouTube rejected the upload setup request.”
Provide a visible retry beside that failure and a way to retry all saved items
with the same issue. Use stepped review modals and parallel independent packets.
The later placement instruction requires exactly one bulk control below the
filters and above the table; never repeat that control on individual rows.

## Acceptance

- [x] Show Retry upload beside a failed transfer error and one bulk control
  below filters and above the table when multiple saved items share a failure.
- [x] Choose → Review → Results stages bind immutable item/channel IDs and a
  native review snapshot. New or changed failures require another review.
- [x] Native code groups by sanitized stage/status/provider reason; generic
  historical setup failures have a conservative exact-message fallback.
- [x] Verify the unchanged original before retry, preserve queue identity,
  channel, metadata, confirmed bytes and protected resumable checkpoints, and
  reconcile uncertain outcomes rather than issuing another upload setup.
- [x] Keep project/channel quota holds, duplicate-review gates and channel
  fences. Batch retries report partial success and precise retained-item reasons.
- [x] Normal single/bulk success, unrelated/foreign failures, stale scope,
  missing/changed originals, uncertain sessions and restart fixtures pass.
- [x] Provide responsive light/dark screenshots and record local checks apart
  from live provider/package verification.

## Ownership and evidence

Native upload_retry module; QueueTable and lazy UploadRetryDialog/bridge;
read-only provider diagnosis; root shared registration, persistence, App and
verification. Existing TASK148 work remains authorized and in progress.

Local results on 2026-10-13: 12 native retry cases passed in the integrated suite
(395 passed, zero failed, six ignored), 26 focused frontend cases passed, and
73 controlled browser assertions passed without console errors or warnings.
Tests include the queue/dashboard/live-event projection of sanitized failure
keys and clearing that projection before later transfer failures. Native
original-file verification, checkpoint preservation, restart, partial bulk
receipts and foreign-channel rejection are covered. No personal uploads,
credentials or source media are used as fixtures. Retrying never creates a new
local upload record.

Bulk groups come from the current matching table view; review explicitly states
the saved whole-group count. A group over 200 items rejects without queueing or
silently truncating and explains the individual retry option. Multiple groups
use one issue selector in the global strip. Review snapshots expire after 15
minutes; later failures need a new review. Secure-store errors fail closed.

Historical exact setup failures at zero confirmed bytes are eligible only when
there is no known video or protected session. New definitive setup rejections
are distinguished from transport errors and uncertain transfers. The screenshot
does not establish the provider's specific rejection reason; no speculative
metadata/category fix was applied.

Evidence directory:
`C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/task161-retry-20261013/`.
`qa-evidence.json` records browser cases. Final screenshots include
`retry-queue-dark-desktop.png`, `retry-queue-dark-mobile.png`,
`retry-review-dark-desktop.png` and `retry-approval-dark-mobile.png`.
Browser plugin unavailable; cached Playwright CLI used. These results establish
local source/fixture behavior; live YouTube retry and packaged playback remain
unverified. Final shared production checks remain in TASK148.

## Recovery and rollback

Native reviews expire and cannot approve a different channel or a later failure.
Failures remain attached to existing jobs. Schema changes are transactional and
retain original locators, signatures, digests and encrypted checkpoint evidence;
older-binary rollback requires a pre-migration database backup.

## Final shared handoff

Final local verification on 2026-10-13: 417 native tests passed, zero failed, six explicitly ignored (five release benchmarks and the opt-in decoder fixture). The real generated-video decoder fixture was invoked separately and passed in 15.57 seconds. All 480 frontend tests across 64 files, strict TypeScript, production native check, native formatting, Tailwind, whitespace and unchanged initial bundle budgets passed. Operation/packaging-script fixtures: 40 passed, one Unix-only check skipped on Windows. Merged UI: 189 browser assertions, zero console errors/warnings. These are source and controlled-fixture results; live Google/YouTube, real OS tray/notice/login behavior, packaged playback and other-platform runtime remain unverified. No personal credentials or media were used.

The single global strip placement and individual row action passed the 189 merged UI cases as well as 73 retry-specific cases. Native retry bounds remain 200 matching items, with no silent truncation.
