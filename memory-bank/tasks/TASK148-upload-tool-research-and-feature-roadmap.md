# TASK148 — Upload-tool research and feature roadmap

**Status:** completed

**Research completed:** 2026-10-13, Asia/Singapore

**Owner:** Codex for implementation and research

**Dependencies:** TASK108, TASK110, TASK114, TASK142–147; see each candidate below

## Objective and scope

Identify useful additions to YouTube Upload Manager by comparing similar tools
and checking the current source. Separate ordinary feature parity from potential
differentiators that the reviewed tools do not document. This file is a researched
implementation backlog. The user authorized all candidates for implementation
on 2026-10-13, starting with the highest-priority ones. TASK149–160 are the scoped
execution records; the full parent goal remains active until all 12 are verified.

The strongest first additions are verified source relocation, repair of failed
post-upload steps, and a complete batch simulation. These address practical
problems around long uploads and many watched folders without adding a cloud
service or making copies of videos.

Research used four maintainers' repository READMEs/examples and official YouTube
Studio/API documentation. No competing app was installed, tested or audited.
“Not documented” means absent from the reviewed pages, not proven absent from
every version, branch or competing product. Priorities and differentiator claims
below are product inferences. Recheck API capabilities before implementation.

## Comparison evidence

| Project / primary source | Documented useful capabilities | Opportunity in our app |
| --- | --- | --- |
| [tokland/youtube-upload](https://github.com/tokland/youtube-upload) | CLI metadata, language, thumbnail, playlist, publication-time options and separate credential files. | Bring comparable metadata/scheduling into reviewed GUI workflows; its README does not document a source-relocation wizard or consolidated recovery view. |
| [porjo/youtubeuploader](https://github.com/porjo/youtubeuploader) | JSON metadata, captions, thumbnails, several playlist destinations, localization and upload bandwidth controls. | GUI templates, timed network controls and independently recoverable follow-up jobs. Its examples do not document a whole-batch simulation or verified source relinking. |
| [beveradb/youtube-bulk-upload](https://github.com/beveradb/youtube-bulk-upload) | GUI/CLI directory batches, dry run, filename transformations, description templates and per-video thumbnail files. | Expand our existing preflight into a complete reviewable plan; offer templates without requiring scripts. Its documented intake centers on one directory. |
| [GRardT/Youtube-Uploader](https://github.com/GRardT/Youtube-Uploader) | Watched-folder automation, hash dedupe, retry, playlists, tray behavior and desktop notifications. It describes copying/moving sources. | Keep our existing no-copy and multiple-folder advantages; add quiet notifications, richer folder rules and clear completion receipts. Verified relocation and perceptual matching are not documented. |
| [YouTube Studio defaults](https://support.google.com/youtube/answer/2660027?hl=en) and [scheduling](https://support.google.com/youtube/answer/1270709?hl=en) | Broad upload defaults and a timezone-aware publication schedule. Studio upload defaults apply to browser uploads. | Extend our own native review presets; never assume Studio defaults will fill API upload metadata. A local upload window and a server-side publication time are different choices. |

### Existing app baseline — verified from source

Do not reopen these as missing features: original-file/no-copy intake; protected
resumable recovery; bounded concurrent uploads; normalized-title and exact-hash
duplicate review; local/YouTube comparison players; bulk playlist selection and
creation; device upload defaults; separate Cloud/channel limit diagnosis; manual
same-channel OAuth JSON replacement; multiple independent watched folders.
These are source implementations with the verification boundaries in TASK142–147,
not evidence that every new feature has shipped or run against a live account.

Relevant source observations:

- `src/lib/types.ts`: manual metadata currently covers audience, visibility, one
  playlist and cleanup; no publication time, descriptive metadata or asset jobs.
- `establish_upload_session` in `src-tauri/src/lib.rs`: sends a title plus privacy
  and audience. A rich upload-metadata/template flow was not found.
- Post-upload SQL already persists playlist and cleanup outcomes, but `UploadItem`
  does not expose a complete step checklist. The pending-job query excludes
  failed playlist steps; a targeted repair action is still useful.
- `folder_monitors.rs`: stable roots, two unchanged scans and direct-child intake
  exist; no include/exclude rules or producer-ready marker flow was found.
- `upload_scheduler.rs` / `provider_transport.rs`: concurrency, volume fairness
  and acknowledgement tracking exist; user bandwidth caps, upload time windows
  and manual priority controls were not found.
- Source-integrity failures retain evidence, but no reviewed relocation command
  was found. Compact archive export already exists for dedupe metadata; it is
  not a resumable queue/credential backup and must not be presented as one.
- Searches of `src/` and `src-tauri/src/` found no thumbnail/caption endpoint,
  tray/toast integration, perceptual-content matcher or multi-profile account UI.
  Title normalization/rename templates do exist; those are not general upload
  metadata presets.

## Prioritized implementation candidates

Priority indicates recommended order, not a claim that competing apps cannot do
the feature. Effort is relative: S = one narrow surface; M = persisted UI/native
workflow; L = several durable jobs, permissions or platform boundaries.

| ID | Priority | Feature | Type | Effort | Main benefit |
| --- | --- | --- | --- | --- | --- |
| 148-A | P0 | Locate moved originals safely | Potential differentiator | M | Resume retained work after a drive letter or folder changes. |
| 148-B | P0 | Completion checklist and repair failed steps | Potential differentiator | M | Recover a playlist/asset failure without uploading the video again. |
| 148-C | P1 | Whole-batch dry run and exportable plan | Parity + extension | M | See the exact outcome before approving a large batch. |
| 148-D | P1 | Metadata presets and CSV/JSON mapping | Parity | M | Prepare event/session batches without repetitive editing. |
| 148-E | P1 | Thumbnail and caption attachments | Parity | L | Finish video presentation/accessibility inside the app. |
| 148-F | P1 | Publication calendar | Parity | M | Upload privately now and publish at a reviewed time. |
| 148-G | P1 | Bandwidth caps, upload windows and priorities | Parity + extension | L | Avoid disrupting other work and favor urgent batches. |
| 148-H | P1 | Quota-aware operation planner | Potential differentiator | M | Explain likely costs and what a limit actually blocks. |
| 148-I | P1 | Folder filters and producer-ready markers | Potential differentiator | M | Keep temporary or unwanted footage out of automatic intake. |
| 148-J | P2 | Quiet desktop notifications and tray controls | Parity | M | Know when attention is needed while doing other work. |
| 148-K | P2 | Protected channel profiles and explicit switching | Parity + extension | L | Switch channels without repeatedly replacing the account setup. |
| 148-L | P2 | Local perceptual duplicate candidates | Potential differentiator | L | Find possible same-footage matches after re-encoding. |

### 148-A — Locate moved originals safely

**Why:** No-copy queues depend on the original location. A renamed folder or
remounted external drive should not force another drop or a second queue item.
The reviewed uploader pages do not document this workflow; the opportunity is
inferred from our source-reference design.

**Acceptance:**

- A Select location → Verify files → Review modal operates on existing item IDs
  in the selected channel; a folder mapping can repair several selected items.
- Verify length and a trusted full digest using native no-follow handles before
  accepting a replacement locator. A filename or size match is insufficient.
  Without a previously trustworthy digest, block automatic relinking and explain
  that identity cannot be established safely.
- Preserve channel, choices, video ID, confirmed bytes and protected checkpoint;
  reconcile the saved provider session before resuming. Updating a locator is
  atomic and auditable, with no video copies or new upload identity.
- Handle wrong bytes, changed-during-verification files, unavailable drives,
  stale channels, cancellation and a crash during commit without losing evidence.
  Existing source signatures must be refreshed only after verified byte identity.
- A watched-folder root change remains a separately approved monitor action;
  repairing an item must not silently authorize a new watched directory.

**Suggested ownership:** `src/components/QueueTable.tsx`, a lazy relocation dialog,
`src/lib/types.ts`, native source-reference helpers, `persistence.rs` and
`media_runtime.rs`. **Dependencies:** TASK142, TASK147.

### 148-B — Completion checklist and repair failed steps

**Why:** “Video uploaded” and “all requested steps complete” are different states.
Our durable receipt already survives playlist failures; expose that distinction
and a targeted repair workflow. The reviewed READMEs do not document a comparable
consolidated, independently retryable completion checklist.

**Acceptance:**

- Show Upload, YouTube processing, Playlist and Source cleanup outcomes per item;
  later add Thumbnail/Caption steps when 148-E exists. Distinguish succeeded,
  waiting, failed, not requested and unknown; no fabricated completion percentage.
- Retry only selected failed follow-up steps after reviewing their channel/video
  identity. Re-read playlist membership before inserting and use the existing
  required management permission flow; never retry an upload to repair membership.
- Preserve the confirmed video receipt through failure/restart; ambiguous writes
  enter reconciliation. Retained-source reasons remain visible. Processing polling
  is bounded to pending work and stops once a terminal result is known.
- Provide a channel/batch-filtered CSV or JSON completion report, distinct from
  the existing portable dedupe archive, excluding credentials, sessions and source
  locators. Export only on an explicit action.
- Verify processing through owner-authorized API results, not a successful HTTP
  upload alone. [Google's processing workflow](https://developers.google.com/youtube/v3/guides/implementation/videos)
  supports this distinction; it does not promise a full Studio copyright check.

**Suggested ownership:** queue presentation/types, post-upload orchestration in
`lib.rs`, `persistence.rs`, `playlist_bulk.rs`. **Dependencies:** TASK108, TASK144.

### 148-C — Whole-batch dry run and exportable plan

**Why:** [beveradb's dry run](https://github.com/beveradb/youtube-bulk-upload)
demonstrates the value of previewing a batch. Our existing preflight focuses on
duplicates, rather than a complete resolved plan across metadata and automation.

**Acceptance:**

- Select → Resolve choices → Review shows each file's channel, title, audience,
  visibility, playlist, source availability, duplicate disposition and cleanup.
  Include template/schedule/asset results as their candidate features become ready.
- An offline mode uses local/cached evidence and labels its age. An explicit online
  refresh may perform disclosed reads; neither mode uploads, edits, deletes,
  enables a monitor or queues work. Plan persistence must be separate from jobs.
- Export a redacted plan. Revalidate original bytes, channel, playlist ownership
  and relevant provider state at execution; a saved plan is not blanket approval.
- Paginate large plans, preserve Back navigation and reject stale approvals.

**Suggested ownership:** `UploadIntakeReview.tsx`, existing preflight helpers,
types and native read-only planning. **Dependencies:** TASK143, TASK146, TASK147.

### 148-D — Metadata presets and CSV/JSON mapping

**Why:** [porjo's JSON metadata](https://github.com/porjo/youtubeuploader) and
[beveradb's templates](https://github.com/beveradb/youtube-bulk-upload) reduce
repeated editing. Studio defaults do not replace an explicit API payload.

**Acceptance:**

- Create a channel-scoped preset for title/description, tags, category and language;
  optional license/subscriber-notification/altered-content choices require explicit
  declarations. Show resolved values per video before queueing. Preserve deliberate
  item overrides, and never rewrite existing queue metadata when a preset changes.
- Allow bounded local CSV/JSON manifests mapped to selected originals; disclose
  unmatched/duplicate rows, unknown fields and invalid values. File references
  cannot escape an explicitly selected root or refer to arbitrary remote URLs.
- Support simple filename/folder/sequence tokens with a preview, independent of
  the existing post-upload rename tool. Validate native/provider limits and
  Unicode. No arbitrary expression, shell or script execution.
- Validate supplied JSON schemas and any `videos.update` patch with a read/merge
  strategy so unrelated metadata is not silently cleared.

**Suggested ownership:** intake/default panels, lazy preset editor, upload types,
session payload/schema. **Dependencies:** TASK114, TASK146; integrate with 148-C.

### 148-E — Thumbnail and caption attachments

**Why:** [porjo documents both attachments](https://github.com/porjo/youtubeuploader).
They can remove a repeated trip to Studio after every upload.

**Acceptance:**

- Attach a local thumbnail and timed caption file through Choose assets → Match
  to videos → Review. Support optional same-basename matching with ambiguous
  matches held for review; validate current provider formats/limits natively.
- Persist separate, channel/video-scoped asset jobs after a confirmed upload.
  Failed assets retain the video receipt and can be repaired through 148-B.
- Use endpoint-specific permission checks: [thumbnail writes](https://developers.google.com/youtube/v3/docs/thumbnails/set)
  accept upload scope, while [caption insertion](https://developers.google.com/youtube/v3/docs/captions/insert)
  requires a different supported scope. Do not silently broaden ordinary consent.
- Caption language/name/draft choices are explicit. Reconcile existing tracks
  before retry; no duplicate insertion or automatic track replacement/deletion.
  The caption `sync` parameter is deprecated, so do not promise API auto-timing.
- Keep original video bytes untouched; no automatic transcription, thumbnail
  generation or cloud AI service is included in this first scope.

**Suggested ownership:** native asset-job module, schema/types and lazy asset UI.
**Dependencies:** 148-B, 148-D; TASK144 permission boundaries.

### 148-F — Publication calendar

**Why:** [Studio](https://support.google.com/youtube/answer/1270709?hl=en) and
[tokland](https://github.com/tokland/youtube-upload) already document scheduling.
The useful extension is a reviewed batch timeline inside this app.

**Acceptance:**

- Choose dates/timezone → Preview per-video times → Review. Offer a start time
  plus spacing; display local time and the persisted UTC instant. Handle DST
  gaps/ambiguity, edits, restart and a timezone change without shifting approvals.
- Distinguish “upload at” from “publish at”. After YouTube confirms an accepted
  server-side schedule, publication does not depend on the app remaining open;
  a local upload window still does.
- Follow [the `publishAt` contract](https://developers.google.com/youtube/v3/docs/videos#status.publishAt):
  private, never-published videos only. Reject past dates instead of accidentally
  publishing immediately. Recheck permission, processing and accepted metadata.
- Late upload or unknown provider outcome requires attention; never silently make
  a watched-folder public upload or change a previously approved publication time.

**Suggested ownership:** upload types/schema/payload, lazy calendar review.
**Dependencies:** 148-D, 148-B; separate from 148-G's local upload controls.

### 148-G — Bandwidth caps, upload windows and priorities

**Why:** [porjo](https://github.com/porjo/youtubeuploader) documents rate limits and
time-based limiting. Multiple watched folders make network control more useful.

**Acceptance:**

- Set one device-wide upload budget with optional timed limits. Enforce it across
  all upload workers, not once per worker; control reads must remain responsive.
- A local window prevents new transfer work outside the reviewed hours. Explain
  when an in-flight request will pause safely; preserve provider-confirmed ranges
  and reconcile after interruption. No claim that a closed app continues uploading.
- Allow selected batches/folders to be prioritized without starving others or
  exceeding existing global/per-volume admission limits. Do not interrupt an
  already claimed item by silently changing its source, channel or metadata.
- Persist settings; show why work is waiting. Verify aggregate rates with loopback
  transfers, fairness, boundary times, cancellation and restart.

**Suggested ownership:** `upload_scheduler.rs`, `provider_transport.rs`, schema,
queue controls. **Dependencies:** TASK108, TASK147; integrate with 148-C.

### 148-H — Quota-aware operation planner

**Why:** Generic daily-limit advice can be stale or conflate a project's API quota
with a channel's upload cap. This is especially useful after TASK145's diagnosis.

**Research finding:** As accessed on 2026-10-13, Google's [API overview](https://developers.google.com/youtube/v3/getting-started)
and [method reference](https://developers.google.com/youtube/v3/docs/videos/insert)
describe a separate default video-upload bucket of 100 calls/day, one unit/call.
The [quota table's body](https://developers.google.com/youtube/v3/determine_quota_cost)
describes separate upload/search buckets, other-method units and midnight Pacific
resets. Its generated page summary still contains an older upload-cost number;
porjo's README also carries older approximate upload-count advice. Use dated
method/table evidence and actual project configuration, not either summary.

**Acceptance:**

- Estimate bucket-specific operations for a reviewed batch, including pagination,
  assets, membership checks/writes and retries. Store a dated/versioned cost table
  and allow explicit project-limit configuration. Label costs as estimates.
- Local usage counts are “this app/device observed”, not account/project remaining
  quota: other devices and clients are invisible. Unknown allowance stays unknown.
- Display Cloud quota and channel limits separately; show Pacific reset converted
  to the user's timezone where applicable. Do not turn an estimated reset into a
  promise of acceptance or clear the independent conservative channel pause.
- Provide an official Console/extension-request link rather than credential
  rotation or quota bypass. Preserve reviewed queues and all existing pause records.

**Suggested ownership:** `youtube_limits.rs`, `oauth_replacement.rs`, account/plan
presentation; no new Cloud administration scopes in the first version.
**Dependencies:** TASK145, 148-C.

### 148-I — Folder filters and producer-ready markers

**Why:** A capture/export folder may contain test clips or files waiting for an
editor's final export. Two unchanged scans alone do not establish editorial intent.
These additional gates are not documented in the reviewed watched-folder README.

**Acceptance:**

- Folder → Rules → Review adds include/exclude patterns and an optional explicit
  producer-ready marker. Show examples of included/excluded files before enabling.
- A ready marker is an additional gate, never a replacement for current stability,
  source-integrity, duplicate and channel checks. Malformed markers cannot choose
  another source path, execute code or override approved upload choices.
- Explain “excluded”, “waiting for ready marker” and “changing” separately. Changes
  affect future discovery, not retained queue items or their captured choices.
- Preserve history through pause/remove/re-add, and define whether a rule change
  may admit previously excluded files before final approval. Keep initial scope
  direct-child only; recursive routing needs separate overlap/symlink design.

**Suggested ownership:** `folder_monitors.rs`, monitor schema and setup/activity
UI. **Dependencies:** TASK147; use 148-C for a no-write preview.

### 148-J — Quiet desktop notifications and tray controls

**Why:** [GRardT's tray and notifications](https://github.com/GRardT/Youtube-Uploader)
are useful parity when the upload window is not foreground.

**Acceptance:**

- Opt-in batch-complete, needs-attention and quota-pause notifications; group
  events and suppress repetitive per-scan/per-chunk messages. Clicking returns
  to the correct local channel/item without exposing sensitive paths or secrets.
- Desktop tray offers Show app and Pause new uploads/monitoring with clear scope;
  Exit remains explicit. Clarify close versus keep-running behavior before enabling.
- Optional run-on-login is separately approved, reversible and platform-specific;
  it creates no application cloud service or hidden always-on privileged service.
- Denied notification permission remains usable. Verify platform support rather
  than assuming desktop tray behavior or unlimited background work on mobile.

**Suggested ownership:** native desktop integration, local preferences and
notification/queue routing. **Dependencies:** TASK147 and platform verification.

### 148-K — Protected channel profiles and explicit switching

**Why:** [tokland's separate credential-file option](https://github.com/tokland/youtube-upload)
shows demand for account reuse. Native profiles can make switching less technical.

**Acceptance:**

- Add account → Verify channel → Review creates an independently OS-protected
  profile. Switch explicitly; never reinterpret old queue bindings as the new
  channel. Every playlist, inventory, audit and credential lookup uses immutable ID.
- Only the selected channel dispatches in the initial scope. Switching fences
  stale UI requests and new claims; active sessions retain their original proof.
  Concurrent uploading across accounts is a separate future decision.
- Consent/management permissions and source references remain profile-scoped;
  removing a profile does not erase queue/recovery evidence without a separate
  reviewed action. Tokens are never exported with presets or portable archives.
- Project quota aggregates across profiles using that project; channel limits
  remain channel-scoped. Profiles are not a mechanism for bypassing either limit.

**Suggested ownership:** connection UI, protected credential references, account
schema and scheduler fencing. **Dependencies:** TASK145, TASK147; high migration risk.

### 148-L — Local perceptual duplicate candidates

**Why:** Exact byte hashes cannot identify the same footage after re-encoding.
GRardT documents exact hashing; the reviewed pages do not document a conservative
perceptual review workflow. This is an inferred opportunity, not a superiority claim.

**Acceptance:**

- Opt-in local analysis samples aligned frames and/or audio with bounded native
  resources, records versioned fingerprints and creates explainable candidates.
  Investigate decoder dependencies/licensing before selecting an implementation.
- Show duration/alignment/sample evidence in the existing comparison modal;
  distinguish a possible content match from a cryptographic byte match. Similar
  intros, still scenes, short clips and edited versions must not become automatic
  deletion or upload suppression rules.
- Analyze selected local originals or retained local fingerprints only. Do not
  download YouTube videos or pretend title/thumbnail/embedded playback establishes
  a remote content hash. Missing remote evidence stays missing.
- No video copies/transcodes; do not preclude upload/recovery priority. Validate
  known re-encodes and deliberately different near-matches, report false positives,
  and require human approval for any consequential action.

**Suggested ownership:** optional native content-analysis module, `media_runtime.rs`,
candidate persistence and `DuplicateMediaPreview.tsx`. **Dependencies:** TASK143,
TASK110; release only after representative accuracy/performance evidence.

## Execution and validation requirements

- First slice: 148-A and 148-B as separately scoped tasks, then 148-C. Metadata,
  attachment and scheduling work should reuse those planning/recovery primitives.
  148-H and 148-I are useful next extensions for the current multi-folder workflow.
  148-K/148-L need a design/permission/performance decision before implementation.
- Preserve existing user changes. No backend, telemetry, video clones/copies,
  cookie/password automation, automatic deletion, automatic OAuth cycling or
  unsupported Studio-only capability claims are part of this roadmap.
- Use stepped modals with Back and a final review for consequential changes;
  test stale-channel responses, normal successful flows, cancellation, failure,
  crash/restart and per-item partial outcomes. Keep large lists bounded/lazy.
- Any schema change needs transactional migration fixtures, retained credential/
  source/checkpoint evidence and pre-migration backup rollback instructions.
- For each implemented slice, run its focused frontend/native tests and relevant
  TypeScript/build/native formatting checks. Use existing startup budgets without
  raising them, provide light/dark responsive screenshots for UI changes, and
  separately record live-provider, packaged and other-platform checks.

## Research validation snapshot — before implementation

- Reviewed current project brief, architecture, technical notes, progress, task
  index and Git status. Traced intake metadata, source integrity, playlist follow-up,
  archive export, scheduler and monitor behavior in source.
- Read the primary sources linked next to each comparison/API claim. This is a
  dated documentation comparison; no vendor runtime verification or market-wide
  absence claim is made. API documentation needs rechecking at implementation time.
- Documentation-only task creation; no app behavior, credentials, media, schema,
  build configuration or dependencies changed. No feature tests were required.
- Document checks passed: 12 candidate sections, 12 unchecked implementation
  items, one task-index entry and 13 distinct primary-source links. The modified
  progress/index files passed `git diff --check`.
- At research completion the implementation checklist was unchecked. Execution
  subsequently split into TASK149–160; their current evidence supersedes this
  documentation-only research snapshot.

## Implementation status — 2026-10-13

All 12 source workflows are implemented and locally verified through scoped native
modules and lazy stepped dialogs. Prospective attachment/calendar planning, whole
native batch reports and quota/account-switch audit fixes are complete. Final
checks pass 417 native cases, 480 frontend cases, production native check, native
formatting and unchanged bundle budgets. The real generated-video decoder case
passes separately. TASK161 adds single and one global matching-issue retry.
The leaf records provide source boundaries, screenshots and release follow-ups.

Perceptual matching remains an explicitly experimental local feature. Synthetic
fixtures expose a hue-shift false-positive limitation; representative accuracy/
performance and release approval remain required before distribution. Real OS
notice/tray/login behavior, live provider workflows and packaged playback are
not established by the local fixtures. No release/push/personal credential or
media actions occurred.

## Implementation checklist

- [x] 148-A — Verified source relocation
- [x] 148-B — Completion checklist and targeted repair/report
- [x] 148-C — Whole-batch dry run
- [x] 148-D — Metadata presets and manifests
- [x] 148-E — Thumbnail/caption attachments
- [x] 148-F — Publication calendar
- [x] 148-G — Network/time/priority controls
- [x] 148-H — Quota-aware planner
- [x] 148-I — Folder rules/ready markers
- [x] 148-J — Notifications/tray
- [x] 148-K — Protected profiles
- [x] 148-L — Experimental local perceptual candidates; release gate remains open

## Final local verification receipt

Final local verification on 2026-10-13: 417 native tests passed, zero failed, six explicitly ignored (five release benchmarks and the opt-in decoder fixture). The real generated-video decoder fixture was invoked separately and passed in 15.57 seconds. All 480 frontend tests across 64 files, strict TypeScript, production native check, native formatting, Tailwind, whitespace and unchanged initial bundle budgets passed. Operation/packaging-script fixtures: 40 passed, one Unix-only check skipped on Windows. Merged UI: 189 browser assertions, zero console errors/warnings. These are source and controlled-fixture results; live Google/YouTube, real OS tray/notice/login behavior, packaged playback and other-platform runtime remain unverified. No personal credentials or media were used.

Initial assets: 223.20 KiB raw JavaScript, 69.97 KiB gzip JavaScript, 39.40 KiB raw CSS; original 235/70/40 KiB budgets are unchanged. Lazy entry points retain named exports while using direct default imports to reduce initial loader overhead. Cold empty startup retains its 110-statement limit through atomic deferred runtime-index installation before recovery/admission.

Evidence root: `C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/`. Final logs/budget/build receipts are under `task148-implementation-20261013/`; merged browser evidence is `merged-workflow-20261013/qa-evidence.json`. Advanced planning, prospective assets, completion batch reports, profiles, publication, controls, duplicate candidates and retry each retain inspected light/dark desktop/mobile screenshots in their named directories.

The existing sidecar junction points at an unavailable F: drive and was preserved. Native tests/check use an explicit local harness resource override, which does not establish normal installer packaging. Windows portable archive fixtures validate decoder bytes/architecture and all six archive members/notices; no GitHub publication or release was performed. Authorization-stage candidate reads are excluded from selected-profile quota observations until an explicit candidate-owner contract exists; the planner labels estimates incomplete and actual remaining quota unknown.
