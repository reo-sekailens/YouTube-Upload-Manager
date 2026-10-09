# TASK142 — Original-file uploads without media copies

## Status

completed (local source and browser scope)

## Objective

Honor the operator's explicit storage constraint: do not make app-owned video
copies. Restore reference-in-place manual and watched intake while preserving
TASK141's channel isolation, usability, and operations improvements.

## Acceptance criteria

- Manual and watched intake create no `.media`, `.partial`, hard link, reflink,
  or clone; persist the original locator/signature and queue identity instead.
- Retry and forced-close recovery retain IDs and protected provider checkpoints,
  verify original-source integrity, and reconcile before sending more bytes.
- Missing/changed originals fail safely; no automatic copying fallback occurs.
- Previously owned media referenced by existing work is preserved if it is still
  needed for safe recovery. No operator profile or original file is modified by
  this source change.
- UI explains that original files must remain available and unchanged until the
  upload completes; screenshots show the revised instructions.
- Repository guidance and current architecture agree with the new user decision.
- Focused native/frontend checks, current webview build/bundle limits, and local
  rendered screenshots document results separately from live/provider evidence.

## Decision

The user's instruction on 2026-10-09 overrides AGENTS.md's former default-copy
requirement. AGENTS.md and the project brief are amended so later work preserves
this preference. Queue records, hashes, and secure resumable sessions remain
device-local; new video bytes are read from the original file only.

## Evidence

- Manual intake persists `source_path = workspace_path`, stable size/modified
  signature, channel binding, and pending background BLAKE3 verification without
  writing media. Watched intake similarly references the stable original.
  Production has no automatic copy/migration helper calls.
- Retry and startup tests retain IDs, immutable channel, digest/confirmed offset,
  and checkpoint data without new media files. A moved/changed original is
  withheld before provider dispatch. Referenced legacy `.media`/`.partial`
  files survive cleanup even when their original disappeared.
- Eight focused native tests passed, each with one exact named-test receipt.
  Integrated native suite: 154 passed, zero failed, five intentionally ignored
  release-only benchmarks (159 discovered). Ordinary Rust check and formatting
  passed; existing dead-code warnings remain.
- Integrated frontend suite: 24 files, 119 passed, zero failed. TypeScript,
  Tailwind source guard, and production webview build passed. Initial bundle
  limits pass unchanged at 228,557 B JavaScript raw, 71,461 B gzip, and 39,649 B
  CSS raw. The remaining initial gzip margin is 219 B.
- Eight browser checks passed for Batch uploads and Folder monitor at 1440/390 px
  in light/dark themes. Revised guidance was present without stale copy promises
  or root horizontal overflow; no runtime console errors appeared. Records were
  synthetic and no native/provider upload was executed through the browser.
- Receipts: `output/task142-native-focused.log`,
  `output/task142-native-tests.log`, `output/task142-native-validation.json`,
  `output/performance/task142-frontend.json`, and
  `output/playwright/task142-browser-check.log`, and
  `output/task142-native-check.log`.
- Screenshots: `output/playwright/task142-original-file-intake.png` and
  `task142-{batch-uploads,folder-monitor}-{light,dark}-{1440,390}.png`.
- No installed-profile cleanup, release, signing, or live Google/YouTube mutation
  is included. Existing broken output/toolchain/sidecar junctions remain intact;
  checks use isolated local paths and process-only Tauri configuration excluding
  sidecars/resources. This does not certify ordinary packaged launch.

## Follow-ups

- Keep original files at their saved locations and unchanged until their work
  completes. Restoring the same verified original permits normal retry; the app
  does not reconstruct missing videos or copy a substitute.
- Interrupted legacy managed imports retain their partial evidence for review;
  they do not restart a copy. Reclaiming referenced legacy storage is a separate
  explicit workflow, not an automatic side effect of this source change.
- Certify packaged launch, actual forced-close recovery, and explicitly scoped
  live-provider flows separately from these local results.
