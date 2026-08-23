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

| Area | Owns | Must not own |
| --- | --- | --- |
| React webview UI | File selection, metadata entry, review, progress display | OAuth tokens, unrestricted filesystem access, direct privileged API calls |
| Rust local command layer | OAuth exchange, validation, scoped filesystem access, batch orchestration, audit events | Rendering untrusted UI or exposing token material |
| Local job engine | Crash-safe queue, resumable upload execution, provider reconciliation, retries, quota/rate-limit handling | User interface state as its source of truth |
| OS secure store | Refresh tokens, encryption keys, session secrets | Webview-readable or plaintext secret storage |
| Local SQLite database | Canonical batches, items, source locators/signatures, idempotency keys, lifecycle, cache, and audit records | Raw OAuth tokens or source-video copies |
| Source-media boundary | Original operator-selected source path, stable signature/digest, and resume-safe verification | App-managed `.media` or `.partial` copies, automatic cloud copying, or a copy fallback |
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

## Canonical entities

- **Account connection:** the authorized YouTube identity and its credential reference; scoped to an operator or tenant when multi-user support exists.
- **Upload batch:** operator-owned group of proposed uploads with a review and execution state.
- **Upload item:** one source asset plus its requested metadata, idempotency identity, lifecycle state, and provider video reference once known.
- **Local asset:** an operator-selected device-local source reference with byte size, stable filesystem signature, BLAKE3 digest, and import state. It remains available for retry only while the original path remains available and unchanged; the application creates no app-managed media copy.
- **Execution attempt:** an append-only record of a specific provider interaction, retry decision, response reference, redacted error details, and durable resume checkpoint.
- **Resume checkpoint:** encrypted resumable session URI, total byte length, provider-confirmed byte offset, request metadata fingerprint, and checkpoint time. It is updated transactionally after each acknowledged range.
- **Inventory video:** an account-scoped projection of an operator-owned YouTube video and its last synchronization state.
- **Duplicate candidate:** an explainable comparison between records with an evidence tier and an operator decision; never an automatic deletion instruction.
- **Deletion request:** immutable selected video IDs, confirmation context, execution results, and audit receipt.
- **Watched-folder authorization:** one operator-selected device-local directory, the channel scope approved to receive its new files, enabled state, scan receipts, and last result. Existing files form a baseline; only later additions or changed replacements can become candidates.
- **Folder observation:** channel-scoped path, size, modification key, processing state, optional BLAKE3 digest, and canonical upload-item link used to make polling idempotent across restarts.

## Lifecycle baseline

`draft → reviewed → queued → uploading → uploaded | failed | cancelled`

On launch, any `uploading` item is reconciled before it can send another byte: verify the original referenced source has not moved or changed, refresh authorization if necessary, query the persisted provider session, then continue only from the provider-confirmed range. A completed response records its returned video ID; an unavailable or changed source, expired session, or ambiguous completion is marked **needs reconciliation**, never silently retried as a new upload.

Queue recovery has an explicit two-phase native boundary. Before the webview
can dispatch work, one database-only transaction returns pre-network claims to
`queued`, classifies interrupted uploads/deletions fail-closed, and resets
local worker markers. That transaction reads no media or protected session,
runs no FFprobe process, and makes no provider request. React first renders a
safe holding shell; only then can one bounded recovery coordinator resume
source-reference validation, protected resumable-session lookup, watched hashes,
and preflight work. Queue actions remain disabled until classification, safe-shell
rendering, deferred recovery, and immutable active-channel binding all pass.
After classification, startup may remove legacy app-managed `.media` and
`.partial` files only when no live queue job references them; queued, uploading,
and reconciling legacy jobs retain their referenced files.
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
signature across consecutive scans, hashes and references accepted media in
place without creating a managed-media copy, withholds local-digest and
last-synchronized-title matches for review, and
dispatches only the persisted private or unlisted resumable uploader. A
connection mismatch pauses before network use. The watched source must remain
available and unchanged until YouTube confirms the upload; if it moves or
changes, the item fails safely rather than uploading a replacement.

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
original source in place, performs one channel inventory/title preparation, commits accepted
queue rows together, returns redacted per-item receipts, and wakes dispatch
once. A failed item does not conceal or roll back the receipts of independent
items, and every import, receipt, lookup, and queue row remains bound to the
reviewed immutable channel. Manual intake has the same availability and
unchanged-source requirement as watched-folder intake; it has no managed-copy
fallback.

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
