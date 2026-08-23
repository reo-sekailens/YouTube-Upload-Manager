# Progress

## 2026-08-23 — Production certification execution started

- TASK085 now has a versioned feature-and-surface certification matrix and a
  static security findings report. The release is not production-certified:
  six source-backed security remediations, live Google/YouTube canaries, signed
  artifact smoke checks, and non-Windows platform evidence remain open.
- Manual uploads now bind to the active immutable channel at queue time and
  dispatch pauses when that reviewed channel is not active. Ordinary OAuth
  consent no longer requests the YouTube deletion scope. Deletion consent now
  uses a separate OS-protected credential that is channel-bound and cleared on
  expiry, disable, disconnect, or a new ordinary connection.
- FFprobe stdout is now capped while it is read, before parsing or unbounded
  process-output accumulation. Persisted library/deletion UI queries now begin
  using immutable active-channel IDs; archive scoping remains open in TASK088.

## 2026-08-23 — Whole-product certification planned

- TASK085 establishes a single feature-and-surface matrix for UI, native,
  packaged-platform, and live Google/YouTube certification. It requires every
  implemented workflow to have evidence or an explicit blocker and prohibits
  treating browser or fixture checks as proof of native or provider behavior.

## 2026-08-23 — Folder-monitor metadata handoff

- Bounded desktop FFprobe fallback to 15 seconds and prefer fast container duration metadata before it. This removes an unbounded pre-queue operation that could leave a watched file in `processing`.

## 2026-08-23 — Fast-start watched uploads with background verification

- Watched-folder uploads now dispatch as soon as their persisted light duplicate gate clears; BLAKE3 verification continues on a separate native worker.
- Verification state and source signature are persisted. Relaunch restarts pending work; an exact local completed-upload match cancels an unfinished provider transfer, while a post-completion match remains an explicit remote-deletion review.

## 2026-08-23 — Sequential large-file hash I/O

- Desktop deep hashes now open direct native file handles. Windows requests its
  sequential-scan cache policy and reads 8 MiB blocks, reducing read-call and
  cache overhead for HDD, USB, and removable-media sources.
- Mobile URI sources keep the platform-aware document-picker path. Exact BLAKE3
  evidence and worker isolation remain unchanged.

## 2026-08-23 — Upload queue clearing

- **Clear upload queue** now cancels every unfinished saved upload and removes
  it from the dashboard rather than leaving it visibly stuck as a saved item.
- Each unfinished upload now has its own **Remove** or **Cancel upload** action.
  An active upload stops at the next chunk checkpoint; local media and resumable
  evidence remain intact, and completed uploads are not removable this way.

## 2026-08-23 — BLAKE3 local content hashing

- New local content hashes use the official optimized BLAKE3 Rust crate for
  streamed deep dedupe, managed-copy verification, watched-folder verification,
  source-cleanup revalidation, and portable duplicate metadata.
- The native content hash remains deterministic and exact-match safe across
  desktop and mobile. SHA-256 remains limited to the Google OAuth PKCE
  challenge and external artifact-verification records where the protocol or
  publisher specifies it.

## 2026-08-23 — Watched-folder direct-source transfer

- Watched-folder intake now streams SHA-256 verification from the stable source
  and queues that source in place. It does not create a full managed-media copy
  or partial copy.
- The worker rechecks size and modification signature after hashing. The source
  must remain available and unchanged until YouTube confirms the upload; a
  missing or changed source fails safely rather than uploading a replacement.
- Folder-monitor copy describes this boundary and identifies **Watched file in
  place** as the transfer source. Post-confirmation source cleanup remains a
  separate opt-in.

## Current verified state — 2026-08-22

- Repository initialized with Git metadata and an initial `.gitattributes` file.
- The distributed Google OAuth client ID has been removed. Operators now create their own Google Cloud project and Desktop OAuth client, then import its JSON locally before connecting a channel.
- Unconfigured installs now present a six-step first-open guide for an operator-owned Google account, Cloud project, YouTube Data API, Google Auth Platform, Desktop OAuth client, and JSON import. Google actions remain operator-controlled in separate unprivileged app windows.
- Duplicate-comparison frames now remain unloaded until the operator presses Play. A separate, unprivileged in-app YouTube window supports direct operator sign-in; actual signed-in frame playback remains unverified pending an authorized test channel and operator consent.
- GitHub social-preview header created from the canonical upload-arrow icon as editable SVG and 1280 × 640 PNG; it is ready to upload in the repository Social preview settings.
- New manual files, intake batches, queued dispatch, and watched-folder discoveries now synchronize the active YouTube inventory before upload. Matching exact or numbered-copy titles remain device-local until the operator chooses **Upload anyway** or **Skip duplicate**, with an optional apply-to-all decision. Frontend validation passed; native test archiving remains unverified because the system drive was full during `cargo test`.
- Every video list now has an accessible local title search. Results update as the operator types and never call YouTube; upload-queue, duplicate-review, saved-library, and deletion-request filtering all passed local type and test checks.
- AI-operational memory bank scaffolded.
- Tauri 2 + React/Vite foundation, local SQLite queue schema, managed asset import, dependency manifest, and responsive dashboard source are in place. Native release build, TypeScript build, Rust tests, and browser UI checks pass on Windows.
- Interrupted managed-media copies now resume from their persisted partial file, re-verify the full SHA-256 digest, and retain a repairable local record if the original source has moved.
- The device-local YouTube connection path uses desktop loopback OAuth with PKCE. Refresh tokens and resumable-session URLs are held in the operating system's credential store; the webview receives only safe connection metadata.
- Queued managed files start private YouTube resumable sessions directly from the native Rust process. Provider-confirmed chunk offsets persist locally; interrupted and ambiguous results fail into reconciliation instead of starting a second session.
- Operators can explicitly bind one local folder to the active channel for recurring private uploads while the app runs. Existing files become the baseline; new direct-child videos must remain unchanged across two five-second scans, are copied into managed storage, checked against that channel's SHA-256 ledger and last synchronized title inventory, and pause if the channel changes.
- The authenticated channel's uploads playlist can be synchronized locally with pagination and bounded video-detail batches; the UI exposes a channel-gated sync action.
- The duplicate review now flags already-uploaded videos in the active channel when normalized titles match exactly or differ only by a trailing `(2)` or higher copy marker. Both YouTube IDs are shown and deletion remains a separate confirmed workflow.
- The removal workflow now presents the locally synchronized videos, requires typed video-ID confirmation before creating a local request, and lets the operator cancel it. No path currently calls YouTube deletion without a separate fresh authorization step.
- A separately scoped deletion authorization and executor now exist: it re-resolves the active channel, verifies the selected video's ownership immediately before `videos.delete`, and records success only for YouTube's HTTP 204 response. The live canary is still pending.
- Queue recovery now runs in native setup before each dashboard session: interrupted imports resume or fail into a repairable state, pre-network dispatch claims return to the saved queue, and interrupted uploads become `needs_reconciliation` without starting provider work. The manual recovery button has been removed.
- Duplicate review now has a channel-gated **Run dedupe** action that refreshes the active YouTube inventory and the rendered exact-title and trailing `(2+)` candidates in one operator action.
- The **Run dedupe** action now exposes a bounded, device-local activity log for its real inventory synchronization and candidate-refresh boundaries, final counts, and errors. It explicitly confirms that duplicate detection does not remove videos.
- Dedupe activity now also provides a three-step, accessible phase-progress bar that reports only real command boundaries and remains visibly incomplete on failure.
- The packaged Windows startup stack overflow is fixed: matching WER dump symbols isolated a release-inlined 1 MiB digest buffer in startup reconciliation, both streaming buffers are now heap-backed, and the reinstalled application stayed running for 10 seconds with no new Application Error event.
- Google Auth Platform branding, authorized domain, testing audience, and the requested YouTube scopes are configured. Version 0.1.1 adds safe OAuth token-error categories, a per-operator Desktop OAuth JSON import fallback, and callback-status polling; the fresh NSIS installer is installed locally. A live connection using an operator-selected Desktop JSON remains pending.
- Connected-account identity now has a distinct, responsive status row. Uploaded-title duplicate candidates can be compared through two in-app YouTube privacy-enhanced embeds with shared play/pause and seek controls. The removal review supports Select all and a multi-item review queue, while still requiring a typed ID for every local request and a fresh deletion authorization for execution.
- The duplicate-comparison player origin is now explicitly permitted by the desktop content-security policy, resolving the previously blocked privacy-enhanced YouTube frames.
- Duplicate comparisons now provide compact synchronized icon controls for back 10 seconds, play/pause, and forward 10 seconds, in addition to the position range control.
- Watched-folder setup now makes its recurring visibility explicit: private or unlisted only. The queue intake area separates its action title from supporting copy and uses more breathable spacing.
- Every manual drag/drop or picker intake now opens a required review before
  import. The operator must declare Made for Kids, choose visibility, and
  choose an owner-authorized playlist or no playlist. These values stay in the
  local item record; the native uploader sends the audience declaration and
  records the later playlist-add outcome separately.
- A device-wide Made for Kids default can now be changed locally. It only
  preselects the same manual intake review; it never bypasses the per-batch
  declaration or changes watched-folder automation.
- Manual imports now support native drag-and-drop and multi-file selection. Each item defaults to private but can be explicitly set to unlisted or public before queueing; the choice is device-local, audited, and applied by the native resumable uploader. The queue shows live current and remaining-batch progress plus ETA after server-confirmed transfer measurements. Watched-folder uploads remain private or unlisted only.
- Dashboard cards now use the available workspace width. Connected manual import batches queue and begin native upload dispatch automatically; watched-folder files do the same once stable. Recognized YouTube daily upload-limit responses return the affected item to the saved queue, persist a 24-hour device-local pause, and resume dispatch after expiry while the app runs or after relaunch.

## Next milestone

Execute [TASK001](tasks/TASK001-cross-platform-foundation.md), beginning with the local Tauri 2 + React/Vite foundation, local persistence, platform OAuth registrations, secure storage, and native device test matrix.

## Verification ledger

| Date       | Check                                             | Result                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-08-22 | Watched-folder auto intake and post-upload cleanup | Folder monitoring now treats existing and newly discovered supported direct-child files identically: after a stable two-scan gate, native managed-copy, dedupe, and resumable upload flow begins automatically. The Process existing files control is removed; legacy baselines migrate on their next scan. Original-source deletion now presents a typed filename confirmation only after YouTube reports upload completion, while separately opted-in automatic cleanup remains available before processing and remains guarded by source rehashing. Focused native tests, frontend test, TypeScript check, formatting, and diff check passed. |
| 2026-08-22 | Native confirmed app exit                         | Replaced the confirmed-exit webview `Window.destroy()` path with a dedicated native `AppHandle::exit(0)` command. Normal title-bar close still opens the confirmation; recovery mode still permits an unblocked close without acknowledging its marker. All 48 Rust tests, all 34 frontend tests, Rust formatting/check, TypeScript check, and diff check passed. Rebuilt unsigned x64 installer SHA-256: `CA0C6F05F4C767FBD8A4E5AE52C6187D506A9A031E6887D66291866E801329A2`; process termination itself is intentionally not automated. |
| 2026-08-22 | Cross-platform app-icon packaging                 | Regenerated every Tauri desktop and mobile icon variant from the canonical blue upload-arrow PNG. The Windows release executable icon was extracted and visually verified, and the unsigned x64 NSIS installer `YouTube Upload Manager_0.1.9_x64-setup.exe` was rebuilt (SHA-256 `DA66A2301D5733267B175386B413A6F3EAECFD0C15B6D1003BFEAB26C78D4D82`). Installation over the currently running application was not exercised. |
| 2026-08-22 | Folder-monitor live overview                     | Watched-folder monitoring now exposes a bounded, channel-scoped live overview: active upload/copy items, watched-folder queue entries, recent observations, and collapsible scan logs. Native automatic scans continue every five seconds while the app runs, record a safe recoverable error state if a cycle fails, and retain the manual action as **Refresh scan**. The UI receives filenames rather than observation source paths. Focused native and frontend tests, TypeScript check, and diff check passed; a signed desktop monitor is still needed to exercise a populated live session. |
| ---------- | ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-22 | Repository inventory                              | Only `.git` and `.gitattributes` existed before this scaffold.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 2026-08-22 | Memory-bank inventory                             | Required context and task-index files created.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 2026-08-22 | Cross-platform product plan                       | TASK001 created and aligned with current official YouTube API and policy documentation; implementation not started.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-08-22 | On-device architecture decision                   | The user required all application work to remain on-device; TASK001 and durable architecture now prohibit application-controlled cloud services.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 2026-08-22 | Queue recovery implementation                     | Managed local import records the partial file and resumes it after relaunch when the original source remains available; completed copies are SHA-256 verified before they can enter the saved queue.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 2026-08-22 | Foundation verification                           | `npm run build`, `npm run test`, `cargo fmt --check`, `cargo test`, `cargo check`, `git diff --check`, native release executable, desktop browser UI, and 390px mobile UI all passed locally.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-08-22 | OAuth and upload implementation                   | PKCE loopback authorization, OS credential-store refresh/session handling, direct private resumable upload worker, offset recovery, explicit disconnect, and safe client-ID configuration implemented. Live OAuth and YouTube canary remain unrun because no OAuth client/test channel has been supplied.                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 2026-08-22 | Inventory implementation                          | Direct channel inventory sync paginates uploads, fetches video metadata in batches, and records the result only in the local database. Live API verification remains pending.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-08-22 | Uploaded-title duplicate candidates               | Exact normalized titles and trailing `(2)` or higher copy markers are paired deterministically within the active channel. Rust tests, TypeScript checks, web tests, production build, and desktop/mobile Browser QA passed; live YouTube inventory verification remains pending.                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 2026-08-22 | Watched-folder private uploads                    | Opt-in channel-bound polling, baseline exclusion, stable-file ingestion, SHA-256 and synced-title duplicate withholding, crash-safe dispatch claiming, and responsive operator controls passed 9 Rust tests, 8 web tests, production build, and desktop/mobile Browser QA. A live private upload canary remains pending.                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-08-22 | Windows installer packaging                       | The current application source produced an x64 NSIS setup executable after a fresh frontend and optimized Rust build. The unsigned artifact was hash-verified; installer execution and live YouTube operations were not performed.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2026-08-22 | Startup recovery and dedupe control               | Native startup reconciliation and the channel-gated Run dedupe action passed 11 Rust tests, 9 web tests, type checking, production build, and deterministic desktop/mobile Browser QA. No live YouTube inventory sync was executed.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-08-22 | Windows startup crash repair                      | WER exception `0xc00000fd` and the matching release PDB resolved to `reconcile_queue_impl` allocating approximately 1 MiB on the GUI-thread stack. Heap-backed buffers and a 512 KiB-stack recovery test passed; the refreshed NSIS installer reinstalled with exit code 0 and the installed app remained alive for 10 seconds with zero new Application Error events.                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-08-22 | OAuth recovery fallback                           | Google consent-screen branding/domain/scopes were saved while retaining Testing mode and its two test users. Rust tests (13), TypeScript checks, web tests (9), production build, and an installed unsigned Windows 0.1.1 NSIS package passed. The app can now import a selected Desktop OAuth JSON into protected local credential storage; no live token exchange has yet succeeded.                                                                                                                                                                                                                                                                                                                                                    |
| 2026-08-22 | Deletion review implementation                    | Local YouTube video review, typed-ID deletion-request creation, audit recording, pending-request visibility, and cancellation implemented. Provider deletion remains intentionally unavailable pending its separately scoped authorization and live canary.                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 2026-08-22 | Confirmed-upload source cleanup                   | Manual batch intake now applies an opt-in source-cleanup choice to each item; individual draft uploads can change it before queueing; and watched-folder configuration persists the same choice. Native cleanup runs only after YouTube returns a video ID, persists a pending receipt for restart recovery, re-hashes the external original, and never deletes managed media. Type check, 30 web tests, production build, Rust format, 29 Rust tests, cargo check, diff check, and browser-preview folder-monitor UI passed; a live YouTube upload/deletion canary remains unverified.                                                                                                                                                   |
| 2026-08-22 | Light-match local-file deletion                   | A filename-first local match now exposes the same deletion action as a deep hash match when its source is an eligible desktop path. Before confirmation, native code reloads the persisted scan result, verifies the match is still present, hashes the source, and issues only a short-lived token; permanent removal still needs an exact typed filename and re-hash. Rust tests (30), web tests (31), type check, production build, diff check, and Browser preview passed; signed-app local-match rendering remains unverified.                                                                                                                                                                                                       |
| 2026-08-22 | Deletion execution implementation                 | Fresh `youtube.force-ssl` authorization, second typed-ID confirmation, immediate channel/video ownership validation, and HTTP 204-only deletion receipt implemented. It has not been exercised against YouTube.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 2026-08-22 | Dedupe activity log and Windows package           | Type checks, 12 web tests, production build, Rust formatting, and 13 Rust tests passed. The unsigned v0.1.3 x64 NSIS installer was built with a visible bounded dedupe log; live inventory synchronization was not executed for this release check.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-08-22 | Comparison and multi-select deletion review       | Type checks, 12 web tests, production build, diff check, and browser-preview desktop/mobile QA passed. Browser preview cannot populate a connected test channel; live embeds and provider actions remain unverified.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 2026-08-22 | Manual intake audience and playlist review        | Type checks, 14 web tests, production build, Rust format/tests, and diff check passed. Live native drag/drop, playlist listing/insertion, and provider behavior require an authorized test channel.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-08-22 | Device-wide Made for Kids default                 | Type checks, 15 web tests, production build, Rust format/tests, and diff check passed. Browser preview rendered the control; signed-app persistence remains unverified.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 2026-08-22 | Upload progress, visibility, and drag-drop intake | Production build, 12 web tests, Rust formatting, 14 Rust tests, and diff check passed. Browser-preview queue intake rendered with no console warnings/errors; native drag-drop, live provider transfers, and ETA behavior await an authorized local canary.                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 2026-08-22 | Dedupe phase-progress bar                         | Type checks, 13 web tests, production build, Rust formatting, 14 Rust tests, diff check, and a v0.1.4 NSIS package passed. Live inventory synchronization was not executed for this UI check.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-08-22 | YouTube embed CSP repair                          | The supplied app screenshot established that the privacy-enhanced comparison frames were blocked by CSP. The v0.1.5 package permits only `youtube-nocookie.com` frames; type checks, 14 web tests, production build, Rust formatting, and 14 Rust tests passed. Live playback remains dependent on YouTube's per-video embedding policy.                                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-08-22 | Comparison playback icons                         | Icon-only synchronized back 10, play/pause, and forward 10 controls passed type checks, 16 web tests, production build, Rust formatting, and 14 Rust tests. The v0.1.6 Windows installer was packaged; live owner-player interaction remains unverified.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-08-22 | Watched-folder visibility and queue layout        | Private/unlisted recurring visibility, safe private migration for existing monitor settings, and queue intake spacing passed type checks, 16 web tests, production build, Rust formatting, and 14 Rust tests. The v0.1.7 Windows installer was packaged; live automatic upload behavior remains unverified.                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 2026-08-22 | Full-width auto-dispatch and daily-limit recovery | All dashboard cards now render full width. Connected manual batches auto-queue and dispatch; stable watched-folder files dispatch directly. Recognized daily-limit responses persist a 24-hour local pause and resume queue work after expiry. Type check, 16 web tests, production build, Rust format, 15 Rust tests, browser QA, and the v0.1.8 NSIS package passed; live quota-provider behavior remains unverified.                                                                                                                                                                                                                                                                                                                   |
| 2026-08-22 | Windows taskbar icon alignment                    | `npx tauri icon` regenerated the ICO from the canonical upload-arrow artwork. An x64 NSIS 0.1.4 installer built and installed with exit code 0; the installed executable's associated icon was verified and the app launched locally.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 2026-08-22 | Product rename and trademark notice               | The app, package metadata, installer display name, and public documentation use YouTube Upload Manager. The GitHub repository is now `Satoshiii-DCS/YouTube-Upload-Manager`, and local `origin` points there. The physical checkout is renamed after Codex releases its active workspace process.                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| 2026-08-22 | Native dropdown runtime detection                 | Replaced internal-bridge detection with Tauri's public runtime capability detector, so dropdown enablement follows the supported native runtime contract. Type check, 16 web tests, production build, and diff check passed.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 2026-08-22 | Cross-platform pre-ingest duplicate check         | The Duplicate review workspace now accepts arbitrary non-empty files for local SHA-256 comparison before ingest, including `.insv` and `.lrv`. Desktop supports drag/drop; Android and iOS use the native document picker and URI-aware native filesystem access. Exact saved-file matches, duplicates within the selection, and optional connected-channel title matches remain distinct. Newer drops supersede stale in-flight checks, while iOS releases its scoped picker access after hashing. `cargo check`, focused Rust tests for proprietary files, drag paths, and mobile picker URIs, type check, 22 web tests, production build, and diff check passed; mobile device-picker behavior remains unverified on physical devices. |
| 2026-08-22 | Local duplicate deletion from pre-ingest review   | Exact desktop source-file matches now display a local comparison/deletion card. Deletion is permanent but requires an opaque, 15-minute native review token, typed exact filename, path protection for managed media, re-hashing immediately before removal, and a local audit event. A focused Rust test verifies wrong-confirmation rejection and external-source-only deletion; frontend command coverage, type check, web tests, production build, and diff check passed.                                                                                                                                                                                                                                                             |
| 2026-08-22 | Multiple-file picker                              | Manual intake and pre-ingest duplicate checks explicitly request multi-file selection on all supported picker platforms. The shared picker-result normalizer preserves each selected path/URI and makes a cancelled picker a no-op. Focused unit coverage, type check, web tests, production build, and diff check passed.                                                                                                                                                                                                                                                                                                                                                                                                                |
| 2026-08-22 | Compact cross-device transfer                     | New Export and import workspace writes gzip-compressed SHA-256/title metadata and synchronized YouTube inventory, then imports it as non-dispatchable metadata-only records for local dedupe. OAuth Desktop JSON is imported separately into protected storage and every device reauthorizes; refresh tokens, secrets, paths, media, and resumable sessions never transfer. Native archive round-trip test, type check, 26 web tests, production build, diff check, and browser-preview UI passed.                                                                                                                                                                                                                                        |
| 2026-08-22 | Background pre-ingest batches                     | Pre-ingest hashing and optional inventory synchronization now run on Tauri's blocking worker pool instead of the desktop UI thread, so large drops keep the application interactive. The duplicate panel displays the selected file count while work runs. Cargo check, focused preflight tests, type check, 26 web tests, production build, and diff check passed.                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-08-22 | Crash-safe light and deep pre-ingest matching     | Fast filename matching is now the default and never reads selected media. Opt-in deep jobs persist each SHA-256 checkpoint and resume unfinished files at app startup. YouTube inventory sync stages pages then atomically replaces the old snapshot only after success. Local verification: cargo check, type check, 27 web tests, production build, and diff check; physical mobile interruption recovery remains unverified.                                                                                                                                                                                                                                                                                                           |
| 2026-08-22 | Universal crash-resume recovery                   | Startup now returns interrupted uploads to the queue only when their protected resumable session exists, preserving provider-range reconciliation. Deletions checkpoint before the remote call and become explicit recoverable requests after a crash. Archive export publishes a synced temporary file without overwriting existing data. Cargo check, focused Rust recovery tests, type check, 27 web tests, production build, and diff check passed; live YouTube interruption canaries remain unverified.                                                                                                                                                                                                                             |
| 2026-08-22 | Cancel and clear persisted work                   | Added audited cancellation for active pre-ingest dedupe jobs, locally queued uploads, and pending/recoverable deletion requests. Clearing retains managed media and never calls YouTube; cancelled jobs do not resume on relaunch. Cargo check, focused Rust preflight tests, type check, 28 web tests, production build, and diff check passed.                                                                                                                                                                                                                                                                                                                                                                                          |
| 2026-08-22 | Native worker isolation                           | Moved manual folder scans, archive transfer, inventory/title checks, playlist retrieval, deletion execution, local import, and queue reconciliation to blocking native workers. The webview remains responsive while native work runs. Cargo check, focused Rust preflight tests, type check, web tests, production build, and diff check passed.                                                                                                                                                                                                                                                                                                                                                                                         |
| 2026-08-22 | Windows installer after worker isolation          | Built the unsigned x64 NSIS installer `YouTube Upload Manager_0.1.9_x64-setup.exe` from the current source. Size 4,464,055 bytes; SHA-256 `E7279C2C5E0369C984015B1921F7DB00EC391CAD0FADCC29F916FADE9BD3F030`. Installation and live provider behavior were not exercised.                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 2026-08-22 | README and GitHub Wiki                            | README now covers the implemented feature set and installer workflow. GitHub Wiki creation/publication is blocked because the configured GitHub CLI credential returned HTTP 401 and the Wiki Git endpoint was unavailable.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 2026-08-22 | Security remediation                              | Watched-folder automation now binds and verifies immutable channel IDs, queued uploads use exclusive durable dispatch claims, and invalid OAuth loopback traffic no longer consumes the PKCE verifier. Rust formatting, 26 Rust tests, cargo check, TypeScript check, 28 web tests, production build, and diff check passed locally. Live Google/YouTube canaries remain pending.                                                                                                                                                                                                                                                                                                                                                         |
| 2026-08-22 | Persisted dedupe exclusions                       | Duplicate-review cards can now persist a false-positive Ignore decision without changing media. An operator-controlled re-audit restores all exclusions, refreshes local candidates without a connection, and re-syncs connected YouTube inventory. Rust formatting, 27 Rust tests, cargo check, TypeScript check, 29 web tests, production build, and diff check passed locally.                                                                                                                                                                                                                                                                                                                                                         |
| 2026-08-22 | Remote video list alignment                       | Deletion-review video cards now use fixed checkbox and action columns with flexible, safely wrapping metadata rather than space-between distribution. The mobile breakpoint stacks each row deliberately. Type check, 29 web tests, production build, diff check, and desktop/mobile browser-preview checks passed.                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-08-22 | Deletion action labels                            | Video deletion list actions now say Delete one and Delete selected. They retain the existing local-request, typed-ID, and separate permanent-execution safety stages. Type check, 29 web tests, production build, diff check, and browser-preview checks passed.                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 2026-08-22 | Bulk local pre-ingest deletion                    | Eligible local duplicate cards now offer individual selection, Select all, selected-count feedback, listed batch review, a typed `DELETE N LOCAL FILE(S)` confirmation, and sequential progress. Each source still receives its own persisted-match check, managed-media-path exclusion, fresh native hash/token, and immediately-pre-delete re-hash; a failure stops the remaining batch. Prettier, 31 web tests, type check, production build, Rust format, 30 Rust tests, cargo check, diff check, and browser-preview entry-state QA passed. Signed-app populated local-match cards remain to be exercised.                                                                                                                           |
| 2026-08-22 | Current Windows installer                         | Rebuilt the unsigned x64 NSIS installer from the current workspace. `YouTube Upload Manager_0.1.9_x64-setup.exe` is 4,638,687 bytes with SHA-256 `25D6D9383A6D876833A9200628D811D3A67EB33F45174B5DE73C0059E7D61AC5`. The bundle build includes the passing production frontend build; installation and live-provider behavior were not exercised.                                                                                                                                                                                                                                                                                                                                                                                         |
| 2026-08-22 | Remote-title local source deletion                | Uploaded-title pre-ingest matches now expose the same local deletion and bulk controls as saved-local matches. Native preparation revalidates the active channel's normalized title evidence before its existing external-path, fresh-hash/token, typed-filename, and pre-delete-rehash safeguards. A regression test covers `VID_20251218_195343_00_005.mp4` against `VID 20251218 195343 00 005`; 31 Rust tests, 31 web tests, type check, production build, cargo check, and diff check passed.                                                                                                                                                                                                                                        |
| 2026-08-22 | Pre-ingest comparison metadata                    | Remote title-match cards now show local and remote titles plus video-length fields by default. Remote duration, privacy, and inventory-sync metadata are provided from the local inventory; secondary details collapse by default. Local length is honestly marked unavailable before ingest for arbitrary/proprietary file formats. Rust format, 31 Rust tests, 31 web tests, type check, production build, cargo check, and diff check passed.                                                                                                                                                                                                                                                                                          |
| 2026-08-22 | Nightly pre-release workflow                      | Every `main` commit has a configured GitHub Actions pipeline for unsigned Windows, Linux, and macOS desktop artifacts plus an installable universal Android debug APK. It allocates immutable SemVer-aligned nightly tags such as `v0.1.9-nightly.a` through `.z`, then `.aa`, from prior GitHub pre-releases, compiles that identifier into diagnostics, and creates a new release. Artifact downloads remain separated, only package files are selected, and each uploaded asset receives a unique artifact-prefixed name. A remote rerun is pending.                                                                                                                                                                                                                                                                                                                                                                                  |
| 2026-08-22 | Contributor agreement and live GitHub Wiki        | Added DCO 1.1 contributor agreement, contribution guide, and PR sign-off reminder. Published the live Wiki Home and nine deep technical pages covering setup, architecture, native commands, persistence/recovery, OAuth boundaries, extension rules, workflows, and troubleshooting; Wiki commit `249a780` was accepted by GitHub.                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-08-22 | Nightly build dependency caching                  | Nightly builds now restore npm downloads, Cargo registries/build outputs, the fixed Android NDK, and Gradle dependencies/wrapper. Caches contain only public build inputs, are keyed to dependency state, and safely fall back to clean installs when unavailable.                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2026-08-22 | Android nightly entry point                       | The first Android APK build reached Gradle but was rejected because the native library did not export Tauri's mobile runtime entry point. `run()` now conditionally applies `tauri::mobile_entry_point`; desktop behavior is unchanged. A subsequent nightly run will verify Android packaging.                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 2026-08-22 | Nightly action runtime updates                    | Repository-controlled nightly actions now use Node 24-capable releases for checkout, Node, cache, Java, Android SDK setup, and artifacts. GitHub-managed Pages annotations remain outside repository control; a subsequent nightly run will verify the updated actions.                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 2026-08-22 | Local deletion reuses opt-in duplicate review     | Preparing and confirming local duplicate deletion now reuses the accepted persisted review and never re-hashes selected files. Deep SHA-256 remains an opt-in duplicate-review operation; exact typed filename, short-lived token, managed-workspace protection, and local audit receipt remain. Rust format and 33 Rust tests passed; frontend type check and diff check passed.                                                                                                                                                                                                                                                                                                                                                         |
| 2026-08-22 | Crash recovery and issue handoff                  | Native panics and webview errors now preserve timestamp-only markers. A modern recovery screen shows immediately when possible or at next startup, preserves work without retrying provider actions, and offers the redacted local GitHub report. Frontend tests (31), production build, native format check, and 37 native tests passed; browser visual QA captured the recovery screen. |
| 2026-08-22 | About support links                               | Added explicit system-browser links to the live GitHub Wiki and project website, alongside the existing local diagnostics report. Frontend tests (31), production build, diff check, and browser-preview visual QA passed. |
| 2026-08-22 | GitHub diagnostics report                         | Added an About/support tab that copies a bounded, redacted Markdown issue report with safe app/system details, saved operational context, and a native panic timestamp marker. Frontend tests (31), production build, Rust formatting, and Rust tests (35) passed; browser preview confirmed copied-status feedback. |
| 2026-08-22 | Release identity                                  | About and copied diagnostic reports now use native release metadata to show the exact version plus regular or nightly channel. Nightly CI explicitly stamps its desktop and Android artifacts as `nightly`; other builds default to `regular`. |
| 2026-08-22 | One-click GitHub issue handoff                    | About and crash recovery now create the redacted local report, copy it as a backup, and open a GitHub new-issue draft with its complete body pre-filled. The operator reviews and submits it explicitly. |
| 2026-08-22 | Recovery failure identity                          | Crash recovery now shows the latest safe failure category and timestamp, while keeping raw exception payloads out of the screen and report. |
| 2026-08-22 | Modal layering and playlist creation               | Upload-review backdrop moved outside the dialog so only the page dims. Manual single-file/batch review and watched-folder configuration can create a private YouTube playlist and select it immediately. Local validation recorded after integration; no live playlist was created. |
# 2026-08-22 — YouTube upload-limit preflight

- Added native 256 GB and 12-hour preflight validation before manual or watched-folder managed copying.
- Verified locally: 50 Rust tests, 35 frontend tests, and production web build pass.
# 2026-08-22 — Universal upload light dedupe

- Extended the native pre-upload title gate to same-batch and active local-queue matches, plus pre-copy watched-folder matching.
- Verified locally: 51 Rust tests, 35 frontend tests, and production web build pass.

# 2026-08-23 — Certification security remediation

- Manual uploads now bind to the immutable active channel at queue time and
  pause safely if the active channel changes before dispatch.
- Watched-folder uploads revalidate their persisted source signature before
  requesting provider access; a changed source is cancelled locally with an
  audit receipt. Focused Rust regressions passed. These are local safeguards,
  not live YouTube certification.

# 2026-08-23 — Channel-isolated local certification

- Completed inventory snapshots, remote-title dedupe, exact-local duplicate
  review, deletion requests, and dashboard queue visibility are scoped to the
  active immutable channel ID. Snapshot replacement preserves other channel
  data while changing only the refreshed channel's records. Focused Rust
  isolation tests pass; live multi-channel YouTube verification remains pending.

# 2026-08-23 — Race-safe local deletion staging

- Reviewed external sources are atomically staged to a unique sibling path
  before final validation and deletion. Local duplicate deletion retains its
  reviewed file signature and verifies it both before and after staging, so a
  changed or replacement file is not deleted. Focused Rust safety tests pass;
  installed-app filesystem behavior remains to be certified.

# 2026-08-23 — Installed Windows artifact smoke

- The fresh unsigned x64 NSIS installer installed into a scoped local
  certification directory, launched its native Tauri window, rendered the
  workspace plus confirmed-exit modal, and exited safely. The executable uses
  the normal device-local application profile, so this is a packaged runtime
  smoke check only; it does not certify a clean profile, signing, or provider
  operations.

# 2026-08-23 — Current security certification rescan

- Sealed current-worktree security scan `1430aa6e-45ef-4912-b47f-ae09cf440a0f`
  reported two native-file safety findings: link/reparse-point following in
  local destructive flows (TASK093) and a mutable-path race before watched
  source upload (TASK094). Production certification remains blocked until both
  tasks are remediated and independently rescanned.

# 2026-08-23 — Native-file finding remediation

- TASK093 now rejects symlinks and Windows reparse points at destructive local
  boundaries; a Windows regression proves linked cleanup retains the link and
  target. TASK094 snapshots each stable watched source from a no-follow file
  handle into verified managed storage before any dispatch, with regressions
  for source replacement and link swap. `cargo test` passed 71 native tests;
  a fresh formal post-remediation security scan remains required.

# 2026-08-23 — Duplicate deletion content binding

- The post-remediation security review identified a size/timestamp-preserving
  replacement gap in duplicate deletion. TASK096 records a BLAKE3 content
  binding in the short-lived review token and verifies the staged bytes before
  removal. Focused regression passes; formal scan validation remains active.

# 2026-08-23 — Final local security and installer evidence

- Final standard security scan `4b8fa995-b7f4-45b6-a378-729c7467fc88` sealed
  with zero findings after TASK093, TASK094, and TASK096. The complete native
  suite passed 73 tests; frontend suite passed 35 tests and the production
  web build passed.
- Fresh Windows x64 NSIS package is 26,486,490 bytes with SHA-256
  `68FDE33B61BB68E6852BE614F7D623607730C68EEE63CB5AF5775DB6BA60059A`.
  It silently installed to a scoped temporary location with executable,
  uninstaller, and FFprobe present. It is unsigned; a process environment
  override did not isolate Tauri's app-data directory, so clean-profile signed
  runtime proof remains outstanding.

# 2026-08-23 — External production-certification availability check

- Current-user certificate-store inspection found no code-signing certificate.
  No Android device is attached, no macOS toolchain is installed, and only the
  Windows Rust target is available. Together with the missing approved
  non-production YouTube account/OAuth client, these are verified external
  gates rather than local implementation failures.

# 2026-08-23 — Whole-app performance optimization planned

- Completed a source and build-output audit of the frontend, native runtime,
  persistence, recovery, provider transport, uploads, inventory/dedupe,
  preflight, watched folders, media probing, build preparation, packaging, and
  current performance tests. The ranked findings and provisional budgets are in
  `memory-bank/performance-audit-2026-08-23.md`.
- The primary speed limits are repeated schema work on ordinary database opens,
  unbounded large-file recovery before first render, eager mounting of all
  workspaces, full polling snapshots with quadratic duplicate reconstruction,
  per-chunk HTTP client/allocation churn, and claimed uploads that execute
  serially.
- Created TASK102 through TASK112 as a dependency-ordered optimization program.
  TASK103 establishes local packaged p50/p95 baselines before speed claims;
  TASK104, TASK106, and TASK110 are the first independent implementation wave.
- `npm run build` passed on the current working tree. The main frontend bundle
  measured 334,910 bytes raw / 96,394 bytes gzip and the main CSS measured
  60,454 bytes raw / 10,689 bytes gzip. No application code was changed by this
  planning audit, and no packaged runtime or live-provider timing was claimed.

# 2026-08-23 — Two-phase fast startup and recovery

- Native setup now stops after one database bootstrap and fail-closed recovery
  classification. Media reads, FFprobe, protected upload-session lookup, and
  provider work cannot begin before React has rendered the safe startup shell.
- One bounded post-shell coordinator resumes interrupted device-local work, and
  upload dispatch remains fenced until classification, shell rendering,
  deferred recovery, and immutable active-channel binding all pass.
- Verified locally: 6/6 focused startup tests, 91 native tests with 5 ignored,
  6/6 performance-harness startup tests, 50 frontend tests, type checking,
  production build, frontend payload budgets, and browser-preview visual QA.
  Current signed/packaged startup and large interrupted-profile timings remain
  explicitly assigned to TASK112.

# 2026-08-23 — Bounded media runtime and FFprobe preparation

- FFprobe preparation now reuses an identity-bound, checksum-verified receipt
  without network access or a full rehash, skips mobile provisioning, and
  selects only the requested desktop target. Its fixture suite passed 6/6; the
  cached Windows sidecar was verified as x64 PE with its license present.
- Runtime probes are limited to two processes and copy/hash reads to one per
  source volume. Active uploads receive same-volume priority, waits and probe
  processes are cancellable, and watched-hash cancellation no longer opens a
  database on every chunk.
- Upload validation uses a 64 KiB duration-only probe; rich metadata is cached
  and coalesced only for the same canonical path, size, and nanosecond mtime.
  Focused probe/media/cache tests passed, including startup's zero-media-work
  boundary and malformed/oversized output handling.
- Windows portable release automation now includes and validates the x64
  FFprobe sidecar and license. TASK110 remains in progress until a quiet-window
  copy/hash distribution and fresh installer/portable extraction prove the
  exact packaged artifacts; no packaged or live-provider result is claimed.
- The quiet release rerun found copy-plus-BLAKE3 at p50 929.784 ms / p95
  1,871.217 ms while standalone BLAKE3 stayed stable. An 8 MiB heap buffer plus
  Windows sequential-scan hints improved copy to 624.299/897.986 ms without
  changing final `sync_all`, partial resume, digest, scheduling, or cancellation
  semantics, but it still missed the frozen 335.852/368.178 ms gate.
- Release-only phase evidence on the C: temporary volume separated p50 74.891
  ms stream/write/hash from p50 934.431 ms durable flush while C: had only 5.982
  GiB free. The safe candidate is retained, but TASK110 remains open for a
  healthy-headroom durable rerun and fresh packaged sidecar inspection; the
  crash-safe flush is not traded away to make a benchmark pass.

# 2026-08-23 — Revisioned events and zero settled-idle polling

- Durable schema-v2 revisions now drive channel-scoped upload/preflight deltas
  and safe invalidations for connection, inventory, deletion, dedupe, folder,
  and quota state. The singleton frontend listener attaches before catch-up,
  rejects cross-channel batches, and recovers retained-history gaps from one
  bounded snapshot.
- Removed all frontend intervals and the permanent native folder/quota polling
  loops. One commit-hook dispatcher blocks when idle; folder and quota workers
  exist only while enabled work or a saved deadline requires them. A 200-update
  fixture retains all revisions while delivering one compact sub-2 KiB delta.
- Verified locally: the full native suite passed 103 tests with 5 release-only
  benchmarks ignored; post-seam event tests passed 4/4 and quota lifecycle tests
  passed. Frontend tests passed 59/59, TypeScript and production build passed,
  and the deterministic budget reports one resident dispatcher. Browser preview
  rendered the lazy Folder monitor without console warnings/errors. Packaged
  idle/wakeup and live-provider timing remain TASK112 evidence, not a current
  production claim.
- The complete listener-first event bridge now loads only after the safe startup
  fence and active-channel binding; video and preflight picker code loads only
  on explicit operator action. The frontend performance gate passes at 230,549
  bytes raw / 71,624 bytes gzip for initial JavaScript and 38,470 bytes raw for
  initial CSS, with all 59 frontend tests passing after the split.

# 2026-08-23 — Incremental native media module boundary

- Extracted the stabilized native media scheduler, active-upload guards,
  resumable copy/hash primitives, ISO-BMFF parser, bounded FFprobe lifecycle,
  and stable-signature cache into `media_runtime.rs`. Tauri commands, SQLite
  cancellation, schema mapping, audits, channel checks, recovery, and provider
  orchestration remain at their existing ownership boundaries.
- The native library compiled after cutover; focused scheduler/probe/cache tests
  passed before the copy-resume fixture moved beside its owner, and the frozen
  post-TASK108/TASK109 integrated native suite then passed 122 tests with 0
  failures and 5 ignored release-only benchmarks. Database-only startup
  classification and 512 KiB small-stack recovery remain green. The
  deterministic native configuration gate also passed.
- No Cargo release-profile override was adopted. The isolated default cold-build
  comparison ran out of drive space before link, its 1.6 GiB temporary target
  was cleaned without touching the shared target, and no invalid timing was
  recorded. Panic/symbol support remains intact; packaged startup/size and
  installer proof remain TASK112 rather than a source-build claim.

# 2026-08-23 — Pooled provider transport and true parallel upload scheduling

- Provider HTTP clients are lazy, pooled, rustls-backed, and timeout-bounded;
  native setup still builds zero clients. Upload/deletion access-token caches
  are separate, expiry aware, singleflight refreshed, and process-memory only.
- Resumable uploads reuse one pooled client for the full session and move one
  exact bounded chunk buffer into each request, with no extra full-chunk copy.
  Every provider `308` remains durably recorded before the next request.
- A four-permit durable scheduler now overlaps independent upload workers,
  enforces cached per-volume limits, and rotates volume priority between
  handoffs. A real loopback HTTP barrier proved exact overlap at capacities two
  and four without exceeding per-volume maxima.
- Provider success, immutable channel identity, video ID, confirmed bytes, and
  audit receipt commit before lower-priority playlist/session-cleanup/source
  cleanup. Relaunch and channel-isolation fixtures preserve that receipt when a
  playlist step fails; destructive remote deletion remains sequential.
- The final integrated release suite passed 5/5 benchmarks. Its 64 MiB loopback
  upload fixture measured optimized p50/p95 **204.516/239.985 ms** at
  **312.933 MiB/s** versus pooled streaming reference **417.810/447.743 ms**,
  a **2.0429x** throughput ratio, with one 8 MiB request buffer and zero extra
  full-chunk copies. The frozen integrated native suite passed **122 tests**,
  failed zero, and ignored five release-only benchmarks. These are local
  loopback results; live YouTube and packaged-runtime certification remain
  TASK112 scope.

# 2026-08-23 — Generation-keyed inventory and batched intake

- Remote inventory now stages versioned normalized, trailing-copy, and numeric
  title keys and promotes only a complete immutable-channel generation in one
  transaction. Channel upload/inventory generations cache a persisted duplicate
  projection; candidate keys narrow work, while the exact evidence function
  remains authoritative for exact, `(2)`, and numeric matches.
- Preflight workers materialize evidence once at completion. The webview reads
  counters, file rows, and activity through separate bounded commands and loads
  rich metadata only after one row is expanded. Watched-folder scans bulk-load
  channel observations rather than querying SQLite per discovered file.
- A reviewed multi-file import now uses one `import_and_queue_batch` bridge call
  for up to 512 paths, returns independent redacted per-item receipts, queues
  accepted items in one transaction after one inventory/title preparation, and
  starts dispatch once. A 100-path bridge regression asserts exactly one invoke.
- Frontend type checking passed, and focused bridge/large-list tests passed
  28/28, including a 10,000-record fixture that renders fewer than 100 rows.
  Native `cargo check` passed at the TASK109 checkpoint, and the final
  frozen-tree native suite passed 122 tests with zero failures and five
  release-only benchmarks ignored.
- Release fixtures passed: the 10,000-row dashboard/dedupe path measured p50
  137.866 ms and p95 149.679 ms; a 1,000-file preflight against 10,000 inventory
  rows measured compact status p50/p95 3.629/4.966 ms and a 48-file/48-activity
  page 12.162/17.844 ms. Maximum serialized page size was 21,945 bytes, below
  the 262,144-byte budget.
- This is local TypeScript, Rust, SQLite, and fixture evidence only. No live
  YouTube inventory, OAuth account, provider pagination, packaged timing, or
  production throughput is claimed; TASK112 owns those certification layers.
