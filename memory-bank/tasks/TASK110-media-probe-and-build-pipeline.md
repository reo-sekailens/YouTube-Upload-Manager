# TASK110: Media probe and build pipeline

## Status

completed

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

## Evidence

- `scripts/prepare-ffprobe.mjs` now streams checksum verification, pins binary
  and license digests, writes an identity-bound provenance receipt, selects one
  host architecture unless universal macOS is explicit, and exits before
  filesystem/network work for mobile targets.
- `scripts/prepare-ffprobe.tests.mjs` covers target selection, mobile skip,
  streamed cold preparation, receipt reuse without network or rehash,
  provenance invalidation, and checksum-failure artifact safety; 6/6 passed.
- On the reference Windows cache, first receipt creation took about 4.5 s and a
  verified receipt reuse took 0.66 s including npm startup.
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
- A healthy-headroom 40-pair run on the `I:` SATA HDD rejected the 8 MiB plus
  sequential-scan copy candidate: it regressed 3.70% at p50 and 18.61% at p95
  against the simultaneously paired 1 MiB standard-open reference. A focused
  40-pair follow-up also rejected 1 MiB plus sequential scan at +3.79%/+10.35%.
  The production copy path therefore uses the fastest tested safe,
  non-regressing variant: a 1 MiB heap buffer with standard opens.
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
- The corrected copy path retains final `sync_all`, exact partial
  length/digest reconstruction, scheduler admission/yielding, cancellation,
  and resume behavior. Across the retained healthy-HDD evidence, all 80 copies
  in each paired campaign completed `sync_all`, copied exactly 67,108,864
  bytes, and matched one BLAKE3 digest; the separate interruption case reopened
  at full size and matched the source digest.
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
- The authoritative unsigned Windows performance package contains the x64
  FFprobe PE (`3A7E2DC003DC2CD1472827E4C7C4F056AE1AE0AE7C5BBC580C99B49827351BA4`,
  82,668,032 bytes) and license
  (`8CEB4B9EE5ADEDDE47B31E975C1D90C73AD27B6B165A1DCD80C7C545EB65B903`,
  35,147 bytes).
- The final ordinary unsigned portable ZIP read back exactly the x64 production
  executable, FFprobe, and license. The extracted application hash matched the
  retained executable, the PE machine was `0x8664`, and the sidecar/license
  hashes and sizes matched the pinned values above.
- Healthy-headroom NVMe throughput remains unverified because the only local
  NVMe volume did not satisfy the 10%-free-space rule. The earlier near-full
  C: samples are diagnostic only; no equivalent-NVMe speed claim is made.
- Retained reports:
  `I:\YouTube-Upload-Manager-TASK110-durable-copy-20260823\REPORT.md`
  (SHA-256 `4331EF3590A2AE8F8285B9D1D4CBCFBF74E32ED6BA85BDBD210059C152FFC1E6`)
  and `I:\YouTube-Upload-Manager-TASK110-durable-copy-20260823\hdd-1mib-sequential-followup\FOLLOWUP-REPORT.md`
  (SHA-256 `BF60C1C2C280E364998A8D82EE9575A313BE37B430052C9A4072F4618C4FC3A1`).
