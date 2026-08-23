# TASK110: Media probe and build pipeline

## Status

in_progress

## Objective

Remove redundant FFprobe, hash, copy, and provisioning work from development,
builds, startup, and intake while keeping media validation and packaged sidecar
integrity explicit.

## Scope

- Stream FFprobe checksum verification and persist a local provenance receipt
  keyed by binary identity, checksum, license, target, and preparation version.
- Make a valid cached preparation network-free, skip mobile targets entirely,
  and select one desktop architecture unless universal packaging is explicit.
- Bound all runtime FFprobe processes and output readers through the shared
  native resource scheduler.
- Add a duration-only probe for upload-limit validation; reuse richer metadata
  only for the same stable file signature.
- Measure single-pass copy-plus-hash and safe platform clone/offload options
  without weakening managed-workspace or watched-source guarantees.
- Include the correct sidecar and license in installer and portable artifacts
  and validate their target architecture.

## Acceptance criteria

- Repeated preparation of a verified cache reads no network resource and avoids
  a full binary hash unless provenance inputs changed.
- Mobile development/build performs zero desktop FFprobe provisioning work;
  ordinary desktop preparation produces only the selected architecture.
- Runtime FFprobe/hash/copy concurrency is bounded, cancellable, and lower
  priority than an active upload where the source volume is shared.
- Duration and supported-format decisions remain identical on the media fixture
  corpus, including malformed and oversized provider output.
- Copy/hash throughput and responsiveness meet TASK103 budgets without asking an
  operator to reselect an already queued asset after interruption.
- Fresh Windows installer and portable artifacts contain the verified FFprobe
  binary and license; unavailable platform packaging is recorded separately.

## Dependencies

TASK103.

## Affected areas

scripts/prepare-ffprobe.mjs, Tauri build hooks/configuration, native media probe,
ingest/hash/copy scheduler, packaging workflow, and media fixtures.

## Evidence to date

- `scripts/prepare-ffprobe.mjs` now streams checksum verification, pins binary
  and license digests, writes an identity-bound provenance receipt, selects one
  host architecture unless universal macOS is explicit, and exits before
  filesystem/network work for mobile targets.
- `scripts/prepare-ffprobe.tests.mjs` covers target selection, mobile skip,
  streamed cold preparation, receipt reuse without network or rehash,
  provenance invalidation, and checksum-failure artifact safety; 6/6 passed.
- On the reference Windows cache, first receipt creation took about 4.5 s and a
  verified receipt reuse took 0.66 s including npm startup.
- Runtime FFprobe scheduling/probe-cache work and final installer/portable
  artifact inspection were previously open.
- Runtime media work now has one shared scheduler with two FFprobe slots and one
  copy/hash reader per source volume. Active upload volumes take probe priority,
  foreground/background reads yield in bounded intervals, and waits/processes
  observe cancellation. Watched-hash cancellation reuses one connection and
  throttles its status query to 250 ms; the 10,000-callback regression performs
  one underlying check.
- Upload-limit fallback now asks FFprobe only for duration with a 64 KiB output
  cap. Rich metadata keeps its 2 MiB cap and is reused only for an unchanged
  canonical-path, size, and nanosecond-mtime signature. The 256-entry cache
  coalesces identical in-flight work and does not retain transient spawn,
  timeout, or cancellation failures.
- Focused native evidence passed: 3 FFprobe tests, 5 media/startup tests, and 3
  cache/signature/cancellation-cadence tests. This includes malformed/nonfinite
  duration output, oversized output, stable-signature invalidation, bounded
  reader/probe cancellation, upload-priority yielding, the ISO-BMFF zero-probe
  fast path, and startup classification that defers all media work.
- `cargo check --manifest-path src-tauri/Cargo.toml --lib` passed. The first full
  integrated run reached 99 passed and 5 ignored before two TASK107 fixture
  regressions; the TASK107 owner owns their repair and final full-suite rerun.
- `npm run test:ffprobe` passed 6/6; cached real preparation was network-free.
  The prepared Windows sidecar was inspected as PE machine `0x8664` and its
  35,147-byte license was present.
- The Windows nightly portable workflow now copies `ffprobe.exe` plus
  `ffprobe-license.txt`, validates x64 PE architecture, and reads back required
  ZIP entries. Tauri installer resources remain declared and mobile configs
  remain sidecar-free.
- The later integrated quiet release run confirmed a copy-path defect rather
  than a BLAKE3 regression: BLAKE3 remained stable at p50 41.713 ms / p95
  48.740 ms, while durable copy-plus-BLAKE3 reached p50 929.784 ms / p95
  1,871.217 ms against the frozen 335.852/368.178 ms reference.
- The safe production candidate increases the heap-backed copy buffer from
  1 MiB to 8 MiB and applies Windows `FILE_FLAG_SEQUENTIAL_SCAN` to source,
  partial replay, and append handles. It retains the final `sync_all`, exact
  partial length/digest reconstruction, per-volume admission, upload yielding,
  and cancellation checks. A coordinated C:-volume release rerun improved to
  p50 624.299 ms / p95 897.986 ms: 32.9% and 52.0% better than the regressed
  run, but still 85.9% and 143.9% above the frozen acceptance reference.
- Release-only phase diagnostics then isolated the remaining variance. The
  64 MiB stream/write/hash phase measured p50 74.891 ms / p95 278.429 ms,
  while the required durable `sync_all` measured p50 934.431 ms / p95
  1,073.260 ms; total copy measured 1,021.546/1,149.402 ms and standalone
  BLAKE3 33.310/36.744 ms. The build target was an F: SATA HDD, but the fixture
  itself used the OS temporary directory on C:, where only 5.982 GiB (about
  0.6% of the volume) was free. Durable flush, not hashing or user-space copy,
  therefore dominated this sample.
- `sync_all` is intentionally retained: removing the final durable boundary or
  pre-extending the partial file would weaken crash-safe resume semantics.
  `sync_data` was not substituted because a newly created partial also needs
  its length/metadata durable before it can be trusted after a crash. The phase
  timing types/functions are `cfg(test)` release-benchmark instrumentation;
  the production monomorph records no timestamps and exposes no diagnostic
  fields.
- Native first-run setup now installs the current schema directly only when
  `user_version = 0` and `sqlite_schema` proves there are no user tables. Any
  unversioned database with existing user data keeps the full transactional v0
  compatibility migration. The focused migration fixtures passed with all 20
  current state-change triggers installed and a legacy upload row/checkpoint
  preserved while its current `visibility` column was added.
- Native initialization now returns and reuses its configured SQLite connection
  for startup classification. A single consolidated presence read classifies
  fail-closed interrupted work plus import, watched-hash, preflight, folder,
  quota, and channel-scoped postprocess eligibility. A clean current-schema
  profile executes exactly one classification statement and no `BEGIN` or
  `UPDATE`; only dirty recovery enters `BEGIN IMMEDIATE` and re-reads/mutates
  durable state. Deferred recovery and worker startup skip their former no-op
  database probes only when those captured eligibility flags are false.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and the full
  `cargo test --manifest-path src-tauri/Cargo.toml --features performance-harness`
  passed using the shared F: target/temp cache: 127 passed, 0 failed, 5 ignored.
  This is local native correctness evidence, not packaged startup proof.
- This task remains `in_progress`. Fresh installer/portable extraction and
  target-architecture inspection remain required under TASK112. The copy/hash
  gate additionally needs one durable rerun with healthy volume headroom; the
  near-full-volume result is diagnostic evidence, not an accepted regression.
