# TASK109: Inventory, dedupe, and batch pipelines

## Status

completed

## Objective

Make large-channel inventory, duplicate review, preflight, folder observation,
and multi-file intake scale with changes and results instead of repeated
all-record or pairwise work.

## Scope

- Stage inventory pages with prepared statements in one bounded transaction and
  atomically promote a complete channel generation.
- Persist normalized, canonical-copy, and numeric-sequence title keys; replace
  all-pairs duplicate comparison with grouped candidate generation.
- Rebuild revisioned duplicate projections only when the channel inventory or
  local-upload generation changes.
- Share one sufficiently fresh inventory generation across a dispatch wave
  without weakening the final native pre-upload gate.
- Materialize preflight match evidence on worker completion; page files and
  activity, and load deep metadata only on expansion.
- Bulk-load folder observations and upload state rather than querying once per
  discovered file.
- Replace sequential frontend import/queue IPC waterfalls with batch commands
  that return receipts and schedule per-item disk work through bounded workers.

## Acceptance criteria

- Existing exact, canonical trailing-copy `(2)`, numeric-sequence, and exact
  digest semantics remain explainable and are protected by golden tests.
- A 10,000-video duplicate rebuild and unchanged dashboard read meet TASK103
  CPU/query budgets without quadratic pair comparison.
- A 1,000-file preflight against 10,000 inventory rows has a status-read p95
  below 50 ms and a bounded payload below 256 KiB on the reference fixture.
- A 100-file import/queue submission uses O(1) webview/native round trips while
  reporting independent per-item validation failures.
- Inventory promotion is transactional; partial provider pages never replace a
  previously complete generation.
- Every table, key, projection, batch receipt, and result remains scoped to the
  immutable channel/account identity.

## Dependencies

TASK103, TASK104, TASK108.

## Affected areas

Remote inventory, title matching, dashboard projections, preflight, watched
folders, batch bridge commands, persistence indexes, and scale fixtures.

## Implemented

- Schema v4 persists versioned normalized, canonical-copy, and numeric-sequence
  title keys on local uploads, active inventory, and staged inventory pages.
  Key generation is centralized in `title_matching.rs`; final candidate
  evidence still runs the exact matching rules rather than trusting an index
  hit. Channel-scoped upload and inventory generations invalidate the durable
  duplicate projection only when source data changes.
- Inventory pages reuse prepared statements inside bounded transactions. A new
  snapshot stays in staging until the last page succeeds, then one transaction
  replaces only the matching immutable channel's active generation, advances
  its generation counter, and leaves every other channel untouched. An
  interrupted provider pagination run therefore cannot displace the previous
  complete snapshot.
- Uploaded-title and queue-title checks load persisted candidate keys into a
  channel-scoped index once, group potential matches by exact key, and run the
  explainable evidence function only on the resulting candidates. Unchanged
  dashboard reads use the materialized duplicate projection instead of
  rebuilding every title pair.
- Preflight completion materializes file evidence, full category counts, and a
  bounded preview once. Status reads return counters only; file results and
  activity use independently bounded native pages; rich metadata and source
  details load for one record only after explicit expansion. Legacy completed
  jobs are materialized on their first paged read.
- Manual multi-file intake now crosses the webview/native bridge once through
  `import_and_queue_batch`. Native code validates and imports each item with an
  independent redacted receipt, performs one inventory/title check, queues the
  accepted items in one SQLite transaction, and starts dispatch once. No source
  locator is returned in the batch receipt.
- Watched-folder discovery bulk-loads existing channel observations before
  walking the selected folder, removing the per-file SQLite lookup pattern.
  Preflight progress increments its durable count and reconciles once on
  recovery instead of executing a count query for every processed file.

## Evidence

- `npm run check` passed on the integrated frontend.
- `npm test -- --run src/lib/local.test.ts
  src/components/large-list-rendering.test.tsx` passed **28/28** tests. The
  bridge regression submits exactly 100 paths and asserts exactly one native
  invoke; the large-list fixture supplies 10,000 records while rendering fewer
  than 100 rows. Separate regressions prove that compact status, bounded pages,
  and expanded metadata use independent commands.
- Native `cargo check --manifest-path src-tauri/Cargo.toml --lib` passed at the
  integrated TASK109 checkpoint. The final performance-harness `cargo test`
  passed **127 tests**, with zero failures and five release-only benchmarks
  ignored. The first attempt had stopped before tests on a TASK111 benchmark
  import; its owner repaired that compile-only seam before the passing run.
- Native golden regressions cover exact titles, trailing `(2)`
  semantics, the deliberately non-copy `(1)` case, multi-sequence numeric
  matching, independent batch failures without returned source paths, and
  persisted-projection generation caching plus channel isolation; all are
  included in the passing frozen-tree suite.
- Release-only local fixtures passed their executable gates. At 10,000
  inventory/local rows, the combined unchanged dashboard read and duplicate
  projection path measured p50 **137.866 ms** and p95 **149.679 ms**. A
  1,000-file preflight against 10,000 processed inventory rows measured compact
  status p50 **3.629 ms** / p95 **4.966 ms** and a 48-file/48-activity page p50
  **12.162 ms** / p95 **17.844 ms**. The maximum serialized page across warmup
  and seven measured samples was **21,945 bytes**, below the **262,144-byte**
  budget.
- All evidence above is local TypeScript, Rust, SQLite, and fixture evidence.
  No Google OAuth, live YouTube inventory, provider pagination, or internet
  throughput was exercised or inferred.

## Follow-ups

- Paged Batch rendering is covered by the final 10,000-row browser harness;
  populated preflight deep-metadata expansion remains a rendered-app follow-up.
- TASK112 closes available unsigned Windows startup and standard-package smoke,
  including the real 81-item Batch page bound. Local data fixtures still do not
  establish live Google/YouTube inventory or provider timing; an approved
  non-production canary remains unavailable.
