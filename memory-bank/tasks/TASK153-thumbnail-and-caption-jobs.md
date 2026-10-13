# TASK153 — Thumbnail and caption jobs

**Status:** completed

**Owner:** Codex

**Parent:** [TASK148](TASK148-upload-tool-research-and-feature-roadmap.md), 148-E

**Dependencies:** TASK150, TASK152

## Objective and acceptance

Implement all acceptance criteria of 148-E in TASK148, retaining its scope and
the existing channel, source-integrity, protected-credential and recovery rules.
Use stepped review modals for consequential actions and preserve unrelated work.
The parent goal covers every TASK148 feature; completing this leaf does not
complete that goal.

## Ownership and evidence

Updated 2026-10-13. Source: `video_assets.rs; VideoAssetsDialog and bridge`.

Choose → Match → Review validates original thumbnail/caption files without copying video. Explicit caption language/name/draft declarations and native ownership checks produce separate durable jobs. Exact job IDs and unknown outcomes require reconciliation, never blind writes or another video upload. Prospective plan integration is complete: no attachment jobs exist during preview, and accepted jobs materialize atomically with their exact native intake ordinals.

Local evidence: 19 native attachment cases passed; 18 current bridge/dialog tests, 27 prospective browser assertions and saved-mode recovery/regression checks passed with responsive screenshots.

Screenshot: `C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/video-assets-20261013/assets-review-dark-desktop.png`.

Final local verification on 2026-10-13: 417 native tests passed, zero failed, six explicitly ignored (five release benchmarks and the opt-in decoder fixture). The real generated-video decoder fixture was invoked separately and passed in 15.57 seconds. All 480 frontend tests across 64 files, strict TypeScript, production native check, native formatting, Tailwind, whitespace and unchanged initial bundle budgets passed. Operation/packaging-script fixtures: 40 passed, one Unix-only check skipped on Windows. Merged UI: 189 browser assertions, zero console errors/warnings. These are source and controlled-fixture results; live Google/YouTube, real OS tray/notice/login behavior, packaged playback and other-platform runtime remain unverified. No personal credentials or media were used.

## Rollout and rollback

New choices are explicit and existing approved queue metadata stays intact.
Any migration is transactional and preserves queue, originals and checkpoints;
older-binary rollback requires a pre-migration database backup. No personal
profile or credential mutations are authorized as a substitute for fixtures.
