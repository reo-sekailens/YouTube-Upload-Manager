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
  `youtube-upload-manager-performance-harness.exe`, 20,100,096 bytes, SHA-256
  `7C49DA881BEFED4BF6B655386519C1512066F4AB45154EE08C18E5D8EFBFFE43`.
  Its unsigned NSIS installer is 26,704,228 bytes, SHA-256
  `3DB142E5B55C386013782C966AFDB170F4DFC306E0B32603B403894E5CA4F90B`;
  build-manifest SHA-256 is
  `AFACFC0FD5FEFD5AB0EC4FA2C647A3EBC0742EC785609861CEF8D0D414C9910F`.
- The frozen source snapshot is
  `I:\YouTube-Upload-Manager-Certification-20260823\source-current-main-48fa-task114`.
  Its manifest records commit `68a69935160d27e20c21d54b71922fbd92739a9a`,
  dirty-status SHA-256
  `35F110ADC3F3A861CCB49DB191C462C497A465135A431F88920676A448704E09`,
  and later containing `main` commit
  `555b1fac5e8f89a7ab6bc53f6f3a83b8a3a54e77`. Later Tailwind/action-icon UI
  migrations are outside this final-v3 artifact.
- The authoritative empty-profile report contains two reversed 40-run blocks,
  five untimed warmups per mode, 80 measured cold and 80 measured warm launches,
  nearest-rank statistics, raw chronological receipts, and zero removed
  outliers. The JSON is 811,056 bytes with SHA-256
  `B5A65FE21783FBD6197BDFA1444A0FEE92CF1EBB2D22087E16FEC3650A95DCFA`;
  Markdown SHA-256 is
  `B80CA0D8A4677B94BF570FF333E0B4083DB4071FE36ADD34BD8DACB98DE530D5`.
- On the healthy-headroom `I:` SATA HDD, empty-profile safe-shell paint was
  3,098/3,806 ms cold and 323/613 ms warm (p50/p95); native readiness was
  2,588/3,155 ms and 1,060/2,419 ms; first Batch paint was 4,628/5,270 ms and
  1,652/2,983 ms. All six settled-idle counter maxima were zero.
- One preceding final-v3 empty attempt was correctly rejected during block 2
  because one native idle duration fell outside 1,900–2,200 ms. No aggregate
  was emitted. Its note SHA-256 is
  `48D730623D0915AB4C8681E497E5260BB3019FB4572BB32363148187BBE66162`.
- The package was `NotSigned` and used the compile-time isolated performance
  profile. This closes the instrumented unsigned-package slice only, not a
  standard unsigned production install, signed production, or live provider.
- The authoritative interrupted report is
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\final-v3-interrupted-authoritative\output\packaged-windows-interrupted-256gb.json`
  (813,880 bytes, SHA-256
  `0E70B5AE291B3F6B0DCE14AF7C50FED9BEBE84A86FAEE2B6A58F6493A3244953`);
  its Markdown SHA-256 is
  `2363FFF3F01A45A80272BD8FCD311F354D65A3E57D678139D910C278D351D1AE`.
- Across 80 cold and 80 warm interrupted launches, every safe-shell receipt was
  present and all six settled-idle counter maxima were zero. Cold safe-shell
  p50/p95 was 3,029/3,458 ms, native-ready 1,225/1,695 ms, and first Batch
  4,865/5,485 ms; warm values were 407/621, 991/1,339, and 1,955/2,441 ms.
- Every clone began with one `uploading` row declaring 256,000,000,000 bytes and
  zero media/sensitive bindings, then passed `quick_check` with one
  `needs_reconciliation` item, zero `uploading` items, and two recovery audit
  receipts. Cold safe-shell delta versus final-v3 empty was -2.23%/-9.14% and
  first-Batch was +5.12%/+4.08% p50/p95, inside the 10% materiality gate.
- The earlier `current-*` v1 matrices remain failed diagnostics: interrupted
  first-Batch was +14.10%/+16.79% versus the bracketing empty mean. The repair
  reuses one database connection for interrupted rows, skips secure-store
  lookup for unscoped rows, and pre-scans legacy UUID media before DB open.

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
  p50/p95/max 33/67/74 ms; 40 Batch clears had 33/67/67 ms; all 80 chronological
  samples mounted at most 32 rows; maximum interaction long task was 0 ms; and
  runtime errors were zero. JSON SHA-256 is
  `A9553213B8FA512262F7C9E35E20AB7EDAD00F9EB464E1C22C0742509383EAC8`;
  screenshot SHA-256 is
  `3AA8463CE37188ECC03A6B8996C8DAEF54CF4074797F0A99F9FA47D99EAFD948`.
- This closes the local browser interaction-response slice only. It does not
  substitute for packaged startup, WebView2, signed production, or live Google
  and YouTube evidence.

## Certification state

- Frozen local source/build: passed — TypeScript; 16 frontend files/87 tests;
  140 native tests, zero failed, five ignored release-only benchmarks; 6/6
  FFprobe tests; frontend bundle gate at 228,995 B raw / 71,496 B gzip and CSS
  at 38,470 B.
- Local browser: passed for the 10,000-row Batch interaction slice only.
- Unsigned instrumented Windows package: empty and interrupted-profile startup,
  recovery, integrity, and settled idle passed.
- Standard unsigned Windows production artifact integrity: passed. Executable
  `50D06CCA95824CF94974D57854E97E196E0220AC2737CD09829EDF340B983FB4`
  (20,057,088 bytes), NSIS
  `5137ED03E448B9F0263C4A929E490DCF8AAABB0BA51CF169D49C34DB5513293F`
  (26,686,021 bytes), and portable ZIP
  `DC49C1BE9BC53A3146878BA1954C8C6A88A03C90473D05A5BB171188E75A10DB`
  (37,601,064 bytes) were verified. The 1,941-byte production manifest SHA-256
  is `D01D32999E9F551F5C86C5088E6B7B0396672B38D64C6F3C268F9AF519FFED7F`.
  PE machine was `0x8664`; ZIP readback
  matched the app, FFprobe, and license; the runner rejected the ordinary
  executable as non-harness.
- Standard unsigned final-v3 executable launch: not exercised. The historical
  exact-path 81-item smoke belongs to an earlier ordinary artifact and is not
  evidence for the final-v3 executable.
- Signed Windows production: unavailable; no local code-signing certificate.
- macOS, Linux, Android, and iOS packaged runtime: unavailable on this host.
- Live Google OAuth/YouTube: not exercised; no approved non-production canary
  account/client was supplied.
- Equivalent healthy-headroom NVMe comparison: unavailable. The historical
  reference used a non-equivalent first-Batch marker and storage/WebView profile,
  while the only local NVMe volume failed the 10%-free-space rule. No 50%
  startup or NVMe copy-speed claim is made from those incomparable samples.
- TASK112 is complete for the available source/browser/local-native and
  unsigned Windows instrumented-package plus production-artifact-integrity
  scope. The final-v3 ordinary launch and unavailable signed, platform,
  provider, and equivalent-NVMe levels remain explicit evidence gaps.
