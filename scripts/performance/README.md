# Packaged Windows performance runner

`measure-packaged-windows.ps1` measures local startup for a packaged Windows
build and writes redacted JSON and Markdown summaries. It is a baseline tool,
not telemetry: output contains timings and aggregate statistics only. Run it
from the repository checkout in PowerShell.

Build the isolated unsigned harness with `npm run performance:package:windows`.
That command activates both the native `performance-harness` feature and the
harness-only webview paint markers. An ordinary production build is rejected
by the runner. A successful build also writes
`output/performance/windows-harness-build-manifest.json` with exact source,
runner, lockfile, executable/installer, FFprobe, and license hashes. Python 3
is required for the runner's read-only SQLite integrity receipts.

## Safety requirement

`-DisposableProfileDirectory` is mandatory. It must name an existing,
explicitly empty directory that is safe to delete or reuse for this run (for
example, a newly created directory under a local `perf-runs` folder). The
runner refuses an omitted profile, the filesystem root, a non-empty directory,
or a path that looks like a real operator profile. This prevents a benchmark
from opening, changing, or deleting normal application data.

The runner sets the app's performance-harness profile override so the packaged
application uses that disposable profile. Do not point it at the normal
Tauri/AppData profile, a profile containing credentials, or a directory shared
with another run. Keep the executable and profile on the same machine for
comparable results.

## Example

```powershell
$profile = Join-Path $PWD 'perf-runs\empty-profile-01'
$output = Join-Path $PWD 'perf-runs\results-01'
New-Item -ItemType Directory -Force -Path $profile, $output | Out-Null

pwsh -File .\scripts\performance\measure-packaged-windows.ps1 `
  -ExecutablePath 'C:\path\to\YouTube Upload Manager.exe' `
  -DisposableProfileDirectory $profile `
  -OutputDirectory $output `
  -Iterations 40 `
  -WarmupIterations 5 `
  -IdleSeconds 2 `
  -Fixture empty
```

The profile must still be empty immediately before the command. Use a fresh
directory for each cold baseline. The output directory may be separate from
the profile; do not place output files inside the disposable profile.

## Arguments and run semantics

- `-ExecutablePath` — packaged Windows executable to launch.
- `-DisposableProfileDirectory` — required existing empty disposable profile;
  safety validation rejects omission, root, non-empty, and real-profile-risk
  paths.
- `-OutputDirectory` — directory for redacted `.json` and `.md` reports.
- `-Iterations` — launches per mode in each block; certification requires at
  least 40. Two blocks run cold-first and then warm-first.
- `-WarmupIterations` — unreported launches used to settle one-time Windows,
  WebView, and filesystem caches; certification requires at least five.
- `-IdleSeconds` — bounded idle period used before the next measurement.
- `-Fixture` — explicit `empty` or `interrupted-256gb` synthetic profile.
  The interrupted fixture contains one pre-existing `uploading` row whose
  declared size is 256,000,000,000 bytes and whose media footprint is zero.
- `-Help` — print validation and usage information without launching the app.

The runner creates a marker-protected template under the disposable root. For
`interrupted-256gb`, a harness-only seed process initializes the template and
exits before startup recovery; that untimed process writes no media and uses no
channel, credentials, secure-store entry, source path, or provider payload.
For `interrupted-256gb`, every cold, warmup, and warm launch receives a separate
clone of the closed preseeded template, so every measured launch begins with
the same `uploading` row and runs the real database-only classification. For
`empty`, cold launches clone a fresh marker-only template, while unmeasured
warmups initialize a separate persistent warm template; every measured warm
launch then clones that closed, current-schema template. This preserves the
reference distinction between a new empty profile and a warmed existing empty
profile without allowing measured-run mutations to leak into the next sample.
`WEBVIEW2_USER_DATA_FOLDER` points inside each generated profile. Fresh cold
clones therefore receive fresh WebView2 data, and measured warm clones receive
the same closed warmed WebView2 template instead of sharing the real app's
browser state.
The measured process environment explicitly removes both fixture seed
variables. Generated clones and templates are removed only after their marker
and containment beneath the disposable root are verified.

After five warmups, block 1 measures 40 cold then 40 warm clones. Block 2
reverses the order. Each block and the combined populations retain raw runs in
chronological order. Percentiles use nearest-rank p50/p90/p95, maximums stay
visible, and no outlier is removed. Do not combine cold and warm populations.

Before every launch, the runner computes path-free template and clone SHA-256
receipts and requires exact equality. A read-only Python SQLite probe records
`quick_check`, schema version, upload/status cardinalities, declared bytes,
reconciliation-audit cardinality, and absence of channel, path, secure-session,
or provider bindings. It repeats the logical receipt after the process closes.

The settled-idle interval is fixed at exactly two seconds. Each JSON snapshot
must contain deltas for periodic invokes, database opens and statements, event
messages, worker creation, and FFprobe processes. All six deltas must be zero
in every measured and warmup run. The runner rejects native idle duration
outside 1,900-2,200 milliseconds.

Markdown reports native-ready and real Batch-content p50/p90/p95/max. JSON also
consumes optional `safeShellPaintMs`, `firstInteractionResponseMs`,
`firstInteractionLatencyMs`, and `firstInteractionKind` receipts. Interaction
values remain `null` instead of substituting input time until a painted-response
receipt exists.

Each report includes aggregate p50 and p95 timings, iteration counts, run
mode/profile state, executable/version metadata when available, and fixture
context. It intentionally excludes tokens, OAuth responses, channel IDs,
source paths, filenames, media bytes, and provider payloads. Treat the JSON as
the machine-readable record and the Markdown as the reviewable handoff.

## Validation and limitations

Check the runner before a baseline:

```powershell
pwsh -File .\scripts\performance\measure-packaged-windows.ps1 -Help
```

This is a local packaged-startup measurement. It does not prove live Google or
YouTube latency, upload throughput, database query performance, browser-only
behavior, or performance on other operating systems. Timing is sensitive to
Windows updates, WebView2 state, antivirus, power mode, disk, and background
processes. Keep hardware, release artifact, profile state, iteration counts,
and idle interval constant; report variance and limitations with every
baseline.

## Certification provenance and headroom

The executable must match the unsigned package executable in the build
manifest. The runner re-hashes the manifest, runner, builder, lockfiles,
FFprobe binaries, and license, verifies that the harness is unsigned, and
records the source commit plus a hash of dirty-tree status.

Before and after measurement it records Windows, WebView2, CPU/RAM, power plan,
AC state, and the filesystem, physical disk model, bus/media type, capacity,
and free bytes for every volume containing the repository, executable, package,
profile, manifest, or output. It aborts unless each volume retains at least the
greater of 20 GiB or 10% of capacity before and after. Do not run builds, disk
cleanup, antivirus scans, or other benchmarks concurrently.

Evidence remains separated:

- source/release tests are local unpackaged evidence;
- this runner produces unsigned instrumented-harness evidence;
- an unsigned production installation is a separate smoke result;
- signed production requires separate Authenticode and installed-app proof;
- Google/YouTube requires an explicitly approved non-production live canary.

The report records signed production and live-provider evidence as
`not-exercised`; it never promotes harness timing into either level.

## Native scale fixtures

Run the release-only synthetic SQLite/media scale suite with:

```powershell
pwsh -File scripts/performance/measure-native-scale.ps1
```

Use `-NoBuild` only to replay the newest already-built release test executable.
The runner keeps redacted JSON, Markdown, and console evidence under
`output/performance/native-scale`, rejects failed or non-local records, and
never opens the operator profile.
