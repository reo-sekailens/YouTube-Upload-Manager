# TASK158 — Desktop tray and notifications

**Status:** completed

**Owner:** Codex

**Parent:** [TASK148](TASK148-upload-tool-research-and-feature-roadmap.md), 148-J

**Dependencies:** TASK147

## Objective and acceptance

Implement all acceptance criteria of 148-J in TASK148, retaining its scope and
the existing channel, source-integrity, protected-credential and recovery rules.
Use stepped review modals for consequential actions and preserve unrelated work.
The parent goal covers every TASK148 feature; completing this leaf does not
complete that goal.

## Ownership and evidence

Updated 2026-10-13. Source: `desktop_presence.rs; state_events.rs; DesktopPresenceDialog and DesktopRuntimeIntegration`.

Separately opt-in grouped notices, tray close behavior and reversible login startup. Device pause gates new claims/discovery. Opaque scoped activity routes are acknowledged only after their checklist is visible; cross-channel activity offers Account navigation without switching automatically. Rollback clears pending hint flags and quiet receipt updates never recurse.

Local evidence: 13 native presence and 7 state-event cases; 20 runtime frontend tests; 11 controlled runtime browser checks. Real tray/OS notification delivery/login startup remains unverified.

Screenshot: `C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/desktop-runtime-20261013/banner-dark-desktop.png`.

Final local verification on 2026-10-13: 417 native tests passed, zero failed, six explicitly ignored (five release benchmarks and the opt-in decoder fixture). The real generated-video decoder fixture was invoked separately and passed in 15.57 seconds. All 480 frontend tests across 64 files, strict TypeScript, production native check, native formatting, Tailwind, whitespace and unchanged initial bundle budgets passed. Operation/packaging-script fixtures: 40 passed, one Unix-only check skipped on Windows. Merged UI: 189 browser assertions, zero console errors/warnings. These are source and controlled-fixture results; live Google/YouTube, real OS tray/notice/login behavior, packaged playback and other-platform runtime remain unverified. No personal credentials or media were used.

## Rollout and rollback

New choices are explicit and existing approved queue metadata stays intact.
Any migration is transactional and preserves queue, originals and checkpoints;
older-binary rollback requires a pre-migration database backup. No personal
profile or credential mutations are authorized as a substitute for fixtures.
