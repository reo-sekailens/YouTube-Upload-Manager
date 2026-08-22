# Technical Notes

## Repository facts (verified 2026-08-22)

- Before this scaffold, the repository contained only Git metadata and `.gitattributes`.
- No package manifest, source tree, CI configuration, test runner, or runtime selection exists yet.

## Engineering constraints

- The app is local-first: do not add an application backend, cloud queue, database, object store, analytics service, or automatic media upload outside Google/YouTube.
- Keep Google/YouTube refresh and access tokens out of source control, webview bundles, logs, and memory-bank files. Store them through platform secure storage accessed from Rust. An installed-app OAuth client ID is a public identifier, not a client secret.
- Use PKCE for installed-app OAuth. Desktop may use loopback redirects; Android and iOS must use an approved platform-specific flow because mobile loopback redirects are deprecated.
- Require least-privilege OAuth scopes. Document every requested scope and its product need before implementation.
- Treat upload retries as an idempotency problem. Persist an item identity and provider response before retrying; surface ambiguous outcomes for operator review rather than silently starting a new upload.
- Validate source type, size, and required metadata before queuing. Enforce provider quota and backoff behavior in the execution layer.
- Redact request headers, tokens, local file paths, and sensitive provider payloads from logs and errors shown to operators.
- At import, copy each queued source into an app-managed, device-local workspace and verify its SHA-256 digest. This is the default on every platform so a crash, source-file move, revoked picker handle, or app relaunch never requires the operator to drag the item in again. Desktop “reference in place” may be offered only as an explicitly less-resilient opt-in.
- Persist the encrypted resumable session URI, total length, metadata fingerprint, and provider-confirmed byte range transactionally after every acknowledgment. On launch, query the session using an empty range request; use the returned `308` range or completed response as the sole resume authority. A `404` session expiry or an ambiguous result enters reconciliation and requires a verified match or operator-approved retry; it must not blindly create a potential duplicate.
- Native setup runs local queue reconciliation before the dashboard loads. Completed managed imports are size-checked and re-digested, resumable partial imports continue from their saved source, `dispatching` claims return to `queued`, and interrupted `uploading` items enter `needs_reconciliation` without any automatic provider request or fresh upload session.
- Streaming copy and digest helpers keep their 1 MiB buffers on the heap. Release inlining previously moved a fixed-size buffer into `reconcile_queue_impl`'s Windows GUI-thread frame and caused startup exception `0xc00000fd`; a 512 KiB-stack regression now protects the packaged startup path.
- The dashboard's channel-gated **Run dedupe** action calls the existing inventory synchronization command and then reloads the snapshot. This is the only manual trigger needed for title-based uploaded-video candidates; queue recovery has no button because it is automatic at app startup.
- Each manual dedupe run records device-local operator activity: start, channel-inventory synchronization, synchronized count, local normalized-title candidate rebuild, final candidate count, and safe errors. The UI reports only command boundaries exposed by the native layer; it never invents per-video progress, and it explicitly states that dedupe removes no video.
- Dedupe activity includes an accessible determinate three-step phase bar: inventory synchronization, local candidate rebuild, and review readiness. Its values are phase completion, not a fabricated count or elapsed-time estimate; failure visibly remains incomplete.
- The desktop CSP permits `frame-src https://www.youtube-nocookie.com` solely for the duplicate-comparison player frames. It does not permit arbitrary frames or broaden OAuth/API connections; individual video embed availability remains controlled by YouTube.
- Duplicate comparison playback uses icon-only buttons with visible browser tooltips and accessible names for shared back 10 seconds, play/pause, and forward 10 seconds. Seeks are dispatched to both player frames and clamp to 0 through 86,400 seconds.
- OAuth uses the Google-installed-app PKCE loopback flow. When a shared Desktop client cannot complete authorization, the operator can choose a downloaded Google **Desktop** OAuth JSON; Rust validates the `installed` client shape, keeps only its public client ID in SQLite, and retains its optional client secret solely in the OS credential store. The JSON contents, authorization codes, verifiers, and token responses never enter the webview or audit log.
- After launching the system browser, the connection panel polls the native connection receipt once per second until it changes. This resolves the callback outcome even when the platform opener does not resolve promptly.
- Separate local checks from live certification. A real upload canary needs an authorized non-production-safe destination/account and explicit operator approval.
- Watched-folder uploads are opt-in and bound to the active channel recorded at enable time. The operator selects private or unlisted visibility at enable time; existing monitors migrate safely to private, and public automatic uploads are rejected. The native app polls every five seconds only while running, treats current supported files as a non-uploading baseline, and requires size plus modification time to remain unchanged across two scans before ingestion.
- Folder discovery is non-recursive and ignores directories, symlinks, hidden files, temporary/download names, zero-byte files, and unsupported extensions. Accepted files still pass through the managed local copy and SHA-256 ledger before the existing private resumable uploader can start. A match in the last synchronized channel-title inventory, including a trailing `(2)` or higher variant, is conservatively withheld from automatic upload for review.
- Monitor configuration, observations, dispatch claims, and channel-scoped audit context are device-local SQLite records. Disable stops discovery without deleting source media, managed copies, or already queued work; no daemon, cloud scheduler, or telemetry service is introduced.
- Manual upload visibility is a per-item device-local value constrained to `private`, `unlisted`, or `public`, with `private` as the migration and import default. It can change only before queueing, is audited, and is sent only in the native resumable-session metadata. Watched-folder items remain private or unlisted only.
- Completed manual intake batches queue and begin their saved native upload work immediately when the selected YouTube channel is connected; without a connection the managed copies remain safe local drafts until the operator connects. Watched-folder files follow the same automatic dispatch after their two-scan stability gate.
- When the YouTube upload API returns a recognized `quotaExceeded`, `dailyLimitExceeded`, or `uploadLimitExceeded` response, the native queue stores a 24-hour device-local dispatch pause without persisting provider payloads. The affected item returns to `queued`, queued work is not sent during the pause, and the local worker resumes it after expiry or at the next app start.
- Current and batch upload ETAs are projections of only the latest server-acknowledged transfer rate. Before a confirmed rate exists, the UI says it is calculating rather than inventing a duration. The webview polls a running queue once per second; provider interactions remain native-only.
- Native file drag-drop supplies filesystem paths directly to the same managed-local import command used by the picker. Both UI and Rust restrict intake to supported video extensions, and the Rust command remains the enforcement boundary.
- Windows taskbar icon source of truth is `src-tauri/icons/icon.png`. Regenerate the
  `.ico` and platform variants with `npx tauri icon src-tauri/icons/icon.png` before
  building an installer; Tauri's Windows bundle embeds `icons/icon.ico`, so updating
  only the PNG leaves the installed taskbar icon stale.

## Scaffolded conventions

- Root `AGENTS.md` defines repository-specific agent guidance.
- `.env.example` contains names and comments only; real environment files are ignored.
- `.codex/instructions.md` supplies a compact AI-agent checklist.
- Pull-request and feature templates prompt for validation and provider/security impact.

- **Product rename compatibility:** Present the product as **YouTube Upload
  Manager**, while retaining `ph.furries.youtube-mass-uploader` and the related
  OS secure-store service namespace. Changing either would strand locally
  persisted queues and protected OAuth credentials from existing installs.

## Conventions to establish with implementation

- Add Tauri 2, React + Vite, Rust, SQLite, OS secure-storage integration, formatter, linter, type checking, tests, and CI.
- Record package manager, runtime version, platform build prerequisites, validation commands, signing, and release workflow here when scaffolded.
