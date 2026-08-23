# Performance certification — 2026-08-23

## Outcome

The app-wide performance implementation and the frozen final-v3 local/unsigned
Windows certification are **complete**. TASK103 through TASK112 pass their
available source, browser, local-native, unsigned instrumented-package, and
standard unsigned artifact-integrity slices. The final-v3 ordinary production
executable was not launched; its runtime is not inferred from an older smoke
receipt. Signed production, non-Windows packages, live Google/YouTube, and an
equivalent healthy-headroom NVMe comparison were unavailable and remain
explicitly unverified external evidence boundaries.

No benchmark artifact contains credentials, OAuth responses, channel IDs,
source paths, filenames, media contents, resumable-session URIs, or provider
payloads.

## Evidence-level matrix

| Level | Status | What is established |
| --- | --- | --- |
| Local source/build | passed | TypeScript passed; 16 frontend files/87 tests passed; 140 native tests passed, 0 failed, 5 ignored release-only benchmarks; 6/6 FFprobe tests; deterministic bundle/query/worker gates |
| Local browser | passed, scoped | Real Batch `QueueTable`, 10,000 synthetic rows, 40 search and 40 clear interactions; not SQLite/WebView2/provider proof |
| Unsigned instrumented Windows package | passed | Empty and interrupted-256 GB Tauri/WebView2 startup, safe-shell receipt, Batch paint, SQLite integrity/cardinality, settled idle, storage provenance |
| Standard unsigned Windows production | artifact integrity passed; launch not exercised | Exact final-v3 hashes, `NotSigned`, x64 PE, portable contents/readback, and harness isolation verified; no final-v3 ordinary executable launch |
| Signed Windows production | unavailable | No local code-signing certificate |
| macOS/Linux/Android/iOS package | unavailable | Required host/toolchains/devices were not available |
| Live Google/YouTube | not exercised | No approved non-production account/OAuth canary was supplied |

## Final local correctness and deterministic gates

- Frozen source snapshot:
  `I:\YouTube-Upload-Manager-Certification-20260823\source-current-main-48fa-task114`.
  Its manifest records commit
  `68a69935160d27e20c21d54b71922fbd92739a9a`, dirty-status SHA-256
  `35F110ADC3F3A861CCB49DB191C462C497A465135A431F88920676A448704E09`,
  and later containing `main` commit
  `555b1fac5e8f89a7ab6bc53f6f3a83b8a3a54e77`.
- The final-v3 artifacts intentionally exclude the later Tailwind and action-
  icon UI migrations now present in the live repository. Those changes are
  outside this frozen performance certification and require their own evidence.
- Frontend: TypeScript, production build, 16 test files/87 tests, and payload
  gate passed.
- Initial frontend payload: 228,995 B JavaScript raw, 71,496 B JavaScript gzip,
  and 38,470 B CSS raw. Budgets are 240,640 B, 71,680 B, and 40,960 B.
- Native: formatting and diff checks passed; the isolated release suite passed
  140 tests with zero failures and five intentionally ignored release-only
  benchmarks.
- FFprobe preparation: 6/6 tests passed; cached preparation is provenance-bound
  and network-free, mobile preparation is skipped, and Windows target selection
  remains x64-only for this package.
- SQLite/data scale: the final 10,000-row dashboard/dedupe path measured
  137.866/149.679 ms p50/p95 versus the frozen 177,330.510 ms single sample.
  The 1,000-file/10,000-inventory fixture measured compact status at
  3.629/4.966 ms and the bounded file/activity page at 12.162/17.844 ms; maximum
  serialized payload was 21,945 B.
- Local loopback upload: 204.516/239.985 ms p50/p95 at 312.933 MiB/s, 2.0429x
  the pooled-streaming reference, with one 8 MiB request-owned buffer and a
  durable checkpoint before each next range. This is not live-provider proof.

## Unsigned packaged Windows — authoritative empty profile

Evidence:

- JSON:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\final-v3-empty-authoritative\output\packaged-windows-empty.json`
  — 811,056 B — SHA-256
  `B5A65FE21783FBD6197BDFA1444A0FEE92CF1EBB2D22087E16FEC3650A95DCFA`.
- Markdown:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\final-v3-empty-authoritative\output\packaged-windows-empty.md`
  — 2,223 B — SHA-256
  `B80CA0D8A4677B94BF570FF333E0B4083DB4071FE36ADD34BD8DACB98DE530D5`.
- Measured executable: 20,100,096 B — SHA-256
  `7C49DA881BEFED4BF6B655386519C1512066F4AB45154EE08C18E5D8EFBFFE43`.
- Harness NSIS installer: 26,704,228 B — SHA-256
  `3DB142E5B55C386013782C966AFDB170F4DFC306E0B32603B403894E5CA4F90B`.
- Build manifest: SHA-256
  `AFACFC0FD5FEFD5AB0EC4FA2C647A3EBC0742EC785609861CEF8D0D414C9910F`.

Method: two reversed blocks of 40 launches per mode, five untimed warmups,
80 measured cold and 80 measured warm runs, nearest-rank percentiles, raw
chronological receipts, no removed outliers, isolated WebView2 data per clone,
and a strict 1,900–2,200 ms settled-idle window. Storage was the healthy-
headroom NTFS `I:` SATA HDD (`WDC WD5000LPVX-22V0TT0`); the max(20 GiB, 10%)
headroom rule passed before and after.

| Empty-profile metric | Cold p50 | Cold p95 | Warm p50 | Warm p95 |
| --- | ---: | ---: | ---: | ---: |
| Safe-shell paint, acknowledged by native | 3,098 ms | 3,806 ms | 323 ms | 613 ms |
| Native ready | 2,588 ms | 3,155 ms | 1,060 ms | 2,419 ms |
| First Batch paint | 4,628 ms | 5,270 ms | 1,652 ms | 2,983 ms |

All 160 measured launches recorded a safe-shell value. The maximum settled-idle
periodic invokes, database opens, SQLite statements, event messages, worker
threads, and FFprobe processes were all zero. All SQLite `quick_check` and
fixture cardinality receipts passed.

One immediately preceding final-v3 attempt was correctly rejected during block
2 because one native snapshot fell outside the required 1,900–2,200 ms idle
duration. The runner emitted no aggregate JSON or Markdown and accepted none of
its partial timings. The retained diagnostic is
`I:\YouTube-Upload-Manager-Certification-20260823\runs\final-v3-empty-authoritative-failed-idle-window.txt`,
SHA-256
`48D730623D0915AB4C8681E497E5260BB3019FB4572BB32363148187BBE66162`.

The executable was Authenticode `NotSigned` and used the isolated
`performance-harness` profile. It is packaged runtime evidence, not a standard
production install or signed release.

## Unsigned packaged Windows — authoritative interrupted 256 GB profile

Evidence:

- JSON:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\final-v3-interrupted-authoritative\output\packaged-windows-interrupted-256gb.json`
  — 813,880 B — SHA-256
  `0E70B5AE291B3F6B0DCE14AF7C50FED9BEBE84A86FAEE2B6A58F6493A3244953`.
- Markdown:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\final-v3-interrupted-authoritative\output\packaged-windows-interrupted-256gb.md`
  — 2,266 B — SHA-256
  `2363FFF3F01A45A80272BD8FCD311F354D65A3E57D678139D910C278D351D1AE`.
- The executable, installer, build manifest, source, lockfile, FFprobe, and
  license hashes exactly match the authoritative empty-profile package.

The same two-block method measured 80 cold and 80 warm launches with no removed
outliers. The closed SQLite template contained one pre-existing `uploading` row
declaring 256,000,000,000 bytes, zero source/media bytes, no channel or secure
binding, and passed `quick_check`. Each measured clone began with that exact
cardinality and ended with the expected fail-closed reconciliation state. All
160 pre/post cardinality receipts passed, with zero sensitive bindings and
`quick_check: ok`.

| Interrupted-profile metric | Cold p50 | Cold p95 | Warm p50 | Warm p95 |
| --- | ---: | ---: | ---: | ---: |
| Safe-shell paint, acknowledged by native | 3,029 ms | 3,458 ms | 407 ms | 621 ms |
| Native ready | 1,225 ms | 1,695 ms | 991 ms | 1,339 ms |
| First Batch paint | 4,865 ms | 5,485 ms | 1,955 ms | 2,441 ms |

All 160 safe-shell values were present. The maximum settled-idle periodic
invokes, database opens, SQLite statements, event messages, worker threads, and
FFprobe processes were all zero. Storage headroom passed before and after on
the same `I:` HDD.

Against the final-v3 empty profile, cold safe-shell p50/p95 changed
-2.23%/-9.14%, native-ready -52.67%/-46.28%, and first-Batch +5.12%/+4.08%.
Safe shell and first Batch stay inside the 10% materiality criterion. The
simulated 256 GB declaration
therefore does not materially delay the cold safe shell or first usable Batch
workspace, and it writes no synthetic media.

## Retained v1 diagnostic and recovery-path repair

The first current-source matrices in `runs\current-empty-authoritative` and
`runs\current-interrupted-authoritative`, together with
`runs\current-empty-bracket-after`, were valid diagnostics but failed the
materiality gate: interrupted first-Batch p50/p95 was +14.10%/+16.79% versus
the mean of the bracketing empty runs. They are retained and not presented as
passing final evidence.

The repair reuses one database connection for interrupted-row classification,
skips protected secure-store lookup for deliberately unscoped rows, and
pre-scans legacy UUID media before opening SQLite. The final-v3 matrices above
are the authoritative post-repair result.

## Local-browser 10,000-row interaction evidence

Evidence:

- JSON:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\interaction-browser-final-v3\browser-interactions.json`
  — 19,117 B — SHA-256
  `A9553213B8FA512262F7C9E35E20AB7EDAD00F9EB464E1C22C0742509383EAC8`.
- Screenshot:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\interaction-browser-final-v3\browser-interactions.png`
  — 112,338 B — SHA-256
  `3AA8463CE37188ECC03A6B8996C8DAEF54CF4074797F0A99F9FA47D99EAFD948`.

Windows Edge 151 at 1416×1108 mounted the real Batch queue with 10,000
synthetic in-memory rows. After five warm-up pairs, 40 actual search inputs
measured 33/67/74 ms p50/p95/max and 40 actual clear-button clicks measured
33/67/67 ms. Every sample mounted at most 32 rows; maximum interaction Long
Task was 0 ms; runtime errors were zero. All gates passed.

This is local browser/React interaction evidence only. It did not open SQLite,
secure storage, a real operator profile, media, packaged WebView2, or a provider
connection.

## Durable media-copy decision

The healthy-headroom `I:` HDD evidence rejected both sequential-scan copy
candidates. The 8 MiB sequential candidate regressed 3.70% at p50 and 18.61%
at p95 against its paired 1 MiB standard-open reference; the 1 MiB sequential
candidate regressed 3.79% and 10.35%. Production therefore uses the fastest
tested safe non-regressing option: 1 MiB heap buffer with standard opens.

All copies completed the final `sync_all`, copied 67,108,864 bytes, and matched
the retained BLAKE3 digest. The interruption case closed, resumed, durably
synced, reopened at full length, and matched the source digest. Scheduler,
cancellation, partial-length, resume, and durability boundaries remain intact.

- Primary report:
  `I:\YouTube-Upload-Manager-TASK110-durable-copy-20260823\REPORT.md`
  — SHA-256
  `4331EF3590A2AE8F8285B9D1D4CBCFBF74E32ED6BA85BDBD210059C152FFC1E6`.
- Candidate-isolation follow-up:
  `I:\YouTube-Upload-Manager-TASK110-durable-copy-20260823\hdd-1mib-sequential-followup\FOLLOWUP-REPORT.md`
  — SHA-256
  `BF60C1C2C280E364998A8D82EE9575A313BE37B430052C9A4072F4618C4FC3A1`.

The package pins x64 FFprobe SHA-256
`3A7E2DC003DC2CD1472827E4C7C4F056AE1AE0AE7C5BBC580C99B49827351BA4`
(82,668,032 B) and license SHA-256
`8CEB4B9EE5ADEDDE47B31E975C1D90C73AD27B6B165A1DCD80C7C545EB65B903`
(35,147 B).

## Standard unsigned Windows production artifact and historical smoke

Retained artifacts:

- Executable:
  `I:\YouTube-Upload-Manager-Certification-20260823\artifacts\production-unsigned-frozen-v3\youtube-upload-manager.exe`
  — 20,057,088 B — SHA-256
  `50D06CCA95824CF94974D57854E97E196E0220AC2737CD09829EDF340B983FB4`.
- NSIS installer:
  `I:\YouTube-Upload-Manager-Certification-20260823\artifacts\production-unsigned-frozen-v3\YouTube Upload Manager_0.1.9_x64-setup.exe`
  — 26,686,021 B — SHA-256
  `5137ED03E448B9F0263C4A929E490DCF8AAABB0BA51CF169D49C34DB5513293F`.
- Portable ZIP:
  `I:\YouTube-Upload-Manager-Certification-20260823\artifacts\production-unsigned-frozen-v3\YouTube-Upload-Manager_0.1.9_x64-portable-unsigned.zip`
  — 37,601,064 B — SHA-256
  `DC49C1BE9BC53A3146878BA1954C8C6A88A03C90473D05A5BB171188E75A10DB`.
- Manifest:
  `I:\YouTube-Upload-Manager-Certification-20260823\artifacts\production-unsigned-frozen-v3\production-build-manifest.json`.
  — 1,941 B — SHA-256
  `D01D32999E9F551F5C86C5088E6B7B0396672B38D64C6F3C268F9AF519FFED7F`.

The executable and installer are Authenticode `NotSigned`; the executable is
x64 PE machine `0x8664`. ZIP readback contains
exactly `youtube-upload-manager.exe`, `ffprobe.exe`, and
`ffprobe-license.txt`; extracted hashes and sizes match the retained production
executable and pinned sidecar/license above. The performance runner rejects the
ordinary executable as a non-harness build, proving the production artifact
does not expose the instrumented harness contract.

The final-v3 ordinary executable was **not launched**. Its artifact integrity
is certified, but its UI/runtime is not. A historical exact-path smoke of the
earlier ordinary executable (SHA-256
`84F39ED1F7827C43BAB5DD2481D503F321D0A1A8A0BB9D816A955BBE7C2C4AA9`)
rendered a real 81-item Batch profile as `1–32 of 81` and exited cleanly; that
receipt remains useful historical behavior evidence but is not promoted to the
final-v3 production artifact.

## Final certification state

- Frozen local source/build: passed — TypeScript; 16 frontend files/87 tests;
  140 native tests, zero failed, five ignored release-only benchmarks; 6/6
  FFprobe tests; payload and deterministic query/worker gates.
- Frozen local browser: passed for the 10,000-row Batch interaction slice only.
- Frozen unsigned instrumented Windows package: empty and interrupted-profile
  startup, recovery, integrity, materiality, and settled idle passed.
- Frozen standard unsigned Windows production artifact integrity: passed.
  Final-v3 executable, NSIS, portable ZIP, x64 PE, sidecar/license readback,
  Authenticode status, and harness isolation were verified.
- Frozen standard unsigned Windows production launch: not exercised. The prior
  exact-path 81-item smoke belongs to an earlier artifact.
- Signed Windows production: unavailable; no local code-signing certificate.
- macOS, Linux, Android, and iOS packaged runtime: unavailable on this host.
- Live Google OAuth/YouTube: not exercised; no approved non-production canary
  account/client was supplied.
- Equivalent healthy-headroom NVMe comparison: unavailable. No 50% startup or
  NVMe copy-speed claim is made from incomparable samples.

## Comparison boundary and remaining receipts

The frozen historical startup baseline used a different first-Batch marker,
non-equivalent WebView profile semantics, only ten samples, and the low/full
system NVMe. The final harness uses an actual native-acknowledged safe-shell
milestone, isolated WebView profiles, two reversed 40-run blocks, and the
healthy-headroom `I:` HDD. Comparing those distributions as though they were
equivalent would be misleading; this report makes no 50% startup claim.

An equivalent healthy-headroom NVMe run also remains unavailable because the
only local NVMe volume did not meet the 10%-free-space rule. Earlier near-full
NVMe copy samples are diagnostic only.

TASK102 and TASK112 are complete for the available local and unsigned Windows
scope. Their completion does not assert the unavailable healthy-NVMe 50%
comparison, signing, non-Windows packaged runtimes, or live-provider behavior.
Those evidence levels require external storage headroom, certificates,
platforms/devices, and an explicitly approved non-production Google/YouTube
canary respectively.
