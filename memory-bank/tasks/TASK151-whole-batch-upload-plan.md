# TASK151 — Whole-batch upload plan

**Status:** completed

**Owner:** Codex

**Parent:** [TASK148](TASK148-upload-tool-research-and-feature-roadmap.md), 148-C

**Dependencies:** TASK143, TASK146, TASK147

## Objective and acceptance

Implement all acceptance criteria of 148-C in TASK148, retaining its scope and
the existing channel, source-integrity, protected-credential and recovery rules.
Use stepped review modals for consequential actions and preserve unrelated work.
The parent goal covers every TASK148 feature; completing this leaf does not
complete that goal.

## Ownership and evidence

Updated 2026-10-13. Source: `upload_plan.rs; upload_plan_execution.rs; UploadPlanDialog and bridge`.

Offline bounded source/metadata/duplicate review and redacted export are read-only. Online owner reads are explicit. Execution rechecks original handles and evidence, claims the plan once, and returns the saved receipt on repeat. Metadata, exact publication context and prospective attachment summaries resolve into one immutable preview. Sealing creates a new native plan ID. Approved attachment proofs persist in the existing execution-claim field, with exact ordinal-to-item/job mappings committed in the same video transaction. Full attachment hashes and held handles are verified before SQLite writes; commit only rechecks identity/signature. Failed intake retains recovery evidence; no new schema statement was added.

Local evidence: 9 native planning and 2 durable claim cases, plus 19 attachment cases covering normal prospective intake, audit rollback, changed/missing/moved assets, exact ordinals, scope and replay. Twenty-seven plan frontend tests and 32 advanced browser checks passed.

Screenshot: `C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/plan-advanced-20261013/review-dark-desktop.png`.

Final local verification on 2026-10-13: 417 native tests passed, zero failed, six explicitly ignored (five release benchmarks and the opt-in decoder fixture). The real generated-video decoder fixture was invoked separately and passed in 15.57 seconds. All 480 frontend tests across 64 files, strict TypeScript, production native check, native formatting, Tailwind, whitespace and unchanged initial bundle budgets passed. Operation/packaging-script fixtures: 40 passed, one Unix-only check skipped on Windows. Merged UI: 189 browser assertions, zero console errors/warnings. These are source and controlled-fixture results; live Google/YouTube, real OS tray/notice/login behavior, packaged playback and other-platform runtime remain unverified. No personal credentials or media were used.

## Rollout and rollback

New choices are explicit and existing approved queue metadata stays intact.
Any migration is transactional and preserves queue, originals and checkpoints;
older-binary rollback requires a pre-migration database backup. No personal
profile or credential mutations are authorized as a substitute for fixtures.
