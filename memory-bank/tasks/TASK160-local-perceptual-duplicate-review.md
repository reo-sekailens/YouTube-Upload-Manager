# TASK160 — Local perceptual duplicate review

**Status:** completed

**Owner:** Codex

**Parent:** [TASK148](TASK148-upload-tool-research-and-feature-roadmap.md), 148-L

**Dependencies:** TASK110, TASK143

## Objective and acceptance

Implement all acceptance criteria of 148-L in TASK148, retaining its scope and
the existing channel, source-integrity, protected-credential and recovery rules.
Use stepped review modals for consequential actions and preserve unrelated work.
The parent goal covers every TASK148 feature; completing this leaf does not
complete that goal.

## Ownership and evidence

Updated 2026-10-13. Source: `perceptual_review.rs; media_runtime.rs; DuplicateMediaPreview; PerceptualReviewDialog; pinned decoder preparation`.

Experimental opt-in selected-local-original analysis uses seven bounded aligned grayscale frame fingerprints, full before/after BLAKE3 identity proof, cancellation and upload-priority admission. Candidates show alignment/distance/coverage in local comparison players; no remote download, automatic suppression/deletion or media copies. Exact pinned binaries and notices are prepared per target.

Local evidence: 11 native fixtures plus the explicitly invoked real decoder test passed; 15 frontend tests and 42 controlled browser checks. Synthetic re-encodes match, shared-intro/distinct fixtures reject; hue shift is a known false-positive limitation. Representative camera accuracy/performance and distribution approval are still required before release.

Screenshot: `C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/perceptual-20261013/comparison-dark-desktop.png`.

Final local verification on 2026-10-13: 417 native tests passed, zero failed, six explicitly ignored (five release benchmarks and the opt-in decoder fixture). The real generated-video decoder fixture was invoked separately and passed in 15.57 seconds. All 480 frontend tests across 64 files, strict TypeScript, production native check, native formatting, Tailwind, whitespace and unchanged initial bundle budgets passed. Operation/packaging-script fixtures: 40 passed, one Unix-only check skipped on Windows. Merged UI: 189 browser assertions, zero console errors/warnings. These are source and controlled-fixture results; live Google/YouTube, real OS tray/notice/login behavior, packaged playback and other-platform runtime remain unverified. No personal credentials or media were used.

## Completion and release boundary

The initial opt-in local source implementation is complete. Distribution remains gated on representative-media accuracy/performance and exact shipped-decoder evidence. Generated fixtures are not camera/general accuracy certification; grayscale hue-shift false positives remain disclosed. No release occurred.

## Rollout and rollback

New choices are explicit and existing approved queue metadata stays intact.
Any migration is transactional and preserves queue, originals and checkpoints;
older-binary rollback requires a pre-migration database backup. No personal
profile or credential mutations are authorized as a substitute for fixtures.
