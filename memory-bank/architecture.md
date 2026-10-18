# Architecture

## Current state

The implemented application is a Tauri 2 runtime using React + Vite for the
presentation layer and Rust for privileged device-local operations. It retains
one cross-platform codebase for Windows, macOS, Linux, Android, and iOS, while
packaged-runtime evidence is recorded separately for each platform actually
available. All application-controlled state stays on the device; Google/
YouTube is the only required external service.

The product is named **YouTube Upload Manager**. It is an independent project,
not an official Google or YouTube product. Google and YouTube are trademarks of
Google LLC; other marks belong to their respective owners.

## Intended ownership boundaries

TASK148 adds device-local native modules for verified source relocation,
completion and targeted repair, immutable batch plans, metadata presets,
thumbnail/caption jobs, publication schedules, aggregate transfer controls,
quota observations, folder rules, desktop presence, protected profiles and
experimental local perceptual candidates. Each has a lazy stepped webview
dialog; `lib.rs` composes account fencing, intake transactions, workers and
commands. TASK161 adds scoped single/matching-issue retries to those retained
upload identities. A native failure snapshot and original-byte verification
authorize retry; a webview error label alone cannot authorize uncertain work.

Schema 8 retains original locators, signatures/digests, immutable channels,
protected checkpoint references and existing approvals. New intake captures
metadata and batch/plan identity. Plan execution is claimed durably once;
repeated completion returns its receipt and interrupted claims cannot create
another identity. Follow-up attachment/publication jobs belong to the existing
video receipt and reconcile independently of uploading the video.

The cold empty-database path creates canonical tables, workflow schemas,
revision triggers and the live watched-root uniqueness invariant. Ordinary
secondary runtime indexes are installed in one atomic deferred transaction
after the safe shell and before recovery, discovery or dispatch. A final index
sentinel is valid only because the full catalog commits atomically. Failed DDL
retains the recovery plan and admission gate for a later retry; it never changes
queue/checkpoint evidence. This preserves the existing 110-statement pre-shell
budget instead of raising it. Migrated populated databases retain their indexes.

Desktop notification/tray/login choices are independent, opt-in and explicitly
approved. Only state-change inserts wake the bounded global activity worker;
notice/cursor receipts remain quiet. Opaque routes retain channel scope and do
not automatically switch accounts. Profile credentials stay in independent OS
secure-store bundles; selected-profile mutation and provider read chains must
serialize so an old token cannot be attributed to a new account.

Perceptual matching is experimental local review, using pinned native decoder
bytes and bounded aligned grayscale frames. It cannot establish a remote
YouTube content hash and never automatically suppresses uploads or deletes
media. Generated fixture evidence and the known hue-shift false positive do not
meet the representative-media accuracy/performance release gate.

| Area | Owns | Must not own |
| --- | --- | --- |
| React webview UI | File selection, metadata entry, review, progress display | OAuth tokens, unrestricted filesystem access, direct privileged API calls |
| Rust local command layer | OAuth exchange, validation, scoped filesystem access, batch orchestration, audit events | Rendering untrusted UI or exposing token material |
| Local job engine | Crash-safe queue, resumable upload execution, provider reconciliation, retries, quota/rate-limit handling | User interface state as its source of truth |
| OS secure store | Refresh tokens, encryption keys, session secrets | Webview-readable or plaintext secret storage |
| Local SQLite database | Canonical batches, items, source locators/signatures, idempotency keys, lifecycle, cache, and audit records | Raw OAuth tokens or video bytes |
| Source-media boundary | Original source locator/signature, BLAKE3 digest, and persisted queue/recovery identity | New app-owned video copies, links/clones, automatic cloud copying, or unverified replacement bytes |
| YouTube APIs | Account authorization and video upload/publish outcomes | Local operational state |

The native runtime now uses incremental, ownership-based modules rather than a
wholesale command-layer rewrite. `persistence` owns SQLite lifecycle and query
primitives; `state_events` owns revision delivery; `provider_transport` and
`upload_scheduler` own bounded provider transport and upload admission; and
`media_runtime` owns device-volume read/probe admission, active-upload priority,
copy/hash primitives, FFprobe lifecycle, ISO-BMFF duration parsing, and the
stable-signature probe cache. `lib.rs` remains the composition boundary for
Tauri commands, product schemas, channel/account checks, audit writes, startup
recovery, and database cancellation adapters. In particular, the media module
cannot open SQLite or translate provider/product state, while the command layer
cannot bypass its bounded read/process guards.

`oauth_replacement` owns reviewed same-channel credential replacement and
project-scoped API pauses. A candidate JSON is parsed/staged only in native code;
PKCE consent verifies the original immutable channel before a SQLite transaction
promotes an opaque OS-protected credential-bundle pointer. Cancellation, failure
and stale callbacks keep the original connection. Dispatch/management/credential
guards serialize admission and promotion; queued source references and protected
resumable checkpoints are retained. Replacement invalidates token caches and
separate management permission so the latter must be granted again.

`youtube_limits` reads exact structured provider reasons. Cloud API quota takes
precedence, while mixed errors persist both API and channel pauses atomically.
Schema 5 adds credential/project metadata and API quota records without copying
credentials or rewriting source/checkpoint records. Channel pauses survive every
replacement. Same/unknown project identity retains the API pause; an established
different project uses its own quota scope and retains the old project's record.
Provider acceptance remains authoritative; no automatic credential cycling or
quota-reset promise is implemented.

`upload_defaults` owns device-local manual-review preferences. Audience and
visibility are global on the device; playlist defaults are keyed by immutable
channel ID. A playlist patch resolves the owner and current title through native
read-only YouTube calls, then rechecks the connection and commits all provided
fields atomically. Omitted fields preserve preferences, and an empty playlist
clears only the connected channel. Schema 6 adds global visibility and scoped
playlist preferences while retaining audience, credentials, queue and checkpoints.
The UI loads these as visible review prefills, retains explicit batch overrides,
and checks the playlist against the current channel list before selecting it.
Existing Folder Monitor settings and queued uploads are independent.

`folder_monitors` owns stable watched-root authorizations. Each root keeps its
immutable channel binding, upload choices, status and observations; one shared
worker checks every enabled folder belonging to the connected channel, regardless
of the folder selected in React. Discovery failures remain local to a folder.
Existing upload and media admission limits are shared across all roots.
Schema 7 associates historical observations, queue items and audit receipts with
monitor IDs before rebuilding observation keys. Migration performs database work
only; runtime approval/scanning checks canonical roots and rejects duplicates.
Pause and removal stop new discovery while retaining originals, existing queue
items, checkpoints and history. Re-adding the same channel/root restores its
retained monitor and observations instead of creating a second upload identity.
Recovery, reconciliation and cleanup carry both
monitor and channel scope. Adding/editing uses Folder → Upload choices → Review
and requires explicit recurring approval.

## Canonical entities

- **Account connection:** the authorized YouTube identity and its credential reference; scoped to an operator or tenant when multi-user support exists.
- **Upload batch:** operator-owned group of proposed uploads with a review and execution state.
- **Upload item:** one source asset plus its requested metadata, idempotency identity, lifecycle state, and provider video reference once known.
- **Local asset:** an operator-selected original file with its signature, locator, BLAKE3 digest, and persisted intake/queue state. New intake does not copy or clone video bytes. The original must remain available and unchanged through upload and recovery. Existing referenced legacy media remains protected when needed for recovery.
- **Execution attempt:** an append-only record of a specific provider interaction, retry decision, response reference, redacted error details, and durable resume checkpoint.
- **Resume checkpoint:** encrypted resumable session URI, total byte length, provider-confirmed byte offset, request metadata fingerprint, and checkpoint time. It is updated transactionally after each acknowledged range.
- **Inventory video:** an account-scoped projection of an operator-owned YouTube video and its last synchronization state.
- **Duplicate candidate:** an explainable comparison between records with an evidence tier and an operator decision; never an automatic deletion instruction.
- **Deletion request:** immutable selected video IDs, confirmation context, execution results, and audit receipt.
- **Watched-folder authorization:** a stable device-local monitor ID, immutable channel, selected directory, independent upload choices, enabled/removed state and scan receipts. Every enabled root remains monitored; existing and newly added supported direct-child videos enter intake after two unchanged scans.
- **Folder observation:** monitor- and channel-scoped path, size, modification key, processing state, optional BLAKE3 digest, and canonical upload-item link used to make polling idempotent across restarts.

## Lifecycle baseline

`draft → reviewed → queued → uploading → uploaded | failed | cancelled`

On launch, any `uploading` item is reconciled before it can send another byte: verify the original source reference, refresh authorization if necessary, query the protected provider session, then continue only from the provider-confirmed range. Expired sessions or ambiguous completion enter **needs reconciliation**, never silently starting a second upload. Missing or changed originals fail safely; the app does not copy substitute bytes or ask for another file drop. Referenced legacy managed files remain usable without creating new copies.

Queue recovery has an explicit two-phase native boundary. Before the webview
can dispatch work, one database-only transaction returns pre-network claims to
`queued`, classifies interrupted uploads/deletions fail-closed, and resets
local worker markers. That transaction reads no media or protected session,
runs no FFprobe process, and makes no provider request. React first renders a
safe holding shell; only then can one bounded recovery coordinator resume
source-reference validation, protected resumable-session lookup, watched hashes,
and preflight work. Queue actions remain disabled until classification, safe-shell
rendering, deferred recovery, and immutable active-channel binding all pass.
After classification, deferred recovery retains interrupted legacy intake for
review and verifies saved original references before dispatch without writing
video copies. Cleanup may remove only
owned UUID media with no upload-record reference; referenced legacy files and
protected checkpoints remain intact, including failed/cancelled work.
The dashboard therefore exposes no manual recovery control.

SQLite also owns the durable webview synchronization cursor. Every relevant
channel-scoped mutation appends a monotonic `state_changes` revision inside the
same transaction. One commit-hook-driven native dispatcher blocks while idle,
coalesces progress for a fixed 100 ms window, and publishes safe deltas or
projection invalidations. React attaches one shared listener before requesting
catch-up, rejects data from another immutable channel, and reloads a bounded
snapshot only when retained revisions cannot cover its cursor. Global revisions
may be absent from a channel batch because other-channel rows are filtered and
same-entity rows are coalesced; the envelope cursor, not contiguous entity
revision numbers, defines coverage.

YouTube transport is native-owned and lazy. Native state construction creates
no HTTP client; the first provider operation initializes one pooled control or
upload client with an explicit timeout policy. Access tokens are cached only in
Rust memory with expiry skew and refresh singleflight, while refresh tokens and
resumable-session URIs stay in OS protected storage. A resumable transfer keeps
one pooled client handle and one request-owned bounded chunk buffer per worker,
and persists every provider-confirmed range before sending the next one.

Upload execution uses durable SQLite claims plus a four-permit native scheduler.
Eligible source volumes are selected round-robin subject to their cached local
limits, and each claim runs independently so a slow transfer cannot serialize
the rest of the batch. A successful provider response commits the video ID,
confirmed byte count, immutable channel binding, and audit receipt atomically.
Playlist insertion, protected session removal, and guarded source cleanup run
after that receipt in one channel-scoped lower-priority worker without holding
an upload permit. Remote deletion is excluded from this scheduler and remains
sequential and operator confirmed.

This boundary is certified locally by the final integrated native suite (127
passed, zero failed, five release-only benchmarks ignored) and the integrated
release suite (5/5 passed). The canonical loopback upload result is optimized
p50/p95 204.516/239.985 ms at 312.933 MiB/s versus pooled streaming
417.810/447.743 ms, a 2.0429x ratio with one 8 MiB request buffer and zero extra
full-chunk copies. It is not evidence of live Google/YouTube performance.

Publishing visibility (private, unlisted, public, scheduled) should be explicit metadata, independently validated before execution, and included in the audit trail.

Watched-folder automation is a deliberate recurring approval rather than an
unreviewed publishing path. The operator chooses private or unlisted visibility
when enabling the monitor; public visibility is unavailable. The local worker
scans direct child files only while the app runs, requires an unchanged
signature across consecutive scans, validates and references the original without
creating a copy, withholds local-digest and
last-synchronized-title matches for review, and
dispatches only the persisted private or unlisted resumable uploader. A
connection mismatch pauses before network use. The watched source must remain
available and unchanged until upload/recovery completes. Guarded opt-in original cleanup
still checks the original and waits for confirmed YouTube processing success.

Completed manual intake batches are queued and dispatch automatically whenever
their approved channel is connected. If YouTube reports its daily upload limit,
the local job engine records a conservative device-local 24-hour dispatch pause,
keeps affected work queued, and resumes it from a conditional worker at the
saved deadline or on the next launch. The worker exits when no pause remains;
the per-channel pause stores no provider response body or credential.

Duplicate detection remains an explicit operator action. **Run dedupe** first
synchronizes the active channel's owner-authorized upload inventory, then
rebuilds local BLAKE3 and uploaded-title candidates for review. It never
creates or executes a deletion request.

Duplicate review now offers a lazy **Compare videos** modal from upload title
holds, pre-ingest results, and saved candidates. Rust resolves the selected
record against the current channel and returns matched YouTube IDs plus opaque,
revocable read-only local grants. The custom `media-preview` protocol serves
original file ranges of at most 1 MiB, checks channel, source signature/handle
identity, expiry and revocation on every request, and follows the existing media
reader admission rules. Preparation never copies or rehashes whole videos.
Closing the native dialog unloads both players and revokes local grants.
Missing originals remain visible beside any usable matched video; local codec
and private/disabled YouTube embedding support are platform/provider boundaries.

TASK163 adds a compatibility path when the local player has metadata but no
decoded picture. The same opaque grant authorizes short memory-only H264/AAC
segments from the original through the verified bundled decoder. The webview
uses MediaSource with a bounded playback window, full source duration and
seeking; no converted video is written to disk. Grant validation, cancellation,
reader admission and the shared two-process inspection cap still apply. Manual
preview can run during uploads with foreground reader yielding; background
analysis admission is unchanged. Only media Blob URLs are added to CSP.

Folder-monitor background invalidations are serialized and coalesced per
channel/selection. They update mounted details without entering initial loading
or disabling navigation. Explicit channel/folder changes retire the old reader
and its queued updates; dialog, focus, disclosure and scroll state survive
ordinary status refreshes.

TASK164 adds optional desktop yt-dlp review at the user's explicit request as a
scoped exception to the official-API-only boundary. Ordinary account, upload,
inventory and deletion operations retain their existing authorization. Selected
yt-dlp and optional Deno/Node resolve only IDs from current native duplicate
evidence. A revocable opaque grant serves at most 1 MiB per validated HTTPS
Googlevideo range through the existing media-preview protocol. Signed URLs,
headers and video bytes stay in native memory. No disk media cache, cookie
import, OAuth handoff or automatic executable download is introduced.

A shared controller links originals, compatibility playback and iframe players
using a left-master clock, offset-adjusted overlap, bounded correction,
buffering holds and one audible side. Fallback/retry preserve selection/time
without resuming automatically. Iframe commands require committed navigation
and actual API readiness, with origin/frame validation. Sync is approximate.
Mobile and unavailable extraction use the embed. Live extraction and installed
WebView playback remain certification gates; a public probe required sign-in.

Playlist selection is bounded to 200 videos per addition and retained across
search, visibility filters and pages. Page/matching shortcuts and Shift ranges
share one selection model. The native playlist bulk adapter preserves reviewed
order, validates channel and playlist ownership, reads current membership and
returns confirmed added/skipped IDs plus unresolved IDs. Progress is bound to
channel, destination and request ID; retries re-read membership before writes.
Creation is a lazy Name → Visibility → Review dialog shared by Playlists and
upload intake. Every playlist write uses the protected management credential
and temporary mode; ordinary upload/read scopes remain unchanged. Unknown
creation outcomes require checking the playlist list before repeating a write.

Inventory and title-dedupe work is generation based. Each staged remote page
persists the versioned normalized, canonical-copy, and numeric-sequence keys
used only to narrow candidates. The final page atomically promotes the complete
generation for one immutable channel; a partial generation never replaces the
last complete view. Local-upload and inventory mutations advance separate
channel generations. Their pair identifies the materialized duplicate
projection, so unchanged dashboard reads are bounded projection reads while a
changed channel rebuilds only its own explainable candidates. Every candidate
is confirmed by the exact evidence function before it becomes reviewable.

Large preflight jobs separate progress from evidence transport. Workers persist
compact counters during execution and materialize full per-file match evidence
once at completion. The webview reads status without file rows, requests
independently bounded file and activity pages, and requests one rich metadata
record only when the operator expands it. The native layer never returns source
locators in list or batch receipt payloads.

Manual multi-file intake is one native batch boundary rather than one bridge
round trip per file. Native code independently validates and references each
original source without copying its bytes, performs one
channel inventory/title preparation, commits accepted
queue rows together, returns redacted per-item receipts, and wakes dispatch
once. A failed item does not conceal or roll back the receipts of independent
items, and every import, receipt, lookup, and queue row remains bound to the
reviewed immutable channel. Both intake paths require the original to remain
available and unchanged through upload and recovery. No copy, hard-link, reflink,
or clone fallback is permitted. TASK142 records the explicit storage preference.

The product display name is **YouTube Upload Manager**. Its Tauri identifier
and OS credential-store service name use the `com.sekailens` namespace:
`com.sekailens.youtube-upload-manager`.

## Performance and certification boundary

The safe shell and full workspace are separate milestones. React keeps the
holding/recovery shell mounted until the native runtime acknowledges its paint;
only then may the bounded deferred-recovery command complete and the Batch
workspace become actionable. The unsigned performance package records native
initialization, safe-shell paint, native readiness, first Batch paint, and a
bounded settled-idle snapshot. A missing or rejected milestone is a failed
receipt rather than a silently ignored performance sample.

Large browser-rendered lists mount at most 32 records per page. Inactive
workspaces remain unmounted and lazy-loaded. State convergence uses the durable
SQLite cursor and compact channel-scoped events rather than periodic full
snapshots. Provider clients remain lazy and pooled; upload work remains bounded
to four global permits plus per-source-volume admission; media probing remains
bounded to two FFprobe processes and one copy/hash reader per source volume.

Evidence levels are never interchangeable. Local TypeScript/Rust tests,
browser interaction fixtures, local SQLite and loopback-provider benchmarks,
the unsigned instrumented Windows package, a standard unsigned package, a
signed production package, and live Google/YouTube operations are reported as
separate levels. The final healthy-headroom packaged timings were captured on a
SATA HDD; the historical system-NVMe baseline used non-equivalent milestone,
WebView-profile, and headroom conditions, so no 50% comparison is inferred.

The available Windows boundary is complete: both empty and synthetic
interrupted-256 GB instrumented matrices passed their integrity, safe-shell,
materiality, and zero-idle-work gates, while the ordinary unsigned x64 package
passed executable/installer/portable hash checks, exact ZIP sidecar/license
readback, harness isolation, and exact-path desktop rendering of a real
81-item Batch profile with a 32-row page. This does not imply signing,
non-Windows packaging, live OAuth/provider behavior, or equivalent-NVMe speed.

## Decisions pending

- Single-operator versus tenant-aware data model.
- Local database encryption-at-rest approach and backup/export semantics.
- Platform-specific OAuth client registrations and redirect handlers.
- Code signing, mobile-store distribution, desktop update, and release pipeline.
