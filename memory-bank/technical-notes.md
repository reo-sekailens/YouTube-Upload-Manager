# Technical Notes

## Credential replacement and upload recovery (TASK169, 2026-10-19)

- Native connection revisions hash only channel/client/project/protected-bundle
  metadata. They expose no tokens/client JSON. Provider reads share revision-aware
  fences; consent/profile coordinators keep their own reviewed-operation fences.
- Resumable status checks use authenticated empty PUTs with a 45-second deadline;
  chunk requests refresh native access and carry Authorization. Upload hosts are
  validated before authorization; pooled provider clients reject redirects.
- Status decoding retains successful video IDs for canonical verification, checks
  range bounds/overflow, and separates quota from terminal/transient errors.
- Recovery waits use one lazily created local SQLite table with no startup schema
  migration. Keys include item, immutable channel and credential revision. Reads
  create no table; malformed/DB errors fail closed. Failure/state/audit updates
  commit together. A terminal hold retains the existing item/session for review.
- Fresh reviewed items remain dispatchable while another session is held. The
  shared deadline worker schedules future recovery and restores waits after the
  safe shell on restart; no additional resident worker or polling loop is added.
- Recovery and queued selection stream ordered rows and cap accepted candidates
  after hold/source validation. Candidate buffers/promotions stay bounded at 256;
  a prefix of older held or missing rows cannot hide later dispatchable work.
- Exact historical quota descriptions refresh only as presentation from current
  native pause records. No text match authorizes transfer or erases a hold.
- Existing explicit library/Dedupe action bodies load lazily so the shared
  credential fences fit the unchanged initial JavaScript budget (70 KiB gzip).


## Google project quota reports (TASK168, 2026-10-19)

- Read-only Cloud access requests `cloud-platform.read-only` and
  `monitoring.read` separately from YouTube authorization. Both granted scopes
  must be returned before promotion. Cancellation, expiry or any selected
  channel/client/project/bundle change fences callbacks and late results.
- Google IAM is separate from OAuth consent. The account needs
  `serviceusage.quotas.get`, `monitoring.timeSeries.list` and project metadata
  read permission (`resourcemanager.projects.get`); Service Usage Viewer plus
  Monitoring Viewer provides these reads. Service Usage/Monitoring APIs must be available. A Google
  denial produces a fixed actionable error without exposing provider payloads.
- Service Usage reports effective global project/day limits; Monitoring
  `serviceruntime.googleapis.com/quota/rate/net_usage` reports weighted consumed
  quota, not API request counts. Sum validated disjoint DELTA INT64 intervals
  since Pacific midnight across methods only for their matching quota metric.
  Unsupported dimensions/units, missing series and ambiguous midnight intervals
  remain unknown. Do not hardcode a single 10,000-unit total for every bucket.
- Monitoring samples every 60 seconds and can lag another 240 seconds. Fetch
  time and latest sample are both visible. Missing data never becomes zero;
  partial/failed pagination cannot promote a new report. Earlier reports retain
  original timestamps after failure. No automatic metric polling is enabled.
- Grant tokens and project mapping stay in OS secure storage, never SQLite,
  webview, logs or repo. Quota reports are bounded in native memory. Reauthorization
  uses the same stepped review and leaves upload/management grants intact.
- Local verification uses provider-shaped synthetic payloads and an actual
  rendered React component fixture. No signed-in Cloud IAM/API access or real
  project's remaining quota has been verified by those tests.

## Provider read budgets and queue waiting (TASK166–167, 2026-10-19)

- Full inventory cost is `1 + 2 * ceil(N/50)` list calls. The injected 5,000-video
  case uses 201; unchanged recent refresh uses one head page; new/changed head
  details use two. Warm checks and known quota holds use zero. These are local
  provider-injected counts, not measurements of a signed-in production project.
- Recent reads are merge-only and can miss changes to older videos. Fifteen-minute
  warm reuse and the 24-hour full snapshot bound are explicit freshness tradeoffs;
  manual Refresh library performs complete reconciliation. A partial/failing read
  cannot replace the last inventory or renew its successful checkpoint.
- Cache keys hash the complete selected channel/client/project/bundle metadata.
  Successful empty inventories/playlists persist across restarts; corrupt cached
  JSON is a miss. Account changes fence late results before promotion.
- Inventory retry delays are 60, 120, 240, 480 and 900 seconds, capped at 900.
  Queue admission schedules the deadline without pausing an already active byte
  transfer. Watched scans do not keep attempting provider reads during that hold.
- The shared Data API read boundary checks saved project quota before HTTP.
  Request telemetry remains local SQLite evidence and is not an API usage total.
  Explicit playlist/source mutation guards and uncertain-write readback are not
  replaced with display caches.
- Cloud quota resets use the canonical dated Pacific/DST calculation. Legacy
  deadline correction is transactional with its audit receipt and preserves the
  original observation time so repeated housekeeping never rolls the hold forward.
- Rendered queue QA uses synthetic files and events. The personal installed queue
  was inspected read-only; no personal upload, original-file mutation or live
  provider acceptance was used for validation.

## Comparison playback and monitor refresh (TASK163, 2026-10-18)

- Metadata alone is not video-picture evidence. A missing video dimension after
  metadata or playback without decoded frames selects the compatibility player;
  the operator can also choose it directly. Failures retain the duplicate
  decision and offer retry without opening an external app.
- Compatibility commands accept only an existing scoped preview token plus a
  finite source time. Native code retains original/decoder identity checks,
  revocation, expiry and channel authority. The pinned bundled decoder emits
  at most six seconds/4 MiB of 640×360 baseline H264 with optional stereo AAC
  per request, entirely in memory. Decode work is time bounded. Native raw IPC
  transports the bytes; the application adds no HTTP server or media files.
- The player serializes reads/appends, invalidates stale seek responses, keeps
  a six-second trailing/eighteen-second leading window, and revokes MediaSource
  URLs on teardown. AAC encoder padding can exceed the source duration; an
  append window clips it instead of shrinking an already buffered duration,
  which Chromium correctly rejects. Original duration is established once.
- Manual preview shares the two-process inspection budget and yields its
  original-file reader to active uploads. Background probes/perceptual work
  retain their existing upload admission behavior. Closing or changing channel
  cancels native preview work; source locators stay native-only.
- Folder-monitor reads use a 250 ms first-event queue with one in-flight and
  one trailing refresh. Ordinary events preserve mounted details and enabled
  navigation. Request fences still reject late folder/channel results. Stable
  activity keys and retained deletion-review/log wrappers preserve local state.

## Tauri dependency compatibility (TASK162, 2026-10-16)

- Core JavaScript API and Rust Tauri must share major/minor releases. The API
  now resolves to 2.12.4 (`~2.12.4`) and Rust remains at 2.12.2 (`~2.12.2`),
  required by the notification integration. Patch differences are supported;
  the independently released CLI remains 2.11.4 and plugins are unchanged.
- `npm run check:tauri-versions` checks both manifests and lockfiles, rejects
  broad ranges/minor drift and runs before production frontend builds. Run it
  after dependency changes; update both core release lines together. Its
  regression suite also runs in CI and `test:operations`. The Rust dependency
  graph did not change in this repair.
- The real Tauri CLI rejected the original locked 2.11.1/2.12.2 pair and passed
  its version gate after alignment. The verification hook deliberately stops
  before native packaging. Locked Cargo checks use the documented resource
  override harness; complete installer and live-provider proof remain separate.
- Native verification prepares both configured sidecars and decoder notices
  before Cargo builds. Initial JS/CSS and native concurrency budgets are not
  relaxed by this repair.

## Repository facts (verified 2026-10-09)

- Before this scaffold, the repository contained only Git metadata and `.gitattributes`.
- The current repository implements a React/Vite webview, privileged Tauri/Rust command layer, device-local SQLite queue, frontend/native tests, and local build/operations checks. Provider and packaged-runtime evidence is recorded separately from local source validation.

## Engineering constraints

- TASK164 adds user-requested optional yt-dlp duplicate review; ordinary
  provider/account writes remain official native APIs. Selected tool paths and
  TASK165's separately approved per-channel browser/profile metadata persist.
  Extraction disables ambient config/plugins/cache, remote components, exported
  cookie jars and default runtimes; optional Deno/Node selection is explicit.
  Browser cookies are disabled unless the current channel has an approved
  profile. Cookies and signed provider details remain native-only.
  Progressive H264/AAC MP4 is the first direct format; other formats/access
  failures and mobile retain the iframe. No app OAuth is handed to the tool.
- Remote grants require current evidence/channel, expire after 20 minutes and
  share the main-only media-preview protocol. Two extractors maximum, 45-second
  and 2 MiB output caps; pooled HTTPS reads enforce Googlevideo redirects,
  exact 206/Content-Range, at most 1 MiB responses and a 15-second timeout.
  Revocation discards reads and cancels pending extraction. A blocking read may
  remain until its timeout. No downloaded video or converted disk cache exists.
- Shared controls retain pair/time/offset/audio across fallback, hold both
  linked players during buffering and fence late play/seek messages. Native
  Play through buffering preserves intent. Iframe messages validate origin and
  source; commands wait for navigation and actual API readiness. Synchronization
  is approximate. Live anonymous probes required sign-in. TASK165 lets operators
  approve a remembered browser profile; no browser auto-discovery or silent
  personal-cookie access occurs. Native session disable/change revokes pending
  and active streams. Third-party database copies use private temporary work
  cleaned after child termination; abrupt-exit recovery remains a follow-up.
  Live private extraction/browser decryption and installed playback are untested.

- TASK147 supports up to 64 active/paused configured roots on the device
  with independent visibility, audience, playlist and cleanup choices. Selection
  changes only the activity view. All enabled roots share one scan coordinator
  and the existing bounded upload/hash/probe engine. Only direct-child supported
  files are eligible, including existing files after two unchanged scans.
- Native monitor IDs scope observations, uploads, audits, scan/retry/reconciliation
  commands and cleanup progress. Mutations recheck the connected channel; omitted
  IDs are accepted only for an unambiguous single-folder compatibility call.
  Removed configurations retain queue/history and originals. Physical root aliases
  are checked during approval/scanning, while schema 7 migration stays database-only.
  Rollback requires a pre-migration database backup; no personal profile is migrated
  by the controlled test fixtures. Playlist choices are verified against native
  read-only provider ownership/title metadata before recurring approval commits.
- Re-adding the same channel/root revives its retained monitor ID and observations
  without replacing original source locators or the choices on queued items.
  Exact archive-key lookup precedes bounded physical-alias checks; a truncated
  unmatched alias search rejects approval rather than silently reintaking files.
  Legacy path keys can be repaired only after same-root runtime proof. Worker
  shutdown rechecks concurrent enablement, and pause/remove commands wait for
  scan serialization outside the GUI thread.

- TASK146 extends manual-upload defaults with Private/Unlisted/Public visibility
  and a default playlist. Audience/visibility apply across channels on a device;
  playlist choices follow the connected channel and are never exposed/applied to
  another channel. Native partial patches use owned provider metadata and one
  transaction; missing fields preserve values, empty playlist clears that channel.
- Review waits for defaults/current playlists, preselects only an available owned
  default, preserves explicit per-batch choices when reads finish/refresh, and
  explains missing defaults. Preferences never modify existing queue entries or
  Folder Monitor configuration. The expanded panel has lazy CSS ownership and
  its command helpers load with manual-upload surfaces to retain startup budgets.
- Schema 6 preserves existing Made for Kids values, OAuth bundle pointers, quota
  pauses and source/checkpoint evidence. As with schema 5, application rollback
  requires a pre-migration database backup; no personal profile was migrated
  during fixture verification.

- TASK145's account action uses Choose JSON → Review → Reconnect. Native staging
  pauses account operations until review is completed/cancelled/expired, rejects
  active uploads/conflicting consent, and only promotes after same-channel proof.
  Tokens/secrets/JSON contents stay native and OS-protected. Queue records,
  original sources and resumable checkpoints stay intact; management consent is
  reset after successful replacement. Google consent still uses its approved
  browser flow rather than collecting login credentials in the application.
- Exact `error.errors[].reason` values distinguish `quotaExceeded` and
  `dailyLimitExceeded` (Cloud API quota) from `uploadLimitExceeded` (channel cap).
  Documented HTTP 400 channel limits are recognized alongside 403/429. Cloud
  takes precedence and mixed results retain both pauses. New Cloud pauses use
  project metadata, with a client fallback when unavailable; a same/unknown
  project change carries the existing pause rather than assuming fresh quota.
  Pause events invalidate the account view without settled-idle polling.
- Schema 5 retains old credential compatibility through a nullable protected
  bundle pointer. Migration is transactional; no personal credentials or live
  profile were changed in local fixtures. Older apps reject a migrated schema;
  reverting a deployed application requires its pre-migration database backup.

- Prefer stepped dialogs for multi-stage in-app flows, per the user's
  2026-10-13 instruction. Playlist creation uses Name, Visibility and final
  Review with Back navigation; provider writes never occur during Next steps.
- Playlist creation/addition use the separately protected management credential
  and 15-minute mode. Ordinary upload/read consent lacks playlist-write scope
  and must stay separate. Review exposes explicit permission actions; native
  code rechecks scope before credentials or writes. Addition receipts/progress
  bind channel, playlist and request; retries reread current membership.

- TASK143's duplicate media modal resolves opaque upload/preflight/candidate IDs
  through Rust. Source paths and credentials remain native-only. The preview
  protocol grants only the main webview, GET/HEAD and single bounded byte ranges;
  grants expire after 30 minutes and are revoked on close/stale acquisition.
  Unsupported source URIs/codecs or unavailable originals are disclosed.
  YouTube embeds retain native controls, have at least a 200px viewport height,
  send the normal referrer policy, and perform no automatic playback. The modal
  and its styles load only after the operator chooses Compare videos.

- Do not add an application backend, cloud queue, database, object store, analytics service, or automatic media upload outside Google/YouTube.
- Keep Google/YouTube refresh and access tokens out of source control, webview bundles, logs, and memory-bank files. Store them through platform secure storage accessed from Rust. An installed-app OAuth client ID is a public identifier, not a client secret.
- Use PKCE for installed-app OAuth. Desktop may use loopback redirects; Android and iOS must use an approved platform-specific flow because mobile loopback redirects are deprecated.
- Require least-privilege OAuth scopes. Document every requested scope and its product need before implementation.
- Treat upload retries as an idempotency problem. Persist an item identity and provider response before retrying; surface ambiguous outcomes for operator review rather than silently starting a new upload.
- Validate source type, size, and required metadata before queuing. Enforce provider quota and backoff behavior in the execution layer.
- Redact request headers, tokens, local file paths, and sensitive provider payloads from logs and errors shown to operators.
- Manual and watched-folder intake reference the original source without creating app-owned `.media`/`.partial` files, links, reflinks, or clones. Validate through a no-follow handle and persist the locator, stable signature/digest, queue ID, immutable channel, and protected session evidence. Originals must remain available and unchanged until upload/recovery completes; missing or changed bytes fail safely rather than being substituted. TASK142 restores this explicit user storage preference and amends AGENTS.md; TASK141's copied-media boundary is historical.
- Persist the encrypted resumable session URI, total length, metadata fingerprint, and provider-confirmed byte range transactionally after every acknowledgment. On launch, query the session using an empty range request; use the returned `308` range or completed response as the sole resume authority. A `404` session expiry or an ambiguous result enters reconciliation and requires a verified match or operator-approved retry; it must not blindly create a potential duplicate.
- Native setup remains database-only: it classifies interrupted work without media reads, probes, networking, or secure-store lookup. After the safe shell paints, one bounded coordinator retains interrupted legacy intake for review, resolves protected resumable sessions, and recovers local jobs without making new video copies. Dispatch verifies original-source integrity before provider use. Cleanup removes only owned UUID media without an upload-record reference; referenced legacy media and checkpoints are retained. Queue dispatch remains fenced until recovery and immutable channel checks pass.
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
  uses an 8 MiB sequential-read buffer; the historical copy benchmark uses a
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
  scan or write. Watched-folder intake references the original without writing
  another video file. The original must remain unchanged and available until
  upload/recovery completes; source cleanup still waits for verified provider processing.
- After the watched-folder light title gate clears, native intake references the stable source and runs deep BLAKE3 verification in the background. A matching completed-upload digest in the same immutable channel withholds automatic dispatch; a source that changes is stopped safely. No duplicate decision deletes a remote video automatically.
- Desktop watched-folder duration validation prefers the fast ISO-BMFF header path and uses bundled FFprobe only as a 15-second bounded fallback. A stalled or unsupported probe cannot hold the persistent observation in `processing`; it is terminated and the normal light-gated queue handoff continues.
- Native media work now uses one shared device-local scheduler: at most two FFprobe processes run at once, and at most one copy/hash reader runs per detected source volume. Active uploads register their source volume; new probes wait behind that upload and foreground/background readers yield in short bounded intervals so media enrichment cannot monopolize the upload disk. Probe and read waits observe cancellation, FFprobe is killed on cancellation or timeout, and watched-hash cancellation reuses one SQLite connection with checks throttled to 250 ms rather than opening a database per chunk.
- Upload-limit validation uses a duration-only FFprobe request capped at 64 KiB. Rich pre-ingest metadata is capped at 2 MiB and is reused for duration only when the canonical path, byte length, and nanosecond modification key still match. The bounded 256-entry in-memory cache coalesces matching in-flight probes and does not permanently cache spawn, timeout, or cancellation failures. ISO-BMFF remains the zero-process fast path, and startup classification/recovery never enters the probe path.
- New records reference original files without managed media writes. Existing
  owned legacy media remains retained while an upload record references it. An
  explicit future storage-reclamation workflow is separate from queue clearing.
- Release-only copy phase diagnostics keep their fields/functions under
  `cfg(test)`. On the near-full C: volume, user-space stream/write/hash was p50
  74.891 ms while the required `sync_all` was p50 934.431 ms. The durable flush
  remains mandatory: neither omitting it nor pre-extending a partial file is
  compatible with trusting byte length and digest reconstruction after a crash.
  Healthy-headroom paired evidence later rejected 8 MiB/sequential copy at
  +3.70%/+18.61% p50/p95 and 1 MiB/sequential copy at +3.79%/+10.35% versus
  their paired 1 MiB standard-open references. This is historical copy-path
  evidence; the retained test/benchmark primitive uses 1 MiB standard opens,
  `sync_all`, resume reconstruction, cancellation, and scheduler admission.
  Current production intake does not call that copy primitive.
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
- Native intake rejects a file before queue creation when it exceeds YouTube's published 256 GB maximum or its detected duration exceeds 12 hours. Desktop derives duration through the bundled hidden FFprobe sidecar with ISO-BMFF fallback; Android and iOS use the ISO-BMFF path available without a sidecar. Manual partial-import notices retain the actionable limit reason, while watched-folder observations are marked rejected rather than retried indefinitely.
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
- Completed manual intake batches queue and begin their saved native upload work immediately when the selected YouTube channel is connected; without a connection their original-source references remain persisted for later dispatch. Originals must remain available and unchanged. Watched-folder files follow the same automatic dispatch after their two-scan stability gate and native source validation, without copying video bytes.
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
  when the original source reference verifies and the protected YouTube
  resumable-session checkpoint survives, then start at the provider-confirmed
  byte range. An unavailable/changed source or missing safe checkpoint remains
  in explicit reconciliation. Startup cleans legacy `.media`/`.partial` files
  only after the database classification proves that no upload record references
  them. Watched-folder observations, pre-ingest jobs,
  inventory staging, and import records are persisted before their work runs.
  Remote deletion writes an `executing` checkpoint before the DELETE request;
  a restart changes it to `needs_reconciliation`, requiring a new typed-ID
  confirmation rather than assuming completion or blindly retrying. Portable
  archive import is transactional and export is written/synced to a temporary
  file before publication, never overwriting an existing archive.
- Queue clearing is cancellation, not destructive cleanup: upload items become
  `cancelled` while their original references and resumable evidence remain local;
  pre-ingest jobs become `cancelled` and workers stop before subsequent files
  or inventory work; pending/recoverable deletion requests become `cancelled`
  without contacting YouTube. Each action has a device-local audit receipt.
- The webview process is presentation-only. Disk streaming, hashing, reference
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

## 2026-10-13 implementation and validation boundaries

- TASK149–161 record locally completed workflow contracts and screenshots. Final
  checks pass 417 native cases (six ignored), 480 frontend cases, strict
  TypeScript, production native check/fmt and unchanged bundle budgets:
  223.20 KiB raw JavaScript, 69.97 KiB gzip JavaScript, 39.40 KiB raw CSS. The
  separately invoked real decoder case passes in 15.57 seconds. Forty operation/
  packaging fixtures pass, with one Unix check skipped on Windows; 189 merged
  browser assertions pass without console errors/warnings. Direct default lazy
  entry points preserve named component exports and reduce startup overhead.
- `persistence::ensure_runtime_indexes` installs ordinary secondary indexes
  atomically during deferred startup before recovery-plan consumption. Cold
  tables/triggers and watched-root uniqueness remain pre-shell. Idempotence,
  DDL rollback/retry, unchanged queue/checkpoints and populated hot query plans
  are covered; the 110-statement cold budget remains unchanged.
- Tauri tray support and desktop notification 2.6/autostart 2.7 are initialized
  natively. Notification compatibility requires Tauri 2.12; Cargo.lock and
  generated native ACL/schema updates reflect that dependency resolution.
  No webview token grant or new JavaScript plugin dependency is introduced.
- Publication captures UTC plus exact selected timezone and native pinned
  `chrono-tz =0.10.4` TZDB 2025b. Private upload setup omits `publishAt`; a
  separate owner-verified durable job applies the reviewed time after processing.
- FFmpeg preparation uses exact per-target length/SHA256 and upstream notices;
  actual binary versions differ by platform despite the upstream release tag.
  Existing fully prepared redirected output is accepted read-only, after all
  bytes/provenance are rechecked; incomplete or changed redirects fail without
  repair. Normal fresh CI directories remain the packaging destination.
- This checkout's existing `src-tauri/binaries` junction points to an unavailable
  F: drive and was preserved. Native check/tests use an explicit local harness
  resource override; that is not normal installer/sidecar packaging evidence.
  The Windows portable workflow validates all six ZIP members, both decoder
  hashes/PE architecture and nonempty license/source notices. Its fixtures do
  not establish a GitHub publication or installed app.
- Retry bulk review accepts at most 200 exact matching saved items and expires
  after 15 minutes. Oversized groups reject without queueing or truncation.
  Structured setup status/reasons are sanitized; secure-store failures are
  distinct from a missing checkpoint. Unknown transfer/provider outcomes retain
  reconciliation gates. A single global issue selector/button sits below queue
  filters; individual rows contain only their own retry.
- Real YouTube operations, personal account changes, OS toast/tray/login startup,
  packaged playback and other-platform runtime behavior remain unverified.
  Perceptual review needs representative camera/media accuracy/performance and
  exact shipped-decoder distribution evidence before release.

## Conventions to establish with implementation

- Add Tauri 2, React + Vite, Rust, SQLite, OS secure-storage integration, formatter, linter, type checking, tests, and CI.
- Record package manager, runtime version, platform build prerequisites, validation commands, signing, and release workflow here when scaffolded.
