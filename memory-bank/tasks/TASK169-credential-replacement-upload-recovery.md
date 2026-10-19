# TASK169 — Credential replacement and resumable upload recovery

**Status:** completed
**Scope:** local source validation; installed/live follow-ups are listed below
**Owner:** Codex
**Date:** 2026-10-19, Asia/Singapore
**Dependencies:** TASK145, TASK166, TASK167

## Objective

Restore app-wide provider state after same-channel OAuth replacement and prevent
interrupted resumable sessions from monopolizing upload dispatch.

## Findings

- Read-only installed-state inspection confirms successful same-channel replacement,
  no active-project API pause and no channel pause. Old-project pauses remain.
- Playlist errors and queued quota descriptions can outlive the replaced project.
- Native status and chunk PUTs omit the bearer authorization required by the
  official resumable protocol. Session-check errors immediately re-enter the
  priority recovery lane; a sampled live error is the fixed session reconciliation
  failure. Repeated generic interruption audits obscure the provider stage.

## Acceptance

- [x] Credential revision invalidates provider reads/errors across app surfaces;
      late results cannot restore the previous credential scope, and routine
      polling preserves drafts and controls.
- [x] Status/chunk PUTs carry native-only authorization with no redirect leakage.
- [x] Resumable status distinguishes confirmed progress, completed result,
      terminal refusal, transient retry and exact quota errors.
- [x] Durable item/channel/credential-scoped retry deadlines prevent recovery
      spinning and allow other reviewed uploads to dispatch.
- [x] Recovery and ordinary queue selection cap accepted candidates after
      eligibility/source checks; 256 older held rows cannot hide later work.
- [x] Expired/ambiguous uploads retain original references, queue identities and
      protected checkpoints; no blind new upload or extra media copy is created.
- [x] Old quota text refreshes from authoritative native pause state, preserving
      same/unknown-project and independent channel holds.
- [x] Focused native/frontend regressions, production checks and rendered
      desktop/mobile evidence pass. Record provider/package limits explicitly.

## Evidence and limits

Personal queue, credentials and media have been inspected read-only only. No
personal provider operation or real upload has been initiated for validation.
The full C: drive interrupted two source writes; both files were restored before
implementation resumed. Regenerable release dependency cache was preserved on D:
to recover disk space; subsequent native build output uses D:.
The missing F: build links were preserved under ignored `src-tauri/target/`
(`binaries-unavailable-f`, `dist-unavailable-f`). Local `src-tauri/binaries` and
`dist` now point at staged tools/frontend output under
`D:/CodexBuildCache/YouTube-Upload-Manager-20261019/`; both pinned sidecar
preparers pass. Rust/Cargo use the valid D: toolchain/cache through process-local
environment settings, leaving the broken global junctions unchanged. No tracked
lockfile, generated source or deployment configuration changed.

Focused verification passed: 16 upload-session/status/admission regressions, nine
durable-recovery tests, 28 replacement tests and 12 transport tests. Provider
requests use loopback fixtures only. Locked/offline non-test native compilation
and all four exact native performance gates pass. Native configuration remains
within the existing worker/upload budgets; five budget-runner unit tests pass.
The first full native run passed 538 tests with nine opt-in cases ignored and
found two test-fixture failures: a minimal connection table lacked the new
metadata columns, and 100 inventory reads outlasted their real 60-second wait
on the busy disk. The fixture now matches the projection; the inventory test
checks the generated 60-second deadline before retaining a longer test-only
wait. Its adjacent explicit-clock expiry test remains unchanged. Both repaired
tests pass individually; no production inventory timing or admission was relaxed.
Final review also found a limit-before-filter edge case in both selection paths.
Those paths now stream ordered rows and cap accepted candidates, with focused
held-prefix regressions passing: four queued-selection cases and one recovery
case included in the 16 session tests. The final full native suite passes 545
tests with nine opt-in cases ignored. Locked/offline non-test compilation and
all four exact native performance gates pass again for the final source.
The final frontend suite passes 591 tests across 76 files. Production frontend
build, strict TypeScript, Tailwind guard and unchanged bundle budgets pass:
223.01 KiB initial JavaScript raw / 69.95 KiB gzip / 39.40 KiB CSS. Existing
library/Dedupe actions load on explicit use to retain the 70 KiB gzip gate.

Actual component QA passes 22 desktop/mobile checks at 1280 and 390 pixels:
same-channel replacement clears old errors, late old responses are ignored,
ordinary polling adds no reads, and reviewed rename drafts remain intact. No
console warnings/errors, page exceptions or horizontal overflow were observed.
These screenshots use synthetic data and local mocks, not the installed app:

- `C:/Users/Workstation/.codex/visualizations/2026/10/19/01a151c7-f234-7221-81f3-f7f444ad4c2b/task169/desktop-after.png`
- `C:/Users/Workstation/.codex/visualizations/2026/10/19/01a151c7-f234-7221-81f3-f7f444ad4c2b/task169/mobile-after.png`

The same evidence folder contains frontend/rendered receipts with source and
fixture hashes. Native test and performance outputs are retained there too.

## Follow-ups outside local source validation

- Publish this patch through the existing GitHub nightly workflow, as authorized
  by the operator on 2026-10-19. Installation remains a separate step.
  The running nightly.17 executable was not stopped, updated or replaced.
- Verify authorized live-provider session reconciliation and actual queued
  upload acceptance in the updated installed app. No claim of resumed personal
  uploads follows from mock HTTP or source/compilation checks.
- Historical completed-but-unconfirmed rows without their original returned
  video ID/checkpoint still require independent provider reconciliation. Keep
  their original identities and media; never infer a safe duplicate upload
  from a matching title or a newly available project quota.

## Publication evidence

The operator requested commit and deployment after local validation. The tested
source is published through `main` and `.github/workflows/nightly-prerelease.yml`,
using its existing version allocation and platform matrix. No release workflow,
version metadata or lockfile change is required for this fix.

The hosted run, exact source commit, published tag/assets and downloaded Windows
artifact verification are recorded separately in the thread evidence folder at
`C:/Users/Workstation/.codex/visualizations/2026/10/19/01a151c7-f234-7221-81f3-f7f444ad4c2b/task169/release-receipt.json`.
That receipt records publication status; it does not certify installation or
live YouTube upload acceptance.
