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
- At manual and watched-folder intake, persist an original device-local source reference with its stable signature and BLAKE3 digest; do not create an app-managed `.media` or `.partial` copy or a copy fallback. Before dispatch or resumable recovery, revalidate that original source. If it moved, is unavailable, or changed, fail safely/reconcile rather than uploading replacement bytes. This trades the former guarantee against source moves/deletion for bounded device storage.
- Persist the encrypted resumable session URI, total length, metadata fingerprint, and provider-confirmed byte range transactionally after every acknowledgment. On launch, query the session using an empty range request; use the returned `308` range or completed response as the sole resume authority. A `404` session expiry or an ambiguous result enters reconciliation and requires a verified match or operator-approved retry; it must not blindly create a potential duplicate.
- Native setup is deliberately database-only: it initializes/migrates SQLite once, returns `dispatching` claims to `queued`, marks interrupted uploads and deletions for reconciliation, and resets durable local-job markers in one immediate transaction. It performs no media read, FFprobe, network request, secure-session lookup, or destructive media cleanup during this classification. React renders a safe shell first; after two animation frames, one bounded coordinator resumes source-reference validation, secure resumable-session resolution, watched hashes, preflight work, and legacy cleanup. Interrupted rows share one SQLite connection, and deliberately unscoped rows remain fail-closed without an OS secure-store lookup. Legacy cleanup scans for owned UUID-shaped media first and opens SQLite only when a candidate exists; it then migrates legacy items to a surviving source path or marks unavailable-source items failed before deleting owned `.media` and `.partial` files. Upload controls and native dispatch remain fenced until both phases complete and the active immutable channel matches.
- TASK112's `interrupted-256gb` packaged fixture exists only behind the native
  `performance-harness` feature plus a separate seed-only environment gate. An
  untimed process commits one channel-less `uploading` row declaring
  256,000,000,000 bytes, writes zero media bytes, and exits before recovery.
  The Windows runner clones that closed, marker-protected template for every
  launch and removes both fixture variables from measured processes, which run
  the ordinary database-only classification. Regular builds ignore the fixture
  variables. Clone/template deletion requires the marker and verified
  containment beneath the disposable root.
- Streaming buffers stay on the heap. The independent read-only digest path
  uses an 8 MiB sequential-read buffer; the legacy resumable copy path uses a
  1 MiB buffer with standard opens after healthy-HDD paired evidence rejected
  both tested sequential-scan copy variants.
  Release inlining previously moved a fixed-size buffer into
  `reconcile_queue_impl`'s Windows GUI-thread frame and caused startup exception
  `0xc00000fd`; a 512 KiB-stack regression protects the packaged startup path.
- The dashboard's channel-gated **Run dedupe** action calls the existing inventory synchronization command and then reloads the snapshot. This is the only manual trigger needed for title-based uploaded-video candidates; queue recovery has no button because it is automatic at app startup.
- Browser-rendered large-list surfaces mount 32 records per page. A dedicated
  local-only interaction harness runs the real Batch queue against 10,000
  synthetic in-memory rows, measures request-to-painted search and clear
  responses after the deferred value resolves, and records Long Tasks API
  entries within each sample. Its evidence is browser-only and must not be
  promoted to packaged WebView2 or live-provider proof.
- The safe shell remains mounted until its real DOM marker has painted across
  two animation frames and the native performance recorder acknowledges the
  `safe_shell_paint` milestone. Deferred startup completion is invoked only
  after that receipt; unknown/rejected milestone calls fail the harness rather
  than silently producing a missing timing.
- The authoritative unsigned empty-profile package uses two reversed 40-run
  blocks, five untimed warmups, isolated WebView2 data per clone, nearest-rank
  percentiles, raw chronological receipts, and no outlier removal. All 160
  measured launches recorded zero deltas for periodic invokes, database opens,
  SQLite statements, event messages, worker threads, and FFprobe processes in
  the bounded two-second settled-idle window.
- The matching final-v3 interrupted-256 GB package uses one channel-less
  synthetic `uploading` row declaring 256,000,000,000 bytes and writes zero
  media. Across 80 cold and 80 warm launches every safe-shell value was
  present, SQLite classified the row into `needs_reconciliation`, and all six
  settled-idle deltas were zero. Cold safe-shell p50/p95 changed
  -2.23%/-9.14%, and cold first-Batch p50/p95 changed +5.12%/+4.08%, versus
  the final-v3 empty profile; both remain within the 10% materiality criterion.
- Each manual dedupe run records device-local operator activity: start, channel-inventory synchronization, synchronized count, local normalized-title candidate rebuild, final candidate count, and safe errors. The UI reports only command boundaries exposed by the native layer; it never invents per-video progress, and it explicitly states that dedupe removes no video.
- Dedupe activity includes an accessible determinate three-step phase bar: inventory synchronization, local candidate rebuild, and review readiness. Its values are phase completion, not a fabricated count or elapsed-time estimate; failure visibly remains incomplete.
- The desktop CSP permits `frame-src https://www.youtube.com` solely for the lazy duplicate-comparison player frames. A separate Tauri WebviewWindow opens directly to YouTube for operator sign-in and has no application capability because only `main` is capability-scoped. The app does not access credentials or browser data; individual video embed and third-party-cookie availability remains controlled by YouTube.
- Duplicate comparison playback uses icon-only buttons with visible browser tooltips and accessible names for shared back 10 seconds, play/pause, and forward 10 seconds. Seeks are dispatched to both player frames and clamp to 0 through 86,400 seconds.
- OAuth uses the Google-installed-app PKCE loopback flow. Each operator creates a Google Cloud project they control, enables the YouTube Data API, configures the consent screen, creates a Google **Desktop** OAuth client, and imports its downloaded JSON. Rust validates the `installed` client shape, keeps its client ID in SQLite, and retains its optional client secret solely in the OS credential store. The JSON contents, authorization codes, verifiers, and token responses never enter the webview or audit log.
- First-open setup is a dismissible, local guide that stays visible on later launches until the safe `oauthConfigured` status becomes true. Its only remote actions open fixed Google account or Cloud Console addresses in dedicated unprivileged WebviewWindows; it cannot create or configure a Google resource on the operator's behalf.
- After launching the system browser, connection/deletion authorization uses the
  native state event as its primary receipt. A bounded exponential fallback runs
  only during the active authorization attempt and then stops; settled idle has
  no authorization invoke timer.
- Separate local checks from live certification. A real upload canary needs an authorized non-production-safe destination/account and explicit operator approval.
- Watched-folder uploads are opt-in and bound to the active immutable channel
  recorded at enable time. The operator selects private or unlisted visibility
  at enable time; public automatic uploads are rejected. An enabled monitor uses
  one condition/deadline worker for the two-scan stability gate; disabling it
  wakes and terminates the worker, so disabled monitoring performs no recurring
  scan or write. Watched-folder intake references the source file in place; it
  does not create a second managed-media copy. The source must remain unchanged
  and available through provider confirmation.
- After the watched-folder light title gate clears, the persisted resumable upload may start immediately while a separate native BLAKE3 verification worker reads the stable source. The worker state is durable and restarts after a crash. A completed local digest match cancels only an unfinished upload; if YouTube completed first, the app records an explicit-review duplicate rather than deleting a remote video automatically. BLAKE3 does not expose a stable serializable streaming state, so an interrupted individual source is safely re-read from its persisted path rather than serializing crate internals.
- Desktop watched-folder duration validation prefers the fast ISO-BMFF header path and uses bundled FFprobe only as a 15-second bounded fallback. A stalled or unsupported probe cannot hold the persistent observation in `processing`; it is terminated and the normal light-gated queue handoff continues.
- Native media work now uses one shared device-local scheduler: at most two FFprobe processes run at once, and at most one copy/hash reader runs per detected source volume. Active uploads register their source volume; new probes wait behind that upload and foreground/background readers yield in short bounded intervals so media enrichment cannot monopolize the upload disk. Probe and read waits observe cancellation, FFprobe is killed on cancellation or timeout, and watched-hash cancellation reuses one SQLite connection with checks throttled to 250 ms rather than opening a database per chunk.
- Upload-limit validation uses a duration-only FFprobe request capped at 64 KiB. Rich pre-ingest metadata is capped at 2 MiB and is reused for duration only when the canonical path, byte length, and nanosecond modification key still match. The bounded 256-entry in-memory cache coalesces matching in-flight probes and does not permanently cache spawn, timeout, or cancellation failures. ISO-BMFF remains the zero-process fast path, and startup classification/recovery never enters the probe path.
- Legacy managed-copy handling exists only to classify and reclaim old
  app-created `.media`/`.partial` files safely. New intake streams BLAKE3 from
  the original reference; no platform clone, reflink, hard-link, or full-copy
  fallback is used. Cleanup never blindly removes a legacy file before the
  database proves that no live job references it.
- Release-only copy phase diagnostics keep their fields/functions under
  `cfg(test)`. On the near-full C: volume, user-space stream/write/hash was p50
  74.891 ms while the required `sync_all` was p50 934.431 ms. The durable flush
  remains mandatory: neither omitting it nor pre-extending a partial file is
  compatible with trusting byte length and digest reconstruction after a crash.
  Healthy-headroom paired evidence later rejected 8 MiB/sequential copy at
  +3.70%/+18.61% p50/p95 and 1 MiB/sequential copy at +3.79%/+10.35% versus
  their paired 1 MiB standard-open references. Production uses the fastest
  tested safe non-regressing variant, 1 MiB standard opens, while retaining
  `sync_all`, resume reconstruction, cancellation, and scheduler admission.
- The stabilized media hot path is isolated in `media_runtime`: it exclusively
  owns source-volume reader/process admission, active-upload priority guards,
  copy/hash primitives, ISO-BMFF duration parsing, FFprobe lifecycle, and the
  stable-signature probe cache. SQLite cancellation queries and product metadata
  mapping stay in `lib.rs`, so media work cannot acquire a database connection
  internally or weaken channel/audit/provider boundaries. Focused fixtures live
  beside the module; the 512 KiB-stack recovery fixture remains at the startup
  integration seam.
- No release-profile override was accepted from file movement alone. Panic
  abort and symbol stripping conflict with crash recovery/support evidence; thin
  LTO and codegen-unit changes require a complete comparable link plus startup,
  throughput, size, and small-stack results. The first isolated default cold
  build ran out of disk before link and is invalid evidence, so Cargo.toml
  remains unchanged and packaged measurements remain assigned to TASK112.
- After the TASK111 cutover and the later startup-recovery repair, the final
  isolated release performance-harness suite passed 140 tests with zero
  failures and five intentionally ignored release-only benchmarks. This is
  source/native behavior evidence, not packaged startup, installer, or
  live-provider proof.
- Nightly Windows portable packaging includes the prepared `ffprobe.exe` and
  `ffprobe-license.txt`, verifies the sidecar PE machine is x64, and verifies
  all three portable ZIP entries. Mobile configs continue to null desktop
  sidecar/resources.
- Final-v3 ordinary unsigned production evidence closes the Windows artifact-
  integrity boundary: the executable is x64 PE `0x8664`, the portable ZIP
  reads back exactly the matching app, FFprobe, and license, and the runner
  rejects the ordinary executable as non-harness. That final-v3 executable was
  not launched. An older exact-path smoke separately rendered a real 81-item
  Batch profile with the 32-row page bound and must not be promoted to current
  final-v3 runtime evidence. Signing, other platforms, and live-provider
  behavior remain separate unverified boundaries.
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
- Monitor configuration, observations, dispatch claims, and channel-scoped audit context are device-local SQLite records. Disable stops discovery without deleting source media or already queued work; legacy managed-media cleanup remains startup queue-safety gated. No daemon, cloud scheduler, or telemetry service is introduced.
- Manual upload visibility is a per-item device-local value constrained to `private`, `unlisted`, or `public`, with `private` as the migration and import default. It can change only before queueing, is audited, and is sent only in the native resumable-session metadata. Watched-folder items remain private or unlisted only.
- Completed manual intake batches queue and begin their saved native upload work immediately when the selected YouTube channel is connected; without a connection their original references remain queued only while the source stays available and unchanged. Watched-folder files follow the same automatic dispatch after their two-scan stability gate.
- When the YouTube upload API returns a recognized `quotaExceeded`, `dailyLimitExceeded`, or `uploadLimitExceeded` response, the native queue stores a 24-hour per-channel device-local dispatch pause without persisting provider payloads. The affected item returns to `queued`; one conditional worker waits until the saved deadline and exits after resuming or when no pause remains.
- Current and batch upload ETAs are projections of only the latest server-acknowledged transfer rate. Before a confirmed rate exists, the UI says it is calculating rather than inventing a duration. Every provider-confirmed range remains durable, while a fixed 100 ms native window coalesces the latest upload state into a compact revisioned event; the webview does not poll the dashboard.
- Schema v2 appends safe channel-scoped mutations to `state_changes`. The event
  envelope uses `fromRevision` as the requested cursor and `toRevision` as the
  covered global cursor. Entity revisions need not be contiguous because other
  channels are filtered and repeated entity progress is coalesced. Cursor gaps,
  reloads, and process restarts recover from SQLite; a channel mismatch is
  rejected before any entity payload enters the active projection.
- Native file drag-drop supplies filesystem paths directly to the same reference-in-place intake command used by the picker. Both UI and Rust restrict intake to supported video extensions, and the Rust command remains the enforcement boundary.
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
  Relaunch recovery uses one sequential coordinator for recovered jobs and their
  metadata instead of creating a thread or FFprobe process per persisted row.
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
- Crash recovery is deliberately operation-specific. Queued uploads resume only
  when the original source reference revalidates and the protected YouTube
  resumable-session checkpoint survives, then start at the provider-confirmed
  byte range. An unavailable/changed source or missing safe checkpoint remains
  in explicit reconciliation. Startup cleans legacy `.media`/`.partial` files
  only after the database classification proves that no live job references
  them. Watched-folder observations, pre-ingest jobs,
  inventory staging, and import records are persisted before their work runs.
  Remote deletion writes an `executing` checkpoint before the DELETE request;
  a restart changes it to `needs_reconciliation`, requiring a new typed-ID
  confirmation rather than assuming completion or blindly retrying. Portable
  archive import is transactional and export is written/synced to a temporary
  file before publication, never overwriting an existing archive.
- Queue clearing is cancellation, not destructive cleanup: upload items become
  `cancelled` while their original-source reference and resumable evidence remain local;
  pre-ingest jobs become `cancelled` and workers stop before subsequent files
  or inventory work; pending/recoverable deletion requests become `cancelled`
  without contacting YouTube. Each action has a device-local audit receipt.
- The webview process is presentation-only. Disk streaming, hashing, managed
  import/recovery, folder scans, SQLite-heavy archive transfer, and all YouTube
  requests run in bounded native workers. Startup recovery has one coordinator;
  ordinary commands schedule blocking work without occupying the UI command
  path. Conditional monitor/quota workers are not started during empty setup.
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
- Provider transport uses lazily built rustls `reqwest` pools. Control requests
  use 10 s connect/45 s request timeouts; resumable uploads use 15 s connect/30
  min request timeouts. Both pools retain idle connections for 90 s with at
  most eight idle connections per host. Setup must continue to build zero
  clients so TASK105's database-only pre-shell boundary remains measurable.
- Upload and deletion access-token grants have separate expiry-aware in-memory
  caches with a 60 s refresh skew and a refresh lock that coalesces concurrent
  callers. Never persist these access tokens or move secure-store refresh-token
  reads into the webview. Connection, disable, and revocation paths invalidate
  only the applicable grant.
- Resumable upload bodies use one exact request-owned chunk buffer per worker
  and one pooled client handle per complete session. The accepted local
  benchmark chunk is 8 MiB (a 256 KiB multiple); the hot path must not restore
  a persistent buffer plus full-body clone. Every `308` remains a durable
  checkpoint boundary before the following range.
- Final integrated local evidence for that transport is p50/p95
  **204.516/239.985 ms** and **312.933 MiB/s** for the optimized 64 MiB
  loopback fixture, versus pooled streaming p50/p95 **417.810/447.743 ms**.
  The **2.0429x** ratio passed the executable 0.90x budget with one 8 MiB buffer
  per request and zero extra full-chunk copies; it is not live-provider proof.
- Upload scheduling is bounded to four global workers and a cached safe limit
  per actual read volume. The SQLite claim is authoritative across relaunch;
  the process ledger is synchronized from `dispatching`/`uploading` rows and
  rotates the first eligible volume at each handoff. Provider success is an
  immediate transaction; playlist and guarded cleanup are durable lower-priority
  work and never retain an upload permit. Destructive remote deletion remains
  sequential and independently authorized.
- SQLite schema v4 persists `normalized_title`, `canonical_title`,
  `has_copy_marker`, `numeric_title_key`, and `title_keys_version` beside local,
  staged-remote, and active-remote records. The accepted copy rule is a trailing
  parenthesized integer of two or greater; `(1)` is not a copy marker. Numeric
  matching requires at least two numeric sequences containing at least six
  digits total. Persisted keys generate candidates only; the shared evidence
  function makes the final exact/copy/numeric decision.
- `channel_generations` tracks inventory and local-upload mutations per
  immutable channel. `duplicate_projection_state` records the generation pair
  represented by `duplicate_candidate_projection`. Reads reuse that projection
  when both generations match and rebuild it only for the changed channel.
  Remote inventory remains in `remote_video_sync_staging` until a complete
  pagination run promotes it in one transaction.
- Preflight list payloads are deliberately split: status contains counters,
  result and activity pages each clamp to 64 rows, list match evidence carries
  at most four previews per category plus full counts, and deep metadata is a
  one-record expansion command. Completed legacy jobs materialize evidence on
  first access. Source locators remain native-only.
- `import_and_queue_batch` accepts at most 512 reviewed paths per bridge call.
  Each path receives an independent success, duplicate, validation, or import
  receipt; accepted queue rows are inserted with one SQLite connection and
  transaction after one channel inventory/title preparation. The receipt omits
  local source paths and dispatch is kicked once for the accepted batch.
- Watched-folder scans load all existing observations for their immutable
  channel into memory before comparing discovered files. This prevents the
  direct-child scan from opening one SQLite query per file while preserving the
  existing no-follow, stability, digest, and channel gates.
- TASK109's release fixture measures the local SQLite/serialization boundary,
  not provider latency. At 10,000 rows, dashboard/dedupe measured p50/p95
  137.866/149.679 ms. A 1,000-file preflight against 10,000 inventory rows
  measured status p50/p95 3.629/4.966 ms and a bounded 48/48 page
  12.162/17.844 ms; its maximum serialized payload was 21,945 bytes against the
  262,144-byte budget.

## Conventions to establish with implementation

- Add Tauri 2, React + Vite, Rust, SQLite, OS secure-storage integration, formatter, linter, type checking, tests, and CI.
- Record package manager, runtime version, platform build prerequisites, validation commands, signing, and release workflow here when scaffolded.
