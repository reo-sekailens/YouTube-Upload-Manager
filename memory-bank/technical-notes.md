# Technical Notes

## Repository facts (verified 2026-08-22)

- Before this scaffold, the repository contained only Git metadata and `.gitattributes`.
- No package manifest, source tree, CI configuration, test runner, or runtime selection exists yet.

## Engineering constraints

- Do not add an application backend, cloud queue, database, object store, analytics service, or automatic media upload outside Google/YouTube.
- Keep Google/YouTube refresh and access tokens out of source control, webview bundles, logs, and memory-bank files. Store them through platform secure storage accessed from Rust. An installed-app OAuth client ID is a public identifier, not a client secret.
- Use PKCE for installed-app OAuth. Desktop may use loopback redirects; Android and iOS must use an approved platform-specific flow because mobile loopback redirects are deprecated.
- Require least-privilege OAuth scopes. Document every requested scope and its product need before implementation.
- Treat upload retries as an idempotency problem. Persist an item identity and provider response before retrying; surface ambiguous outcomes for operator review rather than silently starting a new upload.
- Validate source type, size, and required metadata before queuing. Enforce provider quota and backoff behavior in the execution layer.
- Redact request headers, tokens, local file paths, and sensitive provider payloads from logs and errors shown to operators.
- At import, copy each queued source into an app-managed, device-local workspace and verify its BLAKE3 digest. This is the default on every platform so a crash, source-file move, revoked picker handle, or app relaunch never requires the operator to drag the item in again. Watched-folder intake is the explicit less-resilient reference-in-place exception.
- Persist the encrypted resumable session URI, total length, metadata fingerprint, and provider-confirmed byte range transactionally after every acknowledgment. On launch, query the session using an empty range request; use the returned `308` range or completed response as the sole resume authority. A `404` session expiry or an ambiguous result enters reconciliation and requires a verified match or operator-approved retry; it must not blindly create a potential duplicate.
- Native setup runs local queue reconciliation before the dashboard loads. Completed managed imports are size-checked and re-digested, resumable partial imports continue from their saved source, `dispatching` claims return to `queued`, and interrupted `uploading` items enter `needs_reconciliation` without any automatic provider request or fresh upload session.
- Streaming copy and digest helpers keep their 1 MiB buffers on the heap. Release inlining previously moved a fixed-size buffer into `reconcile_queue_impl`'s Windows GUI-thread frame and caused startup exception `0xc00000fd`; a 512 KiB-stack regression now protects the packaged startup path.
- The dashboard's channel-gated **Run dedupe** action calls the existing inventory synchronization command and then reloads the snapshot. This is the only manual trigger needed for title-based uploaded-video candidates; queue recovery has no button because it is automatic at app startup.
- Each manual dedupe run records device-local operator activity: start, channel-inventory synchronization, synchronized count, local normalized-title candidate rebuild, final candidate count, and safe errors. The UI reports only command boundaries exposed by the native layer; it never invents per-video progress, and it explicitly states that dedupe removes no video.
- Dedupe activity includes an accessible determinate three-step phase bar: inventory synchronization, local candidate rebuild, and review readiness. Its values are phase completion, not a fabricated count or elapsed-time estimate; failure visibly remains incomplete.
- The desktop CSP permits `frame-src https://www.youtube.com` solely for the lazy duplicate-comparison player frames. A separate Tauri WebviewWindow opens directly to YouTube for operator sign-in and has no application capability because only `main` is capability-scoped. The app does not access credentials or browser data; individual video embed and third-party-cookie availability remains controlled by YouTube.
- Duplicate comparison playback uses icon-only buttons with visible browser tooltips and accessible names for shared back 10 seconds, play/pause, and forward 10 seconds. Seeks are dispatched to both player frames and clamp to 0 through 86,400 seconds.
- OAuth uses the Google-installed-app PKCE loopback flow. Each operator creates a Google Cloud project they control, enables the YouTube Data API, configures the consent screen, creates a Google **Desktop** OAuth client, and imports its downloaded JSON. Rust validates the `installed` client shape, keeps its client ID in SQLite, and retains its optional client secret solely in the OS credential store. The JSON contents, authorization codes, verifiers, and token responses never enter the webview or audit log.
- First-open setup is a dismissible, local guide that stays visible on later launches until the safe `oauthConfigured` status becomes true. Its only remote actions open fixed Google account or Cloud Console addresses in dedicated unprivileged WebviewWindows; it cannot create or configure a Google resource on the operator's behalf.
- After launching the system browser, the connection panel polls the native connection receipt once per second until it changes. This resolves the callback outcome even when the platform opener does not resolve promptly.
- Separate local checks from live certification. A real upload canary needs an authorized non-production-safe destination/account and explicit operator approval.
- Watched-folder uploads are opt-in and bound to the active channel recorded at enable time. The operator selects private or unlisted visibility at enable time; public automatic uploads are rejected. The native app polls every five seconds only while running and sends every direct-child supported file—whether already present or newly added—through the same size-plus-modification-time two-scan stability gate before intake. Watched-folder intake references the source file in place; it does not create a second managed-media copy. The source must remain unchanged and available through provider confirmation.
- After the watched-folder light title gate clears, the persisted resumable upload may start immediately while a separate native BLAKE3 verification worker reads the stable source. The worker state is durable and restarts after a crash. A completed local digest match cancels only an unfinished upload; if YouTube completed first, the app records an explicit-review duplicate rather than deleting a remote video automatically. BLAKE3 does not expose a stable serializable streaming state, so an interrupted individual source is safely re-read from its persisted path rather than serializing crate internals.
- Desktop watched-folder duration validation prefers the fast ISO-BMFF header path and uses bundled FFprobe only as a 15-second bounded fallback. A stalled or unsupported probe cannot hold the persistent observation in `processing`; it is terminated and the normal light-gated queue handoff continues.
- Native intake rejects a file before managed copying or queue creation when it exceeds YouTube's published 256 GB maximum or its detected duration exceeds 12 hours. Desktop derives duration through the bundled hidden FFprobe sidecar with ISO-BMFF fallback; Android and iOS use the ISO-BMFF path available without a sidecar. Manual partial-import notices retain the actionable limit reason, while watched-folder observations are marked rejected rather than retried indefinitely.
- Existing persisted baseline observations migrate to the ordinary observed state during a scan, so older monitor configurations also resume automatic intake without an operator-only **Process existing files** action. Stable files still use current-inventory duplicate checks, direct source reference, resumable queue, and source-cleanup safeguards before dispatch.
- Every upload path uses one native light-dedupe gate: manual single-file and batch intake, queued retries/resumes, and watched folders. It re-syncs processed YouTube titles before dispatch, compares normalized separators/duplicate-copy suffixes/capture sequences, and also compares other titles in the same local batch or active channel queue. A watched-folder light match is withheld before source hashing and queue creation. Potential false positives remain an explicit, persisted Upload anyway choice; failed synchronization blocks dispatch rather than uploading blind.
- Unless the operator separately selects automatic source cleanup before queueing, an uploaded item exposes its original-file delete confirmation only after YouTube has confirmed the upload. The native command requires the exact filename, requires the `uploaded` state, rechecks the external source digest, and never deletes managed media or the YouTube video.
- Manual watched-folder scans persist a `scanning` receipt and continue on a native worker, so the webview immediately releases its controls. Queue refreshes are best-effort presentation updates and never hold folder-monitor controls busy; native scan failure is retained as a safe retryable monitor status.
- A confirmed workspace exit invokes a dedicated native `AppHandle::exit(0)`
  command. This is intentionally separate from a title-bar close request,
  which the webview intercepts to show its confirmation; recovery mode leaves
  that normal interception disabled so the window can close without clearing
  the crash marker.
- Folder discovery is non-recursive and ignores directories, symlinks, hidden files, temporary/download names, zero-byte files, and unsupported extensions. Accepted files pass the persisted light title gate before the private/unlisted resumable uploader can start; a separate native BLAKE3 worker verifies the stable watched source concurrently. A match in the last synchronized channel-title inventory, including a trailing `(2)` or higher variant, is conservatively withheld from automatic upload for review.
- Monitor configuration, observations, dispatch claims, and channel-scoped audit context are device-local SQLite records. Disable stops discovery without deleting source media, managed copies, or already queued work; no daemon, cloud scheduler, or telemetry service is introduced.
- Manual upload visibility is a per-item device-local value constrained to `private`, `unlisted`, or `public`, with `private` as the migration and import default. It can change only before queueing, is audited, and is sent only in the native resumable-session metadata. Watched-folder items remain private or unlisted only.
- Completed manual intake batches queue and begin their saved native upload work immediately when the selected YouTube channel is connected; without a connection the managed copies remain safe local drafts until the operator connects. Watched-folder files follow the same automatic dispatch after their two-scan stability gate.
- When the YouTube upload API returns a recognized `quotaExceeded`, `dailyLimitExceeded`, or `uploadLimitExceeded` response, the native queue stores a 24-hour device-local dispatch pause without persisting provider payloads. The affected item returns to `queued`, queued work is not sent during the pause, and the local worker resumes it after expiry or at the next app start.
- Current and batch upload ETAs are projections of only the latest server-acknowledged transfer rate. Before a confirmed rate exists, the UI says it is calculating rather than inventing a duration. The webview polls a running queue once per second; provider interactions remain native-only.
- Native file drag-drop supplies filesystem paths directly to the same managed-local import command used by the picker. Both UI and Rust restrict intake to supported video extensions, and the Rust command remains the enforcement boundary.
- Pre-ingest duplicate checking deliberately uses Tauri's cross-platform dialog
  and filesystem layers: desktop accepts drag/drop paths, while Android and iOS
  use the document picker and its platform URI/handle. The native layer streams
  every selected non-empty file into BLAKE3 without creating an upload item or
  exposing file bytes to the webview. The existing ingest boundary still decides
  which formats can be copied into the managed upload workspace. The UI tracks
  each preflight run so a later drop cannot be overwritten by an earlier result;
  iOS security-scoped file access ends immediately after the streaming hash.
- Both video intake and pre-ingest duplicate checking request `multiple: true`
  from the native document picker. A shared result normalizer preserves all
  returned desktop paths, Android content URIs, and iOS file URIs, while a
  cancelled picker produces no native operation.
- Exact desktop pre-ingest matches receive a short-lived opaque native deletion
  token. The webview only receives that token and a filename; before permanent
  deletion Rust requires the exact filename, rejects app-managed media paths,
  reuses the accepted opt-in duplicate review without re-hashing, then records
  a local audit event without the source path.
- A desktop pre-ingest source can also enter that same guarded local-deletion
  flow when its normalized filename matches the active channel's persisted
  YouTube title inventory. Native preparation rechecks the current active
  channel's match before issuing a token; title evidence never deletes a file
  automatically.
- Cross-device transfer uses a versioned gzip JSON archive with only upload
  title/file-name/size/BLAKE3 metadata and remote YouTube inventory. Imported
  uploads are explicitly `metadata_only`; they contain no media path and cannot
  be resumed or dispatched. The archive excludes all tokens, client secrets,
  source paths, managed media, session URLs, and local audit history, and its
  compressed and decompressed input limits are both 16 MiB.
- Pre-ingest duplicate work is dispatched through Tauri's blocking worker pool.
  This keeps the native event loop responsive while hashing large desktop drops,
  querying local duplicate records, and optionally refreshing YouTube inventory.
- Pre-ingest checks are persistent device-local jobs. Light mode publishes
  filename results first, then records optional FFprobe metadata once per file
  on a separate native worker; result reloads reuse that persisted metadata and
  never relaunch FFprobe. It retains basic filesystem facts and a small
  ISO-BMFF duration-header read while enrichment is pending. Deep mode streams
  BLAKE3 one source at a time, checkpointing after every source and resuming
  unfinished jobs at launch. Native source locators remain in SQLite and are
  not exposed to the webview. Remote inventory sync stages all pages and
  replaces the previous inventory atomically only after success, preserving the
  last known complete snapshot across a crash or failed network call.
- Crash recovery is deliberately operation-specific. Managed imports resume from
  their partial local copy; queued uploads resume only when the protected
  YouTube resumable-session checkpoint survives and then start at the
  provider-confirmed byte range. Uploads with no safe checkpoint remain in
  explicit reconciliation. Watched-folder observations, pre-ingest jobs,
  inventory staging, and import records are persisted before their work runs.
  Remote deletion writes an `executing` checkpoint before the DELETE request;
  a restart changes it to `needs_reconciliation`, requiring a new typed-ID
  confirmation rather than assuming completion or blindly retrying. Portable
  archive import is transactional and export is written/synced to a temporary
  file before publication, never overwriting an existing archive.
- Queue clearing is cancellation, not destructive cleanup: upload items become
  `cancelled` while their managed media and resumable evidence remain local;
  pre-ingest jobs become `cancelled` and workers stop before subsequent files
  or inventory work; pending/recoverable deletion requests become `cancelled`
  without contacting YouTube. Each action has a device-local audit receipt.
- The webview process is presentation-only. Disk streaming, hashing, managed
  import/recovery, folder scans, SQLite-heavy archive transfer, and all YouTube
  requests run in dedicated native worker threads. Tauri commands schedule the
  worker work and return results without occupying the UI command path.
- Windows taskbar icon source of truth is `src-tauri/icons/icon.png`. Regenerate the
  `.ico` and platform variants with `npx tauri icon src-tauri/icons/icon.png` before
  building an installer; Tauri's Windows bundle embeds `icons/icon.ico`, so updating
  only the PNG leaves the installed taskbar icon stale.
- **Desktop FFprobe sidecar:** desktop build preparation downloads the pinned
  `eugeneware/ffmpeg-static` b6.1.1 FFprobe artifacts for Windows x64, Linux
  x64, and macOS x64/arm64, validates each published SHA-256, and bundles the
  matching executable plus its supplied license. The utility is used only by
  the native desktop metadata reader; it never receives media bytes from the
  webview. Windows launches use `CREATE_NO_WINDOW`, preventing console windows
  from taking focus during background enrichment. The upstream provenance is FFmpeg n6.1.1 at
  `https://github.com/FFmpeg/FFmpeg/tree/n6.1.1`. FFprobe's GPLv3-or-later
  terms are compatible with the app's AGPL-3.0-or-later distribution. Android
  and iOS never package or execute the sidecar; their metadata behavior uses
  native/mobile-safe fallbacks.

## Scaffolded conventions

- Root `AGENTS.md` defines repository-specific agent guidance.
- `.env.example` contains names and comments only; real environment files are ignored.
- `.codex/instructions.md` supplies a compact AI-agent checklist.
- Pull-request and feature templates prompt for validation and provider/security impact.

- **Application identity:** The product uses the Tauri identifier and OS
  secure-store service namespace `com.sekailens.youtube-upload-manager`.
  It is intentionally distinct from the retired `ph.furries` identity.
- The About and support tab uses a native Markdown diagnostic-report command.
  It reports app/build/OS/architecture metadata, safe connection booleans,
  queue-status totals, a timestamp-only native panic marker, and at most 30
  local audit events. Before copying, details containing secrets, OAuth data,
  URLs, account IDs, local paths, or provider payloads are redacted. The report
  is copied only to the local clipboard and is never sent automatically.
- The About tab opens the public Wiki and project website through the system
  browser. Those links are fixed project destinations and do not include or
  transmit diagnostic content.
- The About and crash-recovery **Report to GitHub** actions open the
  repository's new-issue page with the complete redacted report already filled
  into the body and also copy it locally as a backup. They never submit an
  issue; GitHub submission remains an explicit operator action.
- Release identity is native build metadata: the version comes from Cargo and
  the channel is `regular` unless a build explicitly sets `APP_RELEASE_CHANNEL`.
  The nightly workflow sets it to `nightly` for every published artifact. About
  and copied reports use the same native command so their release labels agree.
- Native panics and webview failures create separate timestamp-only recovery
  markers. React error boundaries plus global error and unhandled-rejection
  listeners show recovery immediately where the webview remains alive; an
  unacknowledged native or webview marker is queried before the workspace and
  produces the same recovery screen on the next launch. Acknowledgement is an
  explicit local action after the operator can copy the redacted issue report
  or open the repository's new-issue form.
- Recovery mode identifies only a safe, fixed failure category (native panic,
  webview error, unhandled promise rejection, or React render error). It never
  displays a raw exception message because such messages can contain sensitive
  local or provider data.
- Every blocking dialog uses a separate sibling backdrop with a lower z-index;
  dialog content never owns or sits beneath its own dimmer. Playlist creation
  is an explicit native YouTube operation that makes a private playlist and
  returns only its safe ID/title for immediate selection in upload review or
  watched-folder configuration.
- The ordinary YouTube connection now requests `youtube.force-ssl` in addition
  to upload and read-only access: private playlist creation requires management
  permission. Existing device connections must reconnect once before using
  playlist creation; no playlist is created without an explicit click.

## Conventions to establish with implementation

- Add Tauri 2, React + Vite, Rust, SQLite, OS secure-storage integration, formatter, linter, type checking, tests, and CI.
- Record package manager, runtime version, platform build prerequisites, validation commands, signing, and release workflow here when scaffolded.
