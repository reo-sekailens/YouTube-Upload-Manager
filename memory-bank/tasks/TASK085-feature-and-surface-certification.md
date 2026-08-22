# TASK085 — Feature and surface certification matrix

## Status

in-progress

## Objective

Certify every implemented operator-facing feature and supported application
surface against its intended behavior, with evidence that distinguishes unit and
fixture coverage, local native behavior, packaged-platform behavior, and live
Google/YouTube outcomes.

## Scope

Build and execute one maintained certification matrix covering the following
surfaces and workflows:

| Surface or workflow | Required certification focus |
| --- | --- |
| First open, Google setup, and connection | Safe unconfigured guidance; Desktop OAuth JSON validation/import; PKCE callback, connection polling, disconnect, channel scope, and redacted errors. |
| Dashboard and navigation | All cards, loading/empty/error states, modal layers, keyboard access, 390 px responsive layout, and native-only affordances. |
| Manual intake and review | Picker and drag/drop; multiple files; validation; managed copy; private default; audience, visibility, and playlist review; duplicate choices; cancellation; and durable receipts. |
| Upload queue and execution | Automatic dispatch; progress and ETA; pause/error states; cancellation/removal; quota pause; resumable checkpoints; retry reconciliation; and crash/forced-close recovery. |
| Watched-folder monitor | Channel binding; private/unlisted restriction; discovery exclusions; stability and duration gates; scan controls/logs; reference-in-place handling; duplicate withholding; restart behavior; and source cleanup rules. |
| Library, dedupe, and comparison | Account-scoped inventory refresh; title search; activity and phase reporting; exclusions/re-audit; lazy player loading; synchronized controls; and no automatic deletion. |
| Remote and local deletion reviews | Ownership and selection scope; typed confirmations; cancellation; bulk behavior; native worker isolation; post-upload original-file cleanup; and append-only receipts. |
| Transfer, diagnostics, recovery, and exit | Import/export boundaries; redaction; diagnostic/issue handoff; recovery screen; title-bar close; confirmed exit; and retained crash evidence. |
| Security and local-data boundaries | Token and session secrecy; webview/native command boundaries; capability/CSP behavior; account isolation; input validation; persisted-record integrity; and absence of application cloud or telemetry paths. |
| Platform packaging and release | Windows installed-app smoke test and icon; macOS/Linux desktop builds where available; Android/iOS build and device or emulator coverage; accessibility; artifact hashes; and release identity. |

## Acceptance criteria

- A versioned certification matrix maps each row above to its user-visible
  surface, native command or persistence boundary, expected behavior, test data
  or test account, and automated/manual evidence.
- Every implemented feature is marked **certified**, **partially certified**,
  **blocked**, or **not applicable**; no feature is omitted because it lacks a
  test account, device, or provider credential.
- Local checks cover relevant Rust and frontend behavior, TypeScript/build
  checks, and rendered desktop plus narrow-mobile UI inspection. Native-only
  paths are exercised from a packaged app rather than represented as browser
  proof.
- A permitted, operator-approved non-production YouTube account is used for
  live canaries covering OAuth connection, inventory synchronization, a private
  resumable upload and recovery boundary, and separately authorized deletion.
  If unavailable, record the exact blocker and do not call those paths live
  certified.
- Platform results name the exact OS/device or emulator, build/version, and
  artifact hash. Unsupported or unavailable platforms remain explicitly
  uncertified rather than inferred from shared source.
- Failure, interruption, and safety paths are included for durable imports,
  resumable uploads, watched sources, duplicate handling, deletion, and exit.
- Evidence contains no OAuth credentials, tokens, full local paths, or raw
  provider payloads, and documents the distinction between fixture, local,
  packaged, and live-provider verification.
- Any discovered defect becomes a separately indexed task with reproduction
  steps, affected scope, and a retest link. This task is completed only after
  all matrix rows have evidence or an accepted explicit blocker.

## Deliverables

- `memory-bank/certification/feature-surface-matrix.md` with the full matrix,
  evidence links, certification state, and blockers.
- Repeatable narrow commands or documented operator procedures for each row.
- A concise final certification report stating the release/build evaluated and
  every remaining live-provider or platform gap.

## Dependencies

- TASK001, TASK003, TASK004, TASK006, TASK007, TASK008, TASK012, TASK014,
  TASK024, TASK035, TASK037, TASK052, TASK054, TASK055, and all implemented
  feature tasks represented in the matrix.

## Follow-ups

- Obtain explicit operator approval and a safe non-production YouTube test
  channel before live upload or deletion canaries.
- Resolve the pending nightly desktop and Android build work before treating
  release-surface coverage as complete.

## Evidence

- The initial matrix is in
  `memory-bank/certification/feature-surface-matrix.md`.
- Security findings and their individual remediation tasks are in
  `memory-bank/certification/TASK085-security-findings.md` and TASK086–TASK091.
- The historical static scan was sealed with six findings against its original
  2026-08-22 snapshot; those findings map to the completed TASK086–TASK091
  remediations. A fresh current-worktree rescan is running as
  `1430aa6e-45ef-4912-b47f-ae09cf440a0f` and must be sealed separately.
- Local frontend, native, and rendered Browser checks passed at their stated
  scope; 65 Rust tests, 35 frontend tests, and the production frontend build
  pass after the current remediation changes. The matrix records the remaining
  provider, device, package, and formal security-rescan gaps without treating
  them as certified.
