# TASK112: Packaged performance certification

## Status

completed

## Objective

Prove the optimized app starts and operates quickly in real packaged conditions
without conflating source, browser, mock-provider, signed-build, or live-provider
evidence.

## Scope

- Run repeatable cold/warm Windows packaged startup, first-interaction, idle,
  large-library, interrupted-recovery, import, monitor, preflight, and mock
  resumable-upload scenarios.
- Capture p50/p95 timings, RSS, CPU/wakeups, disk/network volume, database opens,
  worker/process peaks, bundle/binary/installer size, and long tasks.
- Add deterministic CI gates for bundles, query plans, fixture algorithms,
  worker bounds, idle behavior, and regressions; keep variable wall-clock gates
  non-blocking until stable.
- Re-run safety, account isolation, recovery, secure-store, resumable checkpoint,
  duplicate review, and destructive-action certification.
- Record macOS, Linux, Android, and iOS results only when the required hardware,
  toolchain, and package are actually available.
- Keep an explicitly authorized non-production YouTube canary separate from
  local mock throughput and local package certification.

## Acceptance criteria

- The completed report compares TASK103 and final p50/p95 results only when
  hardware, storage headroom, fixture, WebView isolation, and milestone
  semantics are equivalent, and explains material variance where they are not.
- Cold and warm startup are reported with an actual acknowledged safe-shell
  milestone; a simulated interrupted 256 GB fixture size does not materially
  affect first safe-shell time.
- Settled idle, large-list responsiveness, upload throughput, bounded memory,
  query, event, and worker/process budgets all pass.
- Current frontend/native suites, recovery fixtures, and packaged Windows smoke
  tests pass with no weakened privacy, security, or deletion boundary.
- Source/build, unpackaged runtime, packaged unsigned, signed production, and
  live-provider evidence are reported as distinct certification levels.
- No benchmark artifact contains credentials, channel IDs, filenames, source
  paths, media contents, or provider payloads.

## Dependencies

TASK103, TASK104, TASK105, TASK106, TASK107, TASK108, TASK109, TASK110, TASK111.

## Evidence destinations

memory-bank/certification/, TASK102, TASK112, progress.md, and release evidence
for each actually exercised platform.

## Implemented harness fixture slice

- The Windows packaged runner requires an explicit `empty` or
  `interrupted-256gb` fixture. The interrupted template contains one synthetic
  pre-existing `uploading` row with a declared size of 256,000,000,000 bytes,
  empty local path fields, no channel, credential, secure-store session,
  provider identifier, or media, and a reported media footprint of zero bytes.
- Fixture insertion exists only in a `performance-harness` build. A second
  seed-only environment gate inserts the row transactionally and idempotently
  into a marker-protected isolated profile, writes redacted cardinality/size
  metadata, and exits before startup recovery. Regular builds neither read nor
  react to either fixture environment variable.
- The seed process is untimed. After it exits, every cold, warmup, and warm
  launch receives a separate clone of the closed template. The measured process
  explicitly removes both seed variables, begins with the pre-existing
  `uploading` row, and exercises normal database-only interrupted-upload
  classification without creating or reading a 256 GB file.
- Clone and template cleanup requires both containment beneath the explicitly
  empty disposable root and the performance-profile marker. The output contains
  fixture ID, counts, declared bytes, zero media bytes, and booleans only; it
  does not copy the synthetic SQLite profile into the report directory.
- The settled-idle interval is fixed at two seconds. A run fails if its final
  native snapshot omits any delta for periodic invokes, database opens, SQLite
  statements, event messages, worker threads, or FFprobe processes.
- The Windows harness builder invokes the installed Tauri JavaScript CLI with
  `process.execPath` and no command shell. This avoids Node 24's Windows
  `spawnSync` failure on `npm.cmd` while preserving feature, bundle, signing,
  environment, and caller-supplied arguments.

## Harness fixture evidence

- PowerShell parser validation passed, and `-Help` exposes only the two accepted
  fixture names.
- A validate-only interrupted-fixture proof returned `valid: true`,
  `declaredTotalBytes: 256000000000`, `mediaBytesWritten: 0`, cloned-template
  and measured-environment-removal receipts, plus all six settled-idle delta
  dimensions. The temporary fake-signature executable/profile used only for
  argument validation was removed afterward.
- `node --check scripts/performance/build-windows-harness.mjs` passed, and the
  focused launcher/bundle-baseline test file passed 3/3. Its launcher regression
  asserts the Node executable, installed Tauri CLI path, complete argument and
  environment propagation, plus the absence of `npm.cmd` and shell execution.
- The authoritative unsigned instrumented package is
  `youtube-upload-manager-performance-harness.exe`, 20,034,048 bytes, SHA-256
  `6CEC897CFE1D83FE7B4F73452974E7ACD7668C4158BF234086DCB7CE4163CB2D`.
  Its unsigned NSIS installer is 26,685,288 bytes, SHA-256
  `04D7E400EAB5146756AA40697A90DEE3C07F99CB56933F105310A3DAC4BB1D59`.
- The authoritative empty-profile report contains two reversed 40-run blocks,
  five untimed warmups per mode, 80 measured cold and 80 measured warm launches,
  nearest-rank statistics, raw chronological receipts, and zero removed
  outliers. The JSON SHA-256 is
  `7C21E5DE0BEA98ED9B583B7A994208C5209E903CE6927D08CF0A11E91F98F90E`.
- On the healthy-headroom `I:` SATA HDD, empty-profile safe-shell paint was
  2,970/3,286 ms cold and 158/406 ms warm (p50/p95); native readiness was
  1,865/2,321 ms and 503/743 ms; first Batch paint was 4,103/4,378 ms and
  967/1,198 ms. All six settled-idle deltas were zero in every run. Cold and
  warm SQLite statement counts were exactly 109 and 23 respectively.
- The package was `NotSigned` and used the compile-time isolated performance
  profile. This closes the instrumented unsigned-package slice only, not a
  standard unsigned production install, signed production, or live provider.
- The authoritative interrupted report is
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\interrupted-v4-authoritative\output\packaged-windows-interrupted-256gb.json`
  (813,009 bytes, SHA-256
  `947088598093BB283A20AEB7C9E9DFE851F7FB9533007D68034EC1739AAAF5B1`);
  its Markdown SHA-256 is
  `CC44686F901A8DC8E18DA9985C2BC6F651FCC6D3BC49B65BF2D4080F1A6D4279`.
- Across 80 cold and 80 warm interrupted launches, every safe-shell receipt was
  present and all six settled-idle deltas were zero. Cold safe-shell p50/p95
  was 2,899/3,449 ms, native-ready 787/1,019 ms, and first Batch
  4,370/4,727 ms; warm values were 305/720, 558/1,021, and 1,550/2,035 ms.
- Every clone began with one `uploading` row declaring 256,000,000,000 bytes and
  zero media/sensitive bindings, then passed `quick_check` with one
  `needs_reconciliation` item, zero `uploading` items, and two recovery audit
  receipts. Cold first-Batch delta versus empty was +6.51%/+7.97% p50/p95,
  inside the 10% materiality gate.

## Browser interaction certification slice

- A separate local Chromium runner now mounts the real Batch `QueueTable` with
  10,000 synthetic in-memory upload records, performs five untimed warm-up
  pairs, then measures 40 actual search inputs and 40 actual clear-button
  clicks through the React event path. Each receipt waits for the deferred
  result and two animation frames and records interaction-local Long Tasks API
  evidence. It never opens SQLite, an operator profile, media, secure storage,
  or a provider connection.
- The shared client-side data window is 32 records rather than 48. The narrower
  window removed observed 50-56 ms clear-result render tasks while preserving
  pagination, complete result counts, and a bounded real-data view.
- Windows Edge 151 evidence on a 1416x1108 viewport passed: 40 searches had
  p50/p95/max 44/87/91 ms; 40 Batch clears had 42/85/96 ms; all 80 chronological
  samples mounted at most 32 rows; maximum interaction long task was 0 ms; and
  runtime errors were zero. JSON SHA-256 is
  `D108AB1207285E04C7FD046A4DE25321357D44245CCA7E1806D194A81917236F`;
  screenshot SHA-256 is
  `35822003D9DC703FDCC024818BC91678E9B17C158C9AE5A4D0612C48666ED3DF`.
- This closes the local browser interaction-response slice only. It does not
  substitute for packaged startup, WebView2, signed production, or live Google
  and YouTube evidence.

## Certification state

- Local source/build: passed — 75/75 frontend tests; 127 native tests, zero
  failed, five ignored release-only benchmarks; 6/6 FFprobe tests; frontend
  bundle gate at 230,478 B raw / 71,657 B gzip and CSS at 38,470 B.
- Local browser: passed for the 10,000-row Batch interaction slice only.
- Unsigned instrumented Windows package: empty and interrupted-profile startup,
  recovery, integrity, and settled idle passed.
- Standard unsigned Windows production package: passed. Executable
  `84F39ED1F7827C43BAB5DD2481D503F321D0A1A8A0BB9D816A955BBE7C2C4AA9`
  (19,988,480 bytes), NSIS
  `4D40D8DDD5FD4A4A7A0213F0E81006D93FBF5CE8DA312DB4462E12071D89E5C8`
  (26,669,567 bytes), and portable ZIP
  `01D9C0F691844052F9D24700DCFFCD98C5F087028EB1A52400303E68D45B712E`
  (37,565,048 bytes) were verified. PE machine was `0x8664`; ZIP readback
  matched the app, FFprobe, and license; the runner rejected the ordinary
  executable as non-harness.
- Standard unsigned desktop smoke: passed at the exact retained executable/PID.
  The active Batch workspace rendered a real 81-item profile as `1–32 of 81`.
  No interaction or provider action was performed, and the exact process exited.
- Signed Windows production: unavailable; no local code-signing certificate.
- macOS, Linux, Android, and iOS packaged runtime: unavailable on this host.
- Live Google OAuth/YouTube: not exercised; no approved non-production canary
  account/client was supplied.
- Equivalent healthy-headroom NVMe comparison: unavailable. The historical
  reference used a non-equivalent first-Batch marker and storage/WebView profile,
  while the only local NVMe volume failed the 10%-free-space rule. No 50%
  startup or NVMe copy-speed claim is made from those incomparable samples.
- TASK112 is complete for the available source/browser/local-native and
  unsigned Windows scope. The unavailable signed, platform, provider, and
  equivalent-NVMe levels remain explicit external evidence gaps.
