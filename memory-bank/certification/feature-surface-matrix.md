# Feature and surface certification matrix

**Version assessed:** 0.1.9 working tree, 2026-08-23  
**Overall release status:** not production-certified

Certification states are deliberately separate: **certified** means the
listed evidence covers the stated boundary; **partial** means only a narrower
boundary is proven; **blocked** requires a missing external authority, device,
or unresolved defect.

| Surface | Current state | Evidence | Remaining production gate |
| --- | --- | --- | --- |
| First-open setup and connection UI | partial | Browser smoke: setup dialog rendered, dismissed safely, no console warnings/errors. Rust tests cover installed-client validation, loopback callback validation, and safe OAuth error handling. | Real operator-approved Desktop OAuth client and test channel have not completed a live connect/reconnect/disconnect canary. |
| Dashboard navigation and responsive UI | partial | Browser smoke at desktop and 390 × 844 rendered meaningful content; the installed x64 NSIS application also rendered the batch workspace and explicit native exit confirmation. | Signed release QA plus keyboard/screen-reader coverage across all panels. |
| Manual intake and review | partial | 35 frontend tests and Rust import/preflight tests pass; current queue path now binds reviewed manual work to the active immutable channel. | Signed-app picker/drop, forced-close import, and live private upload canaries. |
| Upload queue, retry, and recovery | partial | 65 Rust tests include interrupted-copy recovery, provider range handling, exclusive claims, quota pause, startup reconciliation, and manual account-switch dispatch isolation. | Real resumable session interruption/reconciliation against YouTube. |
| Watched-folder monitor | partial | Rust tests cover stability, exclusions, inventory failure, background scans, bounded overview, existing-file intake, and final source-signature withholding. | Signed desktop monitor and provider canary. |
| Library, dedupe, and comparison | partial | Rust and frontend tests cover title candidates, exclusions, activity phases, and controls; Browser navigation renders the surface. | Live channel inventory and authenticated comparison-frame canary. |
| Deletion and source cleanup | partial | Rust tests cover typed confirmation, ownership-validation code paths, interrupted request retention, immutable channel scoping, and staged final source deletion. | Separately authorized live deletion canary on a non-production channel plus signed installed-app filesystem check. |
| Diagnostics, recovery, and explicit exit | partial | Rust tests cover report redaction, crash-marker validation, release identity, and webview error receipt; installed-app inspection confirmed the explicit exit confirmation and safe exit. | Packaged recovery workflow and GitHub handoff must be exercised without submitting external content. |
| Security and local-data boundary | partial | TASK086–TASK091 remediations are locally covered by 65 Rust tests: immutable channel binding/scoping, separate deletion credentials, bounded FFprobe output, watched-source withholding, and staged source deletion. The historical scan is sealed; fresh current-worktree rescan `1430aa6e-45ef-4912-b47f-ae09cf440a0f` is running. | Seal the current rescan and verify packaged capability behavior. |
| Windows packaging | partial | Fresh Tauri x64 NSIS build completed: 26,474,987 bytes, SHA-256 `A5657214A4C9443812670BC707FFE401BCB0E84AE8C3B1D00F80DA4B4003E6A4`; Authenticode status is `NotSigned`. It installed and launched successfully on Windows, then rendered its native exit confirmation and closed safely. | Build, sign, install, and smoke-test the exact production artifact. |
| macOS, Linux, Android, and iOS | blocked | Shared source and icon assets exist. | Platform signing, builds, representative device/emulator tests, file-picker/OAuth checks, and artifact identity evidence are absent. |

## Current local evidence

- `npm test` — 35 passing frontend tests.
- `npm run build` — TypeScript check and Vite build pass; Vite reports the
  existing static/dynamic `@tauri-apps/plugin-opener` chunk advisory.
- `cargo fmt --check` — passes after the current remediation changes.
- `cargo test` — 65 passing native tests after the current remediation changes.
- `npm run tauri build` — fresh unsigned x64 NSIS installer built and
  hash-verified.
- The NSIS installer completed successfully in a scoped local certification
  directory; its installed executable launched, rendered the native workspace
  and confirmed-exit modal, and exited safely. This used existing device-local
  application state, so it is not clean-profile or live-provider evidence.
- Rendered browser smoke at `http://127.0.0.1:4173/` — title and meaningful
  dashboard content confirmed, no relevant console warning/error, first-open
  dismissal and tab navigation exercised, and a 390 × 844 narrow viewport
  captured. Browser preview accurately labels native-only flows unavailable.

## Known blockers

1. No explicit approval, Desktop OAuth JSON, or safe non-production YouTube
   account is available for connect, upload/recovery, inventory, and deletion
   canaries.
2. No signed production artifacts or representative macOS/Linux/Android/iOS
   device evidence exists.
3. YouTube public publishing remains subject to the applicable Google/YouTube
   compliance and audit requirements.

## Retest order

1. Rerun the standard security scan against the remediated worktree.
2. Run all local checks and signed Windows smoke coverage for the resulting
   artifact.
3. With explicit operator approval, perform the least-destructive test-channel
   canaries: connect, private upload, forced-close recovery, inventory/dedupe,
   deletion authorization, and one deletion.
4. Build/sign and exercise each supported desktop/mobile platform, recording
   device, OS, version, artifact digest, results, and any limitations.
