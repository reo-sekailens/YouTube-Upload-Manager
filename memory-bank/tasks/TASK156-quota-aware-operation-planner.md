# TASK156 — Quota-aware operation planner

**Status:** completed

**Owner:** Codex

**Parent:** [TASK148](TASK148-upload-tool-research-and-feature-roadmap.md), 148-H

**Dependencies:** TASK145, TASK151

## Objective and acceptance

Implement all acceptance criteria of 148-H in TASK148, retaining its scope and
the existing channel, source-integrity, protected-credential and recovery rules.
Use stepped review modals for consequential actions and preserve unrelated work.
The parent goal covers every TASK148 feature; completing this leaf does not
complete that goal.

## Ownership and evidence

Updated 2026-10-13. Source: `quota_planner.rs; observed provider adapters; QuotaPlannerDialog`.

Dated method/bucket estimates distinguish app/device observations from unknown actual project allowance. Usage aggregates across local profiles with the same established project, preserving separate channel holds. Captured request scope cannot change after a response. Sorting/deletion/rename attempts capture observations and exact quota holds. Inventory/playlist read chains serialize selection changes and conditional inventory commits cannot reselect stale accounts. OAuth callbacks recheck attempt identity/kind/expiry and serialize cancellation with final promotion. No quota bypass or automatic credential cycling.

Local evidence: 9 native planner cases, canonical scoped quota-hold fixtures, 5 quota/account-chain cases, 4 OAuth cancellation/promotion cases and 12 frontend aggregation cases passed.

Final local verification on 2026-10-13: 417 native tests passed, zero failed, six explicitly ignored (five release benchmarks and the opt-in decoder fixture). The real generated-video decoder fixture was invoked separately and passed in 15.57 seconds. All 480 frontend tests across 64 files, strict TypeScript, production native check, native formatting, Tailwind, whitespace and unchanged initial bundle budgets passed. Operation/packaging-script fixtures: 40 passed, one Unix-only check skipped on Windows. Merged UI: 189 browser assertions, zero console errors/warnings. These are source and controlled-fixture results; live Google/YouTube, real OS tray/notice/login behavior, packaged playback and other-platform runtime remain unverified. No personal credentials or media were used.

## Rollout and rollback

Authorization-time channel verification is intentionally excluded from the
current selected-profile observer when it uses staged candidate credentials.
Assigning those attempts to the active account/project would be false. Future
instrumentation needs an explicit opaque authorization-review owner and captured
candidate project/client scope, including failed reads before channel identity
is known. The planner's incomplete device-local estimate warning remains; its
numbers never establish remaining Google allowance. No candidate error can
write a hold into the currently selected unrelated project.

New choices are explicit and existing approved queue metadata stays intact.
Any migration is transactional and preserves queue, originals and checkpoints;
older-binary rollback requires a pre-migration database backup. No personal
profile or credential mutations are authorized as a substitute for fixtures.
