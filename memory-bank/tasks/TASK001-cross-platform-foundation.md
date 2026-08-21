# TASK001 — Cross-platform application foundation

## Status

`in-progress`

## Objective

Define and build the first production-shaped version of YouTube Mass Uploader: a local-first native application that runs on the operator's device, securely connects an operator's YouTube channel, batch-uploads videos directly from local storage, inventories channel uploads, identifies duplicate candidates, and lets the operator safely review and delete selected videos.

The selected framework is **Tauri 2 with React + Vite**: a shared TypeScript interface runs in Tauri's native webview, while Rust owns all privileged local operations. One codebase targets Windows, macOS, Linux, Android, and iOS. The app has no application-controlled backend or cloud dependency; Google OAuth and YouTube are the sole required network services.

## Product boundaries

### In scope

- Signed Tauri application packages for current Windows, macOS, Linux, Android, and iOS devices.
- Google OAuth connection, channel selection/verification, disconnect, and data-deletion controls.
- Multiple explicitly scoped YouTube account connections, even if the first release has one operator.
- Batch intake through file selection, drag and drop where supported, and CSV metadata import.
- Bulk metadata editing, validation, thumbnail selection, privacy, scheduling, audience, subscriber-notification, and synthetic-media fields.
- Managed, device-local ingestion of every queued source followed by direct resumable uploads to YouTube, with crash-safe queue recovery, progress, retry, pause/cancel where technically safe, and actionable errors.
- Channel inventory synchronization and operator-visible duplicate-candidate groups.
- Explicitly confirmed deletion of operator-selected YouTube videos, with an audit receipt.
- Local SQLite data, OS secure-storage tokens, quota visibility, logs, notifications, privacy controls, accessibility, and recovery workflows.

### Non-goals

- Claiming support for obsolete operating systems or guaranteeing upload progress while a mobile operating system suspends or terminates the app.
- Application APIs, cloud workers, cloud databases, object stores, telemetry pipelines, or copies of source media outside the device.
- Downloading source videos back from YouTube or bypassing YouTube restrictions, quotas, copyright checks, or API audits.
- Treating metadata similarity as proof that two remote YouTube videos have identical media.
- Automatically deleting a video based only on a duplicate score.
- Engagement automation, account farming, quota sharding, or unattended publishing without prior operator approval.

## Architecture decision

Use Tauri 2, React + Vite, and Rust with these explicit boundaries:

1. **React + Vite interface:** batch preparation, metadata editing, review, inventory, duplicate review, deletion confirmation, progress, and accessible desktop/mobile layouts. It has no token access and only invokes narrowly allowed Tauri commands.
2. **Rust application core:** validates webview input, owns filesystem handles, runs the local queue, persists records, and exposes a minimal command surface.
3. **OS secure storage:** retains refresh tokens and encryption keys outside the webview and plaintext local files.
4. **Local SQLite database:** stores canonical account connections, channels, upload batches/items/attempts, inventory snapshots, duplicate candidates, deletion requests, and audit events on the device.
5. **Managed local media workspace:** imports each queued file into app-private device storage, retains an immutable digest and optional source locator, and prevents queued work from depending on a picker handle or an external path that can disappear.
6. **Local job engine:** uploads the managed local copy directly to YouTube using resumable sessions, applies retry/backoff, persists provider-confirmed checkpoints transactionally, reconciles interrupted work at launch, polls processing, synchronizes inventory, and executes approved deletions while the operating system allows the app to run.
7. **YouTube adapter:** the only component permitted to call Google OAuth and YouTube APIs; it normalizes and redacts provider responses and errors.

Do not introduce a remote component to work around mobile background limits. Instead, show a truthful “keep app open” requirement and resume from locally persisted state when the device returns. The selected framework must use least-privilege Tauri capabilities and command scopes.

## Domain and security invariants

- Every connection, batch, item, managed local asset, provider call, duplicate candidate, deletion request, and audit event carries an immutable operator and channel scope.
- OAuth access and refresh tokens never enter webview storage, client logs, task files, analytics, or plaintext local files; Rust accesses them through OS secure storage.
- Request upload and inventory-read scopes for normal connection. Because installed-app OAuth does not support true incremental authorization, require a fresh, explicitly explained re-authorization including deletion scope before any permanent deletion can be executed.
- Use a cryptographic digest of each local source plus stable item identity to prevent duplicate submission within this application's ledger.
- Save the YouTube video ID and provider response before considering an upload complete or retrying an ambiguous request.
- Ingest each queued source into a managed device-local workspace before queueing. Persist the SHA-256 digest, file size, metadata fingerprint, and original locator where available; do not require another drag-and-drop after a restart, move, or lost picker permission.
- Persist the encrypted resumable session URI, total byte length, and provider-confirmed byte offset after every accepted range. On restart, query the provider session and resume from its answer, never from an assumed local offset.
- Source media never reaches application-controlled remote storage. The managed local copy is removed only by an explicit operator action or a documented local-retention choice after a terminal result.
- Destructive actions are fail-closed. A deletion requires a fresh authorization check, exact channel ownership check, explicit selection, a second confirmation showing title and video ID, and an append-only receipt.
- Offer “make private” as a reversible alternative before permanent deletion. Never describe YouTube deletion as recoverable.
- Logs redact tokens, signed URLs, local paths, media names when sensitive, and raw provider payloads.

## Duplicate-detection contract

Duplicate results are candidates with visible evidence and confidence, never automatic verdicts:

| Tier | Evidence | Meaning | Default action |
| --- | --- | --- | --- |
| Exact local | Same SHA-256 digest in this application's upload ledger | Byte-identical managed local source previously handled by the app | Block upload pending operator override |
| Strong remote candidate | Owner-only YouTube file details align, such as filename, file size, and duration | Likely duplicate; YouTube does not expose a source checksum | Review side by side |
| Metadata candidate | Normalized title and duration align within documented tolerances | Possible duplicate with limited evidence | Review only |

Inventory begins with the authenticated channel's uploads playlist and paginates every item. Video details are fetched in batches and refreshed according to an explicit cache policy. Missing/deleted/private results remain explainable rather than silently disappearing.

## Delivery phases

### Phase 0 — Decisions and executable foundation

- Create the Tauri 2 + React/Vite workspace with a Rust core, pinned toolchain, formatting, linting, type checking, unit tests, secret scanning, dependency review, CI, and environment validation.
- Write architecture decision records for local SQLite, OS secure storage, OAuth handlers, file-access permissions, signing, update strategy, and test strategy.
- Define supported operating-system versions and a physical-device test matrix.

Exit criteria: a clean checkout builds a local application, passes all checks, requires no application service to start, and keeps production secrets out of `.env.example`.

### Phase 1 — Authentication and account ownership

- Implement application sign-in if more than one operator is supported.
- Add installed-app Google OAuth authorization-code flow with PKCE, state validation, platform-specific redirect handling, OS-secure token storage, refresh/revocation, and re-consent handling.
- Resolve the authenticated channel through `channels.list(mine=true)` and persist an account-scoped connection receipt.
- Add connect, reconnect, disconnect, revoke, export, and delete-my-data flows.

Exit criteria: one local account connection cannot act through another; tokens remain outside the webview; revoked/expired consent fails closed with a useful recovery path.

### Phase 2 — Local batch preparation and persistence

- Add responsive file intake, metadata templates, bulk edit, CSV import/export, validation, ordering, and review.
- Ingest each selected source into the managed local workspace before it enters the queue; preflight available device storage, verify the completed SHA-256 digest, and persist a crash-safe import state so an interrupted import can resume or be repaired.
- Persist immutable file identity, metadata snapshots, original locator when available, and local workspace location in SQLite. A desktop reference-in-place mode, if ever added, must be opt-in and disclose that moving the source weakens recovery.
- Implement local records, progress, cancellation, integrity checks, account isolation, backup/export, and cleanup without copying media to remote storage.
- Model batch/item/attempt state transitions and idempotency keys.

Exit criteria: force-closing the app during import or queueing preserves or repairs each item from local state without another drag-and-drop; invalid items never queue; repeated submission cannot silently create a second item.

### Phase 3 — YouTube upload execution

- Start and encrypt-persist YouTube resumable-upload sessions before sending upload bytes; stream the managed local file through the Rust job engine directly to YouTube.
- After every `308` response, transactionally persist the provider-confirmed range. On launch after any crash or forced close, query the session with an empty range request, record a returned completed response, or resume exactly at the provider-confirmed next byte; honor retry guidance and exponential backoff.
- If the session has expired or the app crashed after an ambiguous final request, enter **needs reconciliation**. Search the authenticated channel for a verifiable prior result and require operator approval before a new upload session; never automatically create a possible duplicate.
- Support required metadata and deliberate privacy/scheduling choices, including made-for-kids and synthetic-media declarations.
- Track upload, YouTube processing, success, rejection, cancellation, and ambiguous outcomes independently.
- Surface current provider limits and quota use without attempting quota sharding.

Exit criteria: a forced-close test at import, session creation, mid-chunk, final response, and app relaunch resumes or reconciles every item without dragging it in again or silently duplicating a completed upload; every terminal item has a redacted local attempt history and operator-facing result.

### Phase 4 — Inventory and duplicate candidates

- Sync the channel's uploads playlist and retrieve owner-authorized video details in bounded batches.
- Implement the three-tier duplicate contract with explainable evidence, deterministic grouping, dismiss/keep decisions, and rescan behavior.
- Preserve operator decisions without mutating YouTube content.

Exit criteria: fixture tests cover pagination, unavailable items, near matches, false positives, repeated scans, and cross-channel isolation; the UI never labels a metadata-only match as exact.

### Phase 5 — Safe removal workflow

- Add selection and side-by-side comparison with a direct YouTube link.
- Provide “keep both,” “dismiss candidate,” and “make private” before permanent deletion.
- Require second confirmation for each destructive batch and revalidate ownership immediately before every provider request.
- Execute `videos.delete` only for the confirmed IDs; use per-item results so partial failures are recoverable and visible.

Exit criteria: no deletion occurs through a preview/dry-run path; stale selection or channel mismatch blocks execution; success records the API `204` result without claiming recoverability.

### Phase 6 — Cross-platform product quality

- Complete keyboard, screen-reader, contrast, touch-target, reduced-motion, responsive, offline-shell, reconnect, and empty/error-state behavior.
- Test signed packages on representative Windows, macOS, Linux, Android, and iOS devices; document operating-system, webview, file-picker, background-execution, and OAuth constraints.
- Add quota/usage status, local completion notifications, local diagnostics export, privacy policy, terms links, local-data deletion, and operational runbooks.

Exit criteria: critical flows pass automated end-to-end tests plus the documented physical-device matrix; each supported platform runs without an application-controlled cloud service.

### Phase 7 — Live certification and release

- Configure separate development and production Google Cloud projects, platform-specific OAuth clients and redirect handlers, OAuth consent screens, app-signing identities, and provider review/audit artifacts.
- Run an explicit test-channel canary for connect, private upload, resume, processing result, inventory sync, duplicate suggestion, privacy change, deletion, revocation, and retention cleanup.
- Capture quota, provider, deployment, rollback, monitoring, backup/restore, and incident evidence.

Exit criteria: local/CI success and live-provider certification are reported separately; public publishing remains blocked until the API project has the required YouTube compliance audit.

## Acceptance criteria

- [ ] One signed local application codebase completes the core workflow on supported Windows, macOS, Linux, Android, and iOS devices.
- [ ] The operator can connect the intended YouTube channel and can clearly see the active channel before any write action.
- [ ] A reviewed batch persists locally and resumably uploads multiple videos with per-item metadata, progress, results, and safe retries.
- [ ] Duplicate candidates are explainable and confidence-labeled; exact means an application-known digest match only.
- [ ] No deletion is automatic; every permanent deletion has fresh scope validation, explicit confirmation, per-item outcome, and audit receipt.
- [ ] Credentials remain OS-secure-store-owned, inaccessible to the webview, redacted from logs, revocable, and isolated by account.
- [ ] Every queued source is available from the managed device-local workspace after a crash, forced close, source move, or app relaunch; the operator never needs to drag an already queued item in again.
- [ ] Source video never leaves the device except through the explicit direct upload to YouTube; local data deletion and export are operator-controlled.
- [ ] A persisted upload session is reconciled against YouTube after a crash and resumes from its provider-confirmed byte range. Expired or ambiguous sessions fail safely into reconciliation rather than an automatic duplicate retry.
- [ ] Quota exhaustion, expired OAuth, suspended mobile sessions, app relaunches, partial failures, and provider ambiguity have recoverable operator paths.
- [ ] Unit, integration, security, accessibility, native UI, and end-to-end checks run in CI; live test-channel evidence exists separately.
- [ ] Privacy, YouTube API policy, community-guideline certification, data deletion, support, and provider audit requirements are documented before release.

## Dependencies

- A Google Cloud project with YouTube Data API enabled.
- OAuth consent-screen configuration, platform-specific client registrations, approved redirect handlers, and authorized test channel.
- Native build prerequisites, platform signing identities, and app-store accounts where distribution requires them.
- YouTube API compliance audit before non-private uploads from an applicable unverified API project.

## Rollout and rollback

- Release signed packages in this order: connect/read inventory, local batch preparation, private upload, duplicate review, make-private, permanent deletion, broader visibility/scheduling.
- Keep permanent deletion disabled in production until live test-channel certification succeeds.
- Roll back by disabling destructive commands and queue admission in the next signed release; allow in-flight uploads to reach a known local state, retain local audit evidence, and revoke affected sessions if needed. No remote media cleanup is required.

## Validation ledger

| Date | Check | Result |
| --- | --- | --- |
| 2026-08-22 | Plan reviewed against current official YouTube upload, resumable upload, inventory, video-details, delete, quota, OAuth, and developer-policy documentation | Task ready; implementation and live-provider validation not started |
| 2026-08-22 | Architecture revised by user requirement | Tauri 2 + React/Vite local-first application selected; application-controlled remote services explicitly out of scope |
| 2026-08-22 | Crash-safe queue requirement | Managed local asset workspace and provider-authoritative resume checkpoints required; no re-drag for existing queued items |
| 2026-08-22 | Foundation implementation | Tauri/React workspace, local SQLite queue schema, responsive dashboard UI, and managed asset import created; native release executable built successfully after repairing the local Windows SDK. |
| 2026-08-22 | Import recovery proof | Rust unit test resumes an interrupted managed-media copy, verifies the final byte count and SHA-256 digest, and passes. Browser desktop and 390px mobile UI checks pass without console errors. |
| 2026-08-22 | Continuous verification | GitHub Actions now runs the web build/tests on Ubuntu and Rust formatting, queue tests, and compilation on Windows. |
| 2026-08-22 | Direct-provider implementation | Native desktop PKCE loopback connection, OS credential-store token/session storage, private direct resumable upload worker, provider-range persistence, local disconnect, and channel-gated queue start implemented. Local tests pass; no Google client/test-channel live canary has been authorized. |
| 2026-08-22 | Inventory implementation | Channel-gated native sync paginates the authenticated uploads playlist and retrieves video details in 50-item batches into the local inventory ledger; live provider evidence remains pending. |
| 2026-08-22 | Deletion review implementation | The UI and local ledger require exact typed video-ID confirmation before recording a pending deletion request; it can be cancelled, and no request currently invokes the YouTube delete endpoint without fresh deletion scope. |
| 2026-08-22 | Deletion execution implementation | A separate `youtube.force-ssl` consent path, exact second confirmation, fresh active-channel and target-video ownership validation, and HTTP-204 receipt gate now protect the direct delete command. Live provider proof remains pending. |

## Authoritative references

- [Videos: insert](https://developers.google.com/youtube/v3/docs/videos/insert)
- [Resumable uploads](https://developers.google.com/youtube/v3/guides/using_resumable_upload_protocol)
- [Retrieve a channel's uploaded videos](https://developers.google.com/youtube/v3/guides/implementation/videos#retrieve_a_channels_uploaded_videos)
- [Video resource and owner-only file details](https://developers.google.com/youtube/v3/docs/videos)
- [Videos: delete](https://developers.google.com/youtube/v3/docs/videos/delete)
- [YouTube Data API quota calculator](https://developers.google.com/youtube/v3/determine_quota_cost)
- [Google OAuth 2.0](https://developers.google.com/identity/protocols/oauth2)
- [OAuth 2.0 for iOS and desktop apps](https://developers.google.com/identity/protocols/oauth2/native-app)
- [YouTube developer-policy guidance](https://developers.google.com/youtube/terms/developer-policies-guide)
- [Tauri 2 platform support](https://v2.tauri.app/)
