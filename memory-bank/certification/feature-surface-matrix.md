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
| Upload queue, retry, and recovery | partial | 73 Rust tests include interrupted-copy recovery, provider range handling, exclusive claims, quota pause, startup reconciliation, and manual account-switch dispatch isolation. | Real resumable session interruption/reconciliation against YouTube. |
| Watched-folder monitor | partial | Rust tests cover stability, exclusions, inventory failure, background scans, bounded overview, existing-file intake, and final source-signature withholding. | Signed desktop monitor and provider canary. |
| Library, dedupe, and comparison | partial | Rust and frontend tests cover title candidates, exclusions, activity phases, and controls; Browser navigation renders the surface. | Live channel inventory and authenticated comparison-frame canary. |
| Deletion and source cleanup | partial | Rust tests cover typed confirmation, ownership-validation code paths, interrupted request retention, immutable channel scoping, and staged final source deletion. | Separately authorized live deletion canary on a non-production channel plus signed installed-app filesystem check. |
| Diagnostics, recovery, and explicit exit | partial | Rust tests cover report redaction, crash-marker validation, release identity, and webview error receipt; installed-app inspection confirmed the explicit exit confirmation and safe exit. | Packaged recovery workflow and GitHub handoff must be exercised without submitting external content. |
| Security and local-data boundary | partial | TASK086–TASK096 cover the completed local remediations, including immutable channel binding/scoping, bounded FFprobe output, link-safe deletion, watched-source managed snapshots, reconciliation isolation, and cryptographic duplicate-delete binding. Final standard scan `4b8fa995-b7f4-45b6-a378-729c7467fc88` is sealed with zero findings. | Verify packaged capability behavior and live-provider security boundaries. |
| Windows packaging | partial | Fresh x64 NSIS installer: 26,486,490 bytes, SHA-256 `68FDE33B61BB68E6852BE614F7D623607730C68EEE63CB5AF5775DB6BA60059A`; Authenticode status is `NotSigned`. Silent installation into a scoped temporary directory produced the executable, uninstaller, and bundled FFprobe. | Sign, install, and smoke-test the exact signed production artifact under a separate Windows profile. |
| macOS, Linux, Android, and iOS | blocked | Shared source and icon assets exist. | Platform signing, builds, representative device/emulator tests, file-picker/OAuth checks, and artifact identity evidence are absent. |

## Current local evidence

- `npm test` — 35 passing frontend tests.
- `npm run build` — TypeScript check and Vite build pass; Vite reports the
  existing static/dynamic `@tauri-apps/plugin-opener` chunk advisory.
- `cargo fmt --check` — passes after the current remediation changes.
- `cargo test` — 73 passing native tests after the current remediation changes.
- `npm run tauri build` — fresh unsigned x64 NSIS installer built and
  hash-verified.
- The fresh NSIS installer silently installed into a scoped temporary directory
  and produced the executable, uninstaller, and bundled FFprobe. A prior
  native launch confirmed the workspace and exit confirmation, but its profile
  isolation failed, so it is not clean-profile or live-provider evidence.
- Rendered browser smoke at `http://127.0.0.1:4173/` — title and meaningful
  dashboard content confirmed, no relevant console warning/error, first-open
  dismissal and tab navigation exercised, and a 390 × 844 narrow viewport
  captured. Browser preview accurately labels native-only flows unavailable.

## Known blockers

1. No explicit approval, Desktop OAuth JSON, or safe non-production YouTube
   account is available for connect, upload/recovery, inventory, and deletion
   canaries.
2. The current-user certificate store has no code-signing certificate; the
   latest Windows installer is `NotSigned`.
3. No separate Windows profile/VM, attached Android device, macOS toolchain,
   or non-Windows Rust target is available for representative platform evidence.
4. YouTube public publishing remains subject to the applicable Google/YouTube
   compliance and audit requirements.

## Retest order

1. Run all local checks and signed Windows smoke coverage for the resulting
   artifact.
2. With explicit operator approval, perform the least-destructive test-channel
   canaries: connect, private upload, forced-close recovery, inventory/dedupe,
   deletion authorization, and one deletion.
3. Build/sign and exercise each supported desktop/mobile platform, recording
   device, OS, version, artifact digest, results, and any limitations.
