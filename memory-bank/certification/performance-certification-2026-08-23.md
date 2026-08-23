# Performance certification — 2026-08-23

## Outcome

The app-wide performance implementation and the available local/unsigned
Windows certification are **complete**. TASK103 through TASK112 pass their
available source, browser, local-native, unsigned instrumented-package, and
standard unsigned-package acceptance slices. Signed production, non-Windows
packages, live Google/YouTube, and an equivalent healthy-headroom NVMe
comparison were unavailable and remain explicitly unverified external evidence
boundaries.

No benchmark artifact contains credentials, OAuth responses, channel IDs,
source paths, filenames, media contents, resumable-session URIs, or provider
payloads.

## Evidence-level matrix

| Level | Status | What is established |
| --- | --- | --- |
| Local source/build | passed | 75/75 frontend tests; 127 native tests, 0 failed, 5 ignored release-only benchmarks; 6/6 FFprobe tests; deterministic bundle/query/worker gates |
| Local browser | passed, scoped | Real Batch `QueueTable`, 10,000 synthetic rows, 40 search and 40 clear interactions; not SQLite/WebView2/provider proof |
| Unsigned instrumented Windows package | passed | Empty and interrupted-256 GB Tauri/WebView2 startup, safe-shell receipt, Batch paint, SQLite integrity/cardinality, settled idle, storage provenance |
| Standard unsigned Windows production | passed | Exact artifacts hash verified; x64 PE and portable contents verified; exact-path desktop process rendered the real 81-item Batch profile with 32-row pagination |
| Signed Windows production | unavailable | No local code-signing certificate |
| macOS/Linux/Android/iOS package | unavailable | Required host/toolchains/devices were not available |
| Live Google/YouTube | not exercised | No approved non-production account/OAuth canary was supplied |

## Final local correctness and deterministic gates

- Frontend: `npm run check`, production build, 75/75 tests, and payload gate
  passed.
- Initial frontend payload: 230,478 B JavaScript raw, 71,657 B JavaScript gzip,
  and 38,470 B CSS raw. Budgets are 240,640 B, 71,680 B, and 40,960 B.
- Native: formatting/checks passed; the full `performance-harness` suite passed
  127 tests with zero failures and five intentionally ignored release-only
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
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\empty-v6-authoritative\output\packaged-windows-empty.json`
  — 810,423 B — SHA-256
  `7C21E5DE0BEA98ED9B583B7A994208C5209E903CE6927D08CF0A11E91F98F90E`.
- Markdown:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\empty-v6-authoritative\output\packaged-windows-empty.md`
  — 2,216 B — SHA-256
  `ADB3DBEF42DFAC62510900FBA93A9455A9B36DF99EA502CBEA07F661B6672702`.
- Measured executable: 20,034,048 B — SHA-256
  `6CEC897CFE1D83FE7B4F73452974E7ACD7668C4158BF234086DCB7CE4163CB2D`.
- Harness NSIS installer: 26,685,288 B — SHA-256
  `04D7E400EAB5146756AA40697A90DEE3C07F99CB56933F105310A3DAC4BB1D59`.
- Build manifest: SHA-256
  `C62F15891230473841C563E4CC2D57B3BBE5B213188E70EF53E5D3F1E6FF734D`.

Method: two reversed blocks of 40 launches per mode, five untimed warmups,
80 measured cold and 80 measured warm runs, nearest-rank percentiles, raw
chronological receipts, no removed outliers, isolated WebView2 data per clone,
and a two-second settled-idle window. Storage was the healthy-headroom NTFS
`I:` SATA HDD (`WDC WD5000LPVX-22V0TT0`), with 363,612,291,072 B free before
and 363,613,335,552 B after; both exceed the max(20 GiB, 10%) rule.

| Empty-profile metric | Cold p50 | Cold p95 | Warm p50 | Warm p95 |
| --- | ---: | ---: | ---: | ---: |
| Safe-shell paint, acknowledged by native | 2,970 ms | 3,286 ms | 158 ms | 406 ms |
| Native ready | 1,865 ms | 2,321 ms | 503 ms | 743 ms |
| First Batch paint | 4,103 ms | 4,378 ms | 967 ms | 1,198 ms |
| Idle CPU during two seconds | 0 ms | 16 ms | 0 ms | 31 ms |
| Idle private bytes | 6,582,272 | 7,208,960 | 6,819,840 | 7,487,488 |
| Idle working-set bytes | 32,505,856 | 33,062,912 | 32,690,176 | 32,899,072 |
| Native invokes | 7 | 7 | 7 | 7 |
| Database opens | 4 | 4 | 4 | 4 |
| SQLite statements | 109 | 109 | 23 | 23 |

Every measured run recorded zero settled-idle periodic invokes, database opens,
SQLite statements, event messages, worker threads, and FFprobe processes. All
SQLite `quick_check` and fixture cardinality receipts passed. One warm run
observed a 60 ms startup-window Long Task; the p95 was zero, and the dedicated
interaction fixture below recorded no interaction Long Task.

The executable was Authenticode `NotSigned` and used the isolated
`performance-harness` profile. It is packaged runtime evidence, not a standard
production install or signed release.

## Unsigned packaged Windows — authoritative interrupted 256 GB profile

Evidence:

- JSON:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\interrupted-v4-authoritative\output\packaged-windows-interrupted-256gb.json`
  — 813,009 B — SHA-256
  `947088598093BB283A20AEB7C9E9DFE851F7FB9533007D68034EC1739AAAF5B1`.
- Markdown:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\interrupted-v4-authoritative\output\packaged-windows-interrupted-256gb.md`
  — 2,261 B — SHA-256
  `CC44686F901A8DC8E18DA9985C2BC6F651FCC6D3BC49B65BF2D4080F1A6D4279`.
- The executable, installer, build manifest, source, lockfile, FFprobe, and
  license hashes exactly match the authoritative empty-profile package.

The same two-block method measured 80 cold and 80 warm launches with no removed
outliers. The closed SQLite template contained one pre-existing `uploading` row
declaring 256,000,000,000 bytes, zero source/media bytes, no channel or secure
binding, and passed `quick_check`. Each measured clone began with that exact
cardinality and ended with one `needs_reconciliation` item, zero `uploading`
items, two restart-reconciliation audit receipts, the same declared byte count,
zero sensitive bindings, and `quick_check: ok`.

| Interrupted-profile metric | Cold p50 | Cold p95 | Warm p50 | Warm p95 |
| --- | ---: | ---: | ---: | ---: |
| Safe-shell paint, acknowledged by native | 2,899 ms | 3,449 ms | 305 ms | 720 ms |
| Native ready | 787 ms | 1,019 ms | 558 ms | 1,021 ms |
| First Batch paint | 4,370 ms | 4,727 ms | 1,550 ms | 2,035 ms |
| Idle CPU during two seconds | 0 ms | 16 ms | 0 ms | 16 ms |
| Native invokes | 7 | 7 | 7 | 7 |
| Database opens | 7 | 7 | 7 | 7 |
| SQLite statements | 62 | 62 | 62 | 62 |

All 160 safe-shell values were present. Every run recorded zero settled-idle
periodic invokes, database opens, SQLite statements, event messages, worker
threads, and FFprobe processes. Storage headroom passed before and after on the
same `I:` HDD.

Against the authoritative empty profile, cold safe-shell p50 changed -2.39%
and p95 +4.96%; cold first-Batch p50 changed +6.51% and p95 +7.97%. Both stay
inside the 10% materiality criterion. The simulated 256 GB declaration
therefore does not materially delay the cold safe shell or first usable Batch
workspace, and it writes no synthetic media.

## Local-browser 10,000-row interaction evidence

Evidence:

- JSON:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\interaction-browser-v4\browser-interactions.json`
  — 19,117 B — SHA-256
  `D108AB1207285E04C7FD046A4DE25321357D44245CCA7E1806D194A81917236F`.
- Screenshot:
  `I:\YouTube-Upload-Manager-Certification-20260823\runs\interaction-browser-v4\browser-interactions.png`
  — 111,438 B — SHA-256
  `35822003D9DC703FDCC024818BC91678E9B17C158C9AE5A4D0612C48666ED3DF`.

Windows Edge 151 at 1416×1108 mounted the real Batch queue with 10,000
synthetic in-memory rows. After five warm-up pairs, 40 actual search inputs
measured 44/87/91 ms p50/p95/max and 40 actual clear-button clicks measured
42/85/96 ms. Every sample mounted at most 32 rows; maximum interaction Long
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

## Standard unsigned Windows production package and smoke

Retained artifacts:

- Executable:
  `I:\YouTube-Upload-Manager-Certification-20260823\artifacts\production-unsigned\youtube-upload-manager.exe`
  — 19,988,480 B — SHA-256
  `84F39ED1F7827C43BAB5DD2481D503F321D0A1A8A0BB9D816A955BBE7C2C4AA9`.
- NSIS installer:
  `I:\YouTube-Upload-Manager-Certification-20260823\artifacts\production-unsigned\YouTube Upload Manager_0.1.9_x64-setup.exe`
  — 26,669,567 B — SHA-256
  `4D40D8DDD5FD4A4A7A0213F0E81006D93FBF5CE8DA312DB4462E12071D89E5C8`.
- Portable ZIP:
  `I:\YouTube-Upload-Manager-Certification-20260823\artifacts\production-unsigned\YouTube-Upload-Manager_0.1.9_x64-portable-unsigned.zip`
  — 37,565,048 B — SHA-256
  `01D9C0F691844052F9D24700DCFFCD98C5F087028EB1A52400303E68D45B712E`.

The production executable is x64 PE machine `0x8664`. ZIP readback contains
exactly `youtube-upload-manager.exe`, `ffprobe.exe`, and
`ffprobe-license.txt`; extracted hashes and sizes match the retained production
executable and pinned sidecar/license above. The performance runner rejects the
ordinary executable as a non-harness build, proving the production artifact
does not expose the instrumented harness contract.

The exact retained production executable path and process ID were verified at
desktop smoke. It rendered the active Batch workspace from a real local
81-item profile and paginated it as `1–32 of 81`, directly confirming the
32-row mounted bound in the standard Tauri runtime. No destructive, provider,
or other interaction was performed. The exact process exited, and UI automation
stopped at the operator's request.

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
