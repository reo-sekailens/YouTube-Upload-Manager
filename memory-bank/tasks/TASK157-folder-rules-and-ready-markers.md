# TASK157 — Folder rules and ready markers

**Status:** completed

**Owner:** Codex

**Parent:** [TASK148](TASK148-upload-tool-research-and-feature-roadmap.md), 148-I

**Dependencies:** TASK147, TASK151

## Objective and acceptance

Implement all acceptance criteria of 148-I in TASK148, retaining its scope and
the existing channel, source-integrity, protected-credential and recovery rules.
Use stepped review modals for consequential actions and preserve unrelated work.
The parent goal covers every TASK148 feature; completing this leaf does not
complete that goal.

## Ownership and evidence

Updated 2026-10-13. Source: `folder_rules.rs; FolderRulesDialog and monitor discovery integration`.

Reviewed filename include/exclude patterns and optional empty fresh ready markers gate direct-child discovery before stability observations. They cannot choose another source or override upload choices. Previously excluded originals require explicit admission; remove/re-add retains monitoring history and queued identities.

Local evidence: 8 native folder-rule cases; 20 frontend cases plus controlled UI checks.

Final local verification on 2026-10-13: 417 native tests passed, zero failed, six explicitly ignored (five release benchmarks and the opt-in decoder fixture). The real generated-video decoder fixture was invoked separately and passed in 15.57 seconds. All 480 frontend tests across 64 files, strict TypeScript, production native check, native formatting, Tailwind, whitespace and unchanged initial bundle budgets passed. Operation/packaging-script fixtures: 40 passed, one Unix-only check skipped on Windows. Merged UI: 189 browser assertions, zero console errors/warnings. These are source and controlled-fixture results; live Google/YouTube, real OS tray/notice/login behavior, packaged playback and other-platform runtime remain unverified. No personal credentials or media were used.

## Rollout and rollback

New choices are explicit and existing approved queue metadata stays intact.
Any migration is transactional and preserves queue, originals and checkpoints;
older-binary rollback requires a pre-migration database backup. No personal
profile or credential mutations are authorized as a substitute for fixtures.
