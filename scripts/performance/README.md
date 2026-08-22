# Packaged Windows performance runner

`measure-packaged-windows.ps1` measures local startup for a packaged Windows
build and writes redacted JSON and Markdown summaries. It is a baseline tool,
not telemetry: output contains timings and aggregate statistics only. Run it
from the repository checkout in PowerShell.

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
  -Iterations 10 `
  -WarmupIterations 2 `
  -IdleSeconds 2
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
- `-Iterations` — measured launches/runs after warmups.
- `-WarmupIterations` — unreported launches used to settle one-time Windows,
  WebView, and filesystem caches.
- `-IdleSeconds` — bounded idle period used before the next measurement.
- `-Help` — print validation and usage information without launching the app.

Cold results use a fresh disposable profile and fresh process launch for each
measurement, so they represent first-run startup work. Warm results use the
same benchmark setup after warmups and idle intervals, so they expose cached
startup behavior. Record whether the profile was empty or pre-seeded when
comparing runs; do not compare cold and warm p50/p95 as one population.

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
