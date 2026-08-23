[CmdletBinding()]
param(
    [Parameter()]
    [string]$ExecutablePath,

    [Parameter()]
    [string]$DisposableProfileDirectory,

    [Parameter()]
    [string]$OutputDirectory = (Join-Path $PWD 'output\performance\packaged-windows'),

    [Parameter()]
    [ValidateRange(40, 200)]
    [int]$Iterations = 40,

    [Parameter()]
    [ValidateRange(5, 20)]
    [int]$WarmupIterations = 5,

    [Parameter()]
    [ValidateRange(0, 60)]
    [int]$IdleSeconds = 2,

    [Parameter()]
    [ValidateRange(5, 120)]
    [int]$StartupTimeoutSeconds = 30,

    [Parameter()]
    [ValidateSet('empty', 'interrupted-256gb')]
    [string]$Fixture = 'empty',

    [Parameter()]
    [string]$BuildManifestPath,

    [Parameter()]
    [switch]$ValidateOnly,

    [Parameter()]
    [Alias('h')]
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$profileEnvironmentName = 'YUM_PERFORMANCE_PROFILE_DIR'
$fixtureEnvironmentName = 'YUM_PERFORMANCE_FIXTURE'
$fixtureSeedOnlyEnvironmentName = 'YUM_PERFORMANCE_FIXTURE_SEED_ONLY'
$webView2ProfileEnvironmentName = 'WEBVIEW2_USER_DATA_FOLDER'
$profileMarkerName = '.youtube-upload-manager-performance-profile'
$fixtureMetadataName = 'performance-fixture.json'
$appIdentifier = 'com.sekailens.youtube-upload-manager'
$harnessBinarySignature = 'com.sekailens.youtube-upload-manager.performance-harness'
$idleMinimumMilliseconds = 1900
$idleMaximumMilliseconds = 2200
$minimumFreeBytes = [int64](20GB)
$measurementBlockCount = 2
$cleanupTimeoutMilliseconds = 15000
$cleanupInitialDelayMilliseconds = 50
$cleanupMaximumDelayMilliseconds = 1000

function Show-Usage {
    @'
Usage:
  pwsh -File scripts/performance/measure-packaged-windows.ps1 `
    -ExecutablePath <performance-harness exe> `
    -DisposableProfileDirectory <existing empty directory> `
    [-OutputDirectory <directory>] [-Iterations 40] [-WarmupIterations 5]
    [-IdleSeconds 2] [-StartupTimeoutSeconds 30] `
    [-Fixture empty|interrupted-256gb] `
    [-BuildManifestPath <windows-harness-build-manifest.json>] [-ValidateOnly]

Safety:
  The executable must be built with Cargo feature performance-harness. The
  disposable profile is mandatory, must already exist, must be empty, and must
  not be the real application profile. The runner verifies the harness-only
  secure-store signature in the binary and refuses an ordinary production app.
  Interrupted fixture setup is untimed, synthetic, metadata-only, and cloned
  from a closed template for every measured launch. Certification runs two
  complete blocks in cold-first then warm-first order.
'@ | Write-Output
}

function Resolve-ExistingPath([string]$Path, [string]$Label, [switch]$Directory) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw "$Label is required."
    }
    $item = Get-Item -LiteralPath $Path -ErrorAction Stop
    if ($Directory -and -not $item.PSIsContainer) {
        throw "$Label must be a directory."
    }
    if (-not $Directory -and $item.PSIsContainer) {
        throw "$Label must be a file."
    }
    return $item.FullName
}

function Assert-DisposableProfile([string]$Path) {
    $resolved = Resolve-ExistingPath $Path 'DisposableProfileDirectory' -Directory
    $root = [System.IO.Path]::GetPathRoot($resolved)
    if ($resolved.TrimEnd('\') -eq $root.TrimEnd('\')) {
        throw 'DisposableProfileDirectory cannot be a filesystem root.'
    }

    $realProfiles = @(
        (Join-Path $env:APPDATA $appIdentifier),
        (Join-Path $env:LOCALAPPDATA $appIdentifier)
    ) | ForEach-Object { [System.IO.Path]::GetFullPath($_).TrimEnd('\') }
    $candidate = [System.IO.Path]::GetFullPath($resolved).TrimEnd('\')
    foreach ($realProfile in $realProfiles) {
        if ($candidate.Equals($realProfile, [System.StringComparison]::OrdinalIgnoreCase) -or
            $candidate.StartsWith("$realProfile\", [System.StringComparison]::OrdinalIgnoreCase) -or
            $realProfile.StartsWith("$candidate\", [System.StringComparison]::OrdinalIgnoreCase)) {
            throw 'DisposableProfileDirectory overlaps the real application profile.'
        }
    }

    if (Get-ChildItem -LiteralPath $resolved -Force | Select-Object -First 1) {
        throw 'DisposableProfileDirectory must be empty before a benchmark run.'
    }
    return $resolved
}

function Assert-PerformanceHarnessBinary([string]$Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $contents = [System.Text.Encoding]::UTF8.GetString($bytes)
    if (-not $contents.Contains($harnessBinarySignature, [System.StringComparison]::Ordinal)) {
        throw 'ExecutablePath is not a performance-harness build; refusing to launch it.'
    }
}

function Get-FileReceipt([string]$Path, [string]$Kind) {
    $item = Get-Item -LiteralPath $Path -ErrorAction Stop
    [ordered]@{
        kind = $Kind
        name = $item.Name
        bytes = [int64]$item.Length
        sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash
    }
}

function Get-DirectoryReceipt([string]$Path) {
    $resolved = [System.IO.Path]::GetFullPath($Path)
    $records = @()
    $totalBytes = [int64]0
    foreach ($file in @(Get-ChildItem -LiteralPath $resolved -File -Recurse -Force | Sort-Object FullName)) {
        $relative = [System.IO.Path]::GetRelativePath($resolved, $file.FullName).Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        $totalBytes += [int64]$file.Length
        $records += "$relative|$($file.Length)|$hash"
    }
    $payload = [System.Text.Encoding]::UTF8.GetBytes(($records -join "`n"))
    [ordered]@{
        fileCount = $records.Count
        totalBytes = $totalBytes
        sha256 = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($payload))
    }
}

function Get-PythonInvocation {
    $python = Get-Command python -ErrorAction SilentlyContinue
    if ($null -ne $python) {
        return [ordered]@{ command = $python.Source; prefix = @() }
    }
    $launcher = Get-Command py -ErrorAction SilentlyContinue
    if ($null -ne $launcher) {
        return [ordered]@{ command = $launcher.Source; prefix = @('-3') }
    }
    throw 'Python 3 is required for read-only SQLite quick_check and cardinality receipts.'
}

function Get-SqliteReceipt([string]$Profile) {
    $databasePath = Join-Path $Profile 'queue.sqlite3'
    if (-not (Test-Path -LiteralPath $databasePath -PathType Leaf)) {
        return [ordered]@{
            databasePresent = $false
            quickCheck = 'not-applicable-fresh-profile'
            schemaVersion = $null
            uploadItems = 0
            uploadingItems = 0
            needsReconciliationItems = 0
            declaredTotalBytes = 0
            auditEvents = 0
            restartReconciliationEvents = 0
            sensitiveBindingCount = 0
        }
    }

    $probe = Get-PythonInvocation
    $source = @'
import json, sqlite3, sys
from pathlib import Path
p = Path(sys.argv[1]).resolve()
db = sqlite3.connect(p.as_uri() + '?mode=ro', uri=True)
tables = {row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
def scalar(sql):
    return int(db.execute(sql).fetchone()[0])
quick = ','.join(str(row[0]) for row in db.execute('PRAGMA quick_check'))
receipt = {
    'databasePresent': True,
    'quickCheck': quick,
    'schemaVersion': scalar('PRAGMA user_version'),
    'uploadItems': 0,
    'uploadingItems': 0,
    'needsReconciliationItems': 0,
    'declaredTotalBytes': 0,
    'auditEvents': 0,
    'restartReconciliationEvents': 0,
    'sensitiveBindingCount': 0,
}
if 'upload_items' in tables:
    receipt['uploadItems'] = scalar('SELECT COUNT(*) FROM upload_items')
    receipt['uploadingItems'] = scalar("SELECT COUNT(*) FROM upload_items WHERE status = 'uploading'")
    receipt['needsReconciliationItems'] = scalar("SELECT COUNT(*) FROM upload_items WHERE status = 'needs_reconciliation'")
    receipt['declaredTotalBytes'] = scalar('SELECT COALESCE(SUM(total_bytes), 0) FROM upload_items')
    receipt['sensitiveBindingCount'] = scalar("SELECT COUNT(*) FROM upload_items WHERE COALESCE(channel_id, '') <> '' OR COALESCE(channel_name, '') <> '' OR COALESCE(file_name, '') <> '' OR COALESCE(workspace_path, '') <> '' OR COALESCE(source_path, '') <> '' OR resumable_session_uri IS NOT NULL OR video_id IS NOT NULL")
if 'audit_events' in tables:
    receipt['auditEvents'] = scalar('SELECT COUNT(*) FROM audit_events')
    receipt['restartReconciliationEvents'] = scalar("SELECT COUNT(*) FROM audit_events WHERE kind = 'restart_reconciliation'")
print(json.dumps(receipt, separators=(',', ':')))
'@
    $arguments = @($probe.prefix) + @('-c', $source, $databasePath)
    $output = & $probe.command @arguments
    if ($LASTEXITCODE -ne 0) {
        throw 'The read-only SQLite profile receipt failed.'
    }
    return $output | ConvertFrom-Json
}

function Assert-SqliteReceipt([object]$Receipt, [string]$ExpectedState) {
    if ($Receipt.databasePresent -and $Receipt.quickCheck -ne 'ok') {
        throw "SQLite quick_check failed for $ExpectedState."
    }
    if ([int64]$Receipt.sensitiveBindingCount -ne 0) {
        throw "Synthetic profile $ExpectedState contains a channel, path, session, or provider binding."
    }
    switch ($ExpectedState) {
        'fresh-empty' {
            if ($Receipt.databasePresent -or [int64]$Receipt.uploadItems -ne 0) {
                throw 'The fresh empty template is not marker-only.'
            }
        }
        'initialized-empty' {
            if (-not $Receipt.databasePresent -or [int64]$Receipt.uploadItems -ne 0) {
                throw 'The initialized empty profile has unexpected upload rows.'
            }
        }
        'interrupted-before' {
            if (-not $Receipt.databasePresent -or [int64]$Receipt.uploadItems -ne 1 -or
                [int64]$Receipt.uploadingItems -ne 1 -or
                [int64]$Receipt.declaredTotalBytes -ne 256000000000) {
                throw 'The interrupted template does not contain exactly the declared synthetic upload.'
            }
        }
        'interrupted-after' {
            if (-not $Receipt.databasePresent -or [int64]$Receipt.uploadItems -ne 1 -or
                [int64]$Receipt.uploadingItems -ne 0 -or
                [int64]$Receipt.needsReconciliationItems -ne 1 -or
                [int64]$Receipt.restartReconciliationEvents -lt 1) {
                throw 'The interrupted launch did not leave one durable reconciliation receipt.'
            }
        }
        default { throw 'Unknown SQLite receipt expectation.' }
    }
}

function Get-WebView2Version {
    $roots = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Microsoft\EdgeWebView\Application'),
        (Join-Path $env:ProgramFiles 'Microsoft\EdgeWebView\Application'),
        (Join-Path $env:LOCALAPPDATA 'Microsoft\EdgeWebView\Application')
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and (Test-Path -LiteralPath $_ -PathType Container) }
    $versions = foreach ($root in $roots) {
        Get-ChildItem -LiteralPath $root -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^\d+(\.\d+){3}$' } |
            ForEach-Object { [version]$_.Name }
    }
    if (@($versions).Count -eq 0) { return 'unavailable' }
    return (@($versions) | Sort-Object -Descending | Select-Object -First 1).ToString()
}

function Get-SystemProvenance {
    $computer = Get-CimInstance Win32_ComputerSystem
    $processor = Get-CimInstance Win32_Processor | Select-Object -First 1
    $powerPlan = (& powercfg /GETACTIVESCHEME 2>$null | Out-String).Trim()
    $acStatus = 'unavailable'
    try {
        Add-Type -AssemblyName System.Windows.Forms -ErrorAction Stop
        $acStatus = [System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus.ToString()
    }
    catch {
        if ($null -eq (Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue)) {
            $acStatus = 'not-applicable-no-battery'
        }
    }
    [ordered]@{
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
        operatingSystem = [System.Environment]::OSVersion.VersionString
        architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        processor = $processor.Name.Trim()
        logicalProcessors = [System.Environment]::ProcessorCount
        totalPhysicalMemoryBytes = [int64]$computer.TotalPhysicalMemory
        webView2Version = Get-WebView2Version
        activePowerPlan = $powerPlan
        acLineStatus = $acStatus
    }
}

function Get-StorageProvenance([hashtable]$LabeledPaths, [string]$Stage) {
    $byDrive = @{}
    foreach ($entry in $LabeledPaths.GetEnumerator()) {
        $fullPath = [System.IO.Path]::GetFullPath([string]$entry.Value)
        $root = [System.IO.Path]::GetPathRoot($fullPath)
        if ($root -notmatch '^[A-Za-z]:\\$') {
            throw "The $($entry.Key) benchmark path must use a local Windows volume."
        }
        $driveLetter = $root.Substring(0, 1).ToUpperInvariant()
        if (-not $byDrive.ContainsKey($driveLetter)) {
            $byDrive[$driveLetter] = @()
        }
        $byDrive[$driveLetter] += [string]$entry.Key
    }

    $receipts = @()
    foreach ($driveLetter in @($byDrive.Keys | Sort-Object)) {
        $volume = Get-Volume -DriveLetter $driveLetter -ErrorAction Stop |
            Where-Object { $null -ne $_.Size -and $_.Size -gt 0 } |
            Select-Object -First 1
        $partition = Get-Partition -DriveLetter $driveLetter -ErrorAction Stop |
            Select-Object -First 1
        $disk = $partition | Get-Disk -ErrorAction Stop | Select-Object -First 1
        if ($null -eq $volume -or $null -eq $partition -or $null -eq $disk) {
            throw "Storage provenance is unavailable for benchmark volume $driveLetter`: ."
        }
        $diskNumber = Get-OptionalProperty $disk 'Number'
        $diskModel = Get-OptionalProperty $disk 'FriendlyName'
        $diskBusType = Get-OptionalProperty $disk 'BusType'
        $diskMediaType = Get-OptionalProperty $disk 'MediaType'
        $physicalDisk = $null
        try {
            $physicalDisks = @(Get-PhysicalDisk -ErrorAction Stop)
            $physicalDisk = $physicalDisks | Where-Object {
                $candidateDeviceId = Get-OptionalProperty $_ 'DeviceId'
                $candidateName = Get-OptionalProperty $_ 'FriendlyName'
                ($null -ne $diskNumber -and [string]$candidateDeviceId -eq [string]$diskNumber) -or
                ($null -ne $diskModel -and [string]$candidateName -eq [string]$diskModel)
            } | Select-Object -First 1
            if ($null -eq $physicalDisk -and $physicalDisks.Count -eq 1) {
                $physicalDisk = $physicalDisks[0]
            }
        }
        catch {
            # Get-Disk remains authoritative when Get-PhysicalDisk is absent.
        }
        if ($null -ne $physicalDisk) {
            if ($null -eq $diskModel) { $diskModel = Get-OptionalProperty $physicalDisk 'FriendlyName' }
            if ($null -eq $diskModel) { $diskModel = Get-OptionalProperty $physicalDisk 'Model' }
            if ($null -eq $diskBusType) { $diskBusType = Get-OptionalProperty $physicalDisk 'BusType' }
            if ($null -eq $diskMediaType) { $diskMediaType = Get-OptionalProperty $physicalDisk 'MediaType' }
        }
        $requiredFreeBytes = [int64][Math]::Max($minimumFreeBytes, [double]$volume.Size * 0.10)
        $receipts += [ordered]@{
            stage = $Stage
            drive = "$driveLetter`:"
            labels = @($byDrive[$driveLetter] | Sort-Object)
            fileSystem = $volume.FileSystem
            volumeSizeBytes = [int64]$volume.Size
            freeBytes = [int64]$volume.SizeRemaining
            requiredFreeBytes = $requiredFreeBytes
            diskNumber = if ($null -eq $diskNumber) { $null } else { [int]$diskNumber }
            model = if ([string]::IsNullOrWhiteSpace([string]$diskModel)) { 'unknown' } else { [string]$diskModel }
            busType = if ([string]::IsNullOrWhiteSpace([string]$diskBusType)) { 'unknown' } else { [string]$diskBusType }
            mediaType = if ([string]::IsNullOrWhiteSpace([string]$diskMediaType)) { 'unknown' } else { [string]$diskMediaType }
        }
    }
    return $receipts
}

function Assert-StorageHeadroom([object[]]$Receipts) {
    foreach ($receipt in $Receipts) {
        if ([int64]$receipt.freeBytes -lt [int64]$receipt.requiredFreeBytes) {
            throw "Benchmark volume $($receipt.drive) has $($receipt.freeBytes) free bytes; at least $($receipt.requiredFreeBytes) are required."
        }
    }
}

function Get-RepositoryReceipt([string]$RepositoryRoot) {
    $head = (& git -C $RepositoryRoot rev-parse HEAD 2>$null | Select-Object -First 1)
    $status = (& git -C $RepositoryRoot status --short --untracked-files=all 2>$null | Out-String)
    $statusBytes = [System.Text.Encoding]::UTF8.GetBytes($status)
    [ordered]@{
        commit = if ($LASTEXITCODE -eq 0) { $head.Trim() } else { 'unavailable' }
        workingTreeClean = [string]::IsNullOrWhiteSpace($status)
        workingTreeStatusSha256 = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($statusBytes))
    }
}

function Read-And-VerifyBuildManifest(
    [string]$Path,
    [string]$Executable,
    [string]$RepositoryRoot
) {
    $resolved = Resolve-ExistingPath $Path 'BuildManifestPath'
    $manifest = Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json
    if ($manifest.localOnly -ne $true -or
        $manifest.containsSensitiveData -ne $false -or
        $manifest.evidenceBoundary -ne 'unsigned-performance-harness-build' -or
        $manifest.performanceHarness -ne $true -or
        $manifest.signed -ne $false -or
        $manifest.liveProvider -ne $false) {
        throw 'The build manifest does not describe an unsigned local performance harness.'
    }
    $executableHash = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash
    if (-not @($manifest.artifacts | Where-Object { $_.sha256 -eq $executableHash -and $_.kind -eq 'package-executable' }).Count) {
        throw 'The measured executable hash is absent from the harness build manifest.'
    }
    foreach ($kind in @('package-executable', 'package-installer', 'runner', 'builder', 'lockfile', 'ffprobe', 'license')) {
        if (-not @($manifest.artifacts | Where-Object { $_.kind -eq $kind }).Count) {
            throw "The build manifest omits required $kind provenance."
        }
    }
    foreach ($record in @($manifest.artifacts | Where-Object { $_.kind -in @('runner', 'builder', 'lockfile', 'ffprobe', 'license') })) {
        $currentPath = Join-Path $RepositoryRoot $record.relativePath
        if (-not (Test-Path -LiteralPath $currentPath -PathType Leaf) -or
            (Get-FileHash -LiteralPath $currentPath -Algorithm SHA256).Hash -ne $record.sha256) {
            throw "Build provenance changed after packaging: $($record.relativePath)."
        }
    }
    return [ordered]@{
        manifest = $manifest
        receipt = Get-FileReceipt $resolved 'build-manifest'
    }
}

function Get-Percentile([double[]]$Values, [double]$Percentile) {
    if ($Values.Count -eq 0) { return $null }
    $ordered = @($Values | Sort-Object)
    $index = [Math]::Max(0, [Math]::Ceiling($Percentile * $ordered.Count) - 1)
    return [Math]::Round([double]$ordered[$index], 2)
}

function Get-Distribution([double[]]$Values) {
    if ($Values.Count -eq 0) {
        return [ordered]@{ p50 = $null; p90 = $null; p95 = $null; minimum = $null; maximum = $null }
    }
    [ordered]@{
        p50 = Get-Percentile $Values 0.50
        p90 = Get-Percentile $Values 0.90
        p95 = Get-Percentile $Values 0.95
        minimum = [Math]::Round(($Values | Measure-Object -Minimum).Minimum, 2)
        maximum = [Math]::Round(($Values | Measure-Object -Maximum).Maximum, 2)
    }
}

function Get-Summary([object[]]$Runs) {
    [ordered]@{
        count = $Runs.Count
        percentileMethod = 'nearest-rank'
        outliersRemoved = 0
        startupWindowMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.startupWindowMs }))
        idlePrivateBytes = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.idlePrivateBytes }))
        idleWorkingSetBytes = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.idleWorkingSetBytes }))
        idleCpuMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.idleCpuMs }))
        safeShellPaintMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { if ($null -ne $_.native.safeShellPaintMs) { $_.native.safeShellPaintMs } }))
        nativeReadyMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.nativeReadyMs }))
        firstBatchPaintMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.firstBatchPaintMs }))
        firstInteractionResponseMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { if ($null -ne $_.native.firstInteractionResponseMs) { $_.native.firstInteractionResponseMs } }))
        firstInteractionLatencyMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { if ($null -ne $_.native.firstInteractionLatencyMs) { $_.native.firstInteractionLatencyMs } }))
        nativeInvokes = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.nativeInvokes }))
        databaseOpens = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.databaseOpens }))
        databaseStatements = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.databaseStatements }))
        settledIdlePeriodicInvokes = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.settledIdlePeriodicInvokes }))
        settledIdleDatabaseOpens = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.settledIdleDatabaseOpens }))
        settledIdleDatabaseStatements = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.settledIdleDatabaseStatements }))
        settledIdleEventMessages = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.settledIdleEventMessages }))
        settledIdleDurationMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.idleSampleDurationMs }))
        settledIdleWorkerThreads = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.settledIdleWorkerThreads }))
        settledIdleFfprobeProcesses = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.settledIdleFfprobeProcesses }))
        longTasks = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.longTasks }))
        maxLongTaskMs = Get-Distribution ([double[]]@($Runs | ForEach-Object { $_.native.maxLongTaskMs }))
    }
}

function Get-FixtureMetadata([string]$Name) {
    switch ($Name) {
        'empty' {
            return [ordered]@{
                id = 'empty'
                initialState = 'empty'
                uploadItems = 0
                interruptedUploads = 0
                declaredTotalBytes = 0
                mediaBytesWritten = 0
                templatePreseeded = $false
                templateVerified = $true
                clonedPerLaunch = $true
            }
        }
        'interrupted-256gb' {
            return [ordered]@{
                id = 'interrupted-256gb'
                initialState = 'pre-existing-interrupted-upload'
                uploadItems = 1
                interruptedUploads = 1
                declaredTotalBytes = [int64]256000000000
                mediaBytesWritten = 0
                templatePreseeded = $true
                templateVerified = $false
                clonedPerLaunch = $true
            }
        }
        default { throw 'Unsupported synthetic performance fixture.' }
    }
}

function New-RunProfile([string]$Root, [string]$Name) {
    $profile = Join-Path $Root $Name
    New-Item -ItemType Directory -Path $profile -ErrorAction Stop | Out-Null
    Set-Content -LiteralPath (Join-Path $profile $profileMarkerName) `
        -Value 'local synthetic performance fixtures only' -Encoding utf8NoBOM
    return $profile
}

function Assert-GeneratedProfile([string]$Root, [string]$Path) {
    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\')
    $resolvedPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
    if ($resolvedPath.Equals($resolvedRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not $resolvedPath.StartsWith("$resolvedRoot\", [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Generated benchmark profile escaped the disposable root.'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $resolvedPath $profileMarkerName) -PathType Leaf)) {
        throw 'Generated benchmark profile lacks the required isolation marker.'
    }
    return $resolvedPath
}

function Get-ExactIsolatedWebView2ProcessIds([string[]]$ExactWebViewPaths) {
    $webViewPaths = @(
        $ExactWebViewPaths |
            ForEach-Object { [System.IO.Path]::GetFullPath($_).TrimEnd('\') } |
            Select-Object -Unique
    )
    try {
        return @(
            Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" -ErrorAction Stop |
                Where-Object {
                    $commandLine = Get-OptionalProperty $_ 'CommandLine'
                    if ($null -eq $commandLine) { return $false }
                    foreach ($webViewPath in $webViewPaths) {
                        if (([string]$commandLine).IndexOf(
                            $webViewPath,
                            [System.StringComparison]::OrdinalIgnoreCase
                        ) -ge 0) {
                            return $true
                        }
                    }
                    return $false
                } |
                ForEach-Object { Get-OptionalProperty $_ 'ProcessId' } |
                Where-Object { $null -ne $_ }
        )
    }
    catch {
        return @()
    }
}

function Get-IsolatedWebView2ProcessIds([string]$SafePath) {
    return @(Get-ExactIsolatedWebView2ProcessIds @(
        (Join-Path $SafePath 'webview2'),
        (Join-Path $SafePath 'webview2-seed')
    ))
}

function Invoke-SafeProfileRemovalAttempt([string]$SafePath) {
    $markerPath = Join-Path $SafePath $profileMarkerName
    foreach ($item in @(Get-ChildItem -LiteralPath $SafePath -Force -ErrorAction Stop)) {
        if ($item.FullName.Equals($markerPath, [System.StringComparison]::OrdinalIgnoreCase)) {
            continue
        }
        Remove-Item -LiteralPath $item.FullName -Recurse -Force -ErrorAction Stop
    }
    Remove-Item -LiteralPath $markerPath -Force -ErrorAction Stop
    try {
        Remove-Item -LiteralPath $SafePath -Force -ErrorAction Stop
    }
    catch {
        if (Test-Path -LiteralPath $SafePath -PathType Container) {
            Set-Content -LiteralPath $markerPath `
                -Value 'local synthetic performance fixtures only' -Encoding utf8NoBOM
        }
        throw
    }
}

function Remove-SafeRunPath(
    [string]$Root,
    [string]$Path,
    [int]$TimeoutMilliseconds = $cleanupTimeoutMilliseconds,
    [int]$InitialDelayMilliseconds = $cleanupInitialDelayMilliseconds,
    [int]$MaximumDelayMilliseconds = $cleanupMaximumDelayMilliseconds,
    [scriptblock]$RemoveAttempt
) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return }
    if ($TimeoutMilliseconds -lt 1 -or $InitialDelayMilliseconds -lt 1 -or
        $MaximumDelayMilliseconds -lt $InitialDelayMilliseconds) {
        throw 'Safe cleanup retry settings are invalid.'
    }
    $safePath = Assert-GeneratedProfile $Root $Path
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    $attempts = 0
    $delayMilliseconds = $InitialDelayMilliseconds
    $lastError = $null
    $matchingWebViewProcessIds = @()
    while ($true) {
        $attempts += 1
        try {
            if ($null -eq $RemoveAttempt) {
                Invoke-SafeProfileRemovalAttempt $safePath
            }
            else {
                & $RemoveAttempt $safePath
            }
            if (Test-Path -LiteralPath $safePath) {
                throw 'The exact generated profile still exists after its cleanup attempt.'
            }
            return
        }
        catch {
            $lastError = $_.Exception.Message
            $matchingWebViewProcessIds = @(Get-IsolatedWebView2ProcessIds $safePath)
            if ($timer.ElapsedMilliseconds -ge $TimeoutMilliseconds) {
                $processReceipt = if ($matchingWebViewProcessIds.Count -eq 0) {
                    'none observed'
                }
                else {
                    ($matchingWebViewProcessIds -join ',')
                }
                throw "Timed out after $attempts attempts removing the exact marker-validated benchmark clone. The remaining clone is preserved for inspection. Matching isolated WebView2 process IDs: $processReceipt. Last error: $lastError"
            }
            $remainingMilliseconds = $TimeoutMilliseconds - [int]$timer.ElapsedMilliseconds
            Start-Sleep -Milliseconds ([Math]::Min($delayMilliseconds, $remainingMilliseconds))
            $delayMilliseconds = [Math]::Min(
                $MaximumDelayMilliseconds,
                [Math]::Max($delayMilliseconds + 1, $delayMilliseconds * 2)
            )
        }
    }
}

function Remove-GeneratedProfile([string]$Root, [string]$Path) {
    Remove-SafeRunPath $Root $Path
}

function Invoke-BoundedProfileOperation(
    [string]$Root,
    [string]$SourceProfile,
    [string]$DestinationProfile,
    [string]$OperationLabel,
    [string[]]$ObservedWebViewPaths,
    [scriptblock]$Operation,
    [int]$TimeoutMilliseconds = $cleanupTimeoutMilliseconds,
    [int]$InitialDelayMilliseconds = $cleanupInitialDelayMilliseconds,
    [int]$MaximumDelayMilliseconds = $cleanupMaximumDelayMilliseconds
) {
    if ($TimeoutMilliseconds -lt 1 -or $InitialDelayMilliseconds -lt 1 -or
        $MaximumDelayMilliseconds -lt $InitialDelayMilliseconds) {
        throw 'Bounded profile-operation retry settings are invalid.'
    }
    $safeSource = Assert-GeneratedProfile $Root $SourceProfile
    $safeDestination = Assert-GeneratedProfile $Root $DestinationProfile
    $allowedPrefixes = @(
        "$($safeSource.TrimEnd('\'))\",
        "$($safeDestination.TrimEnd('\'))\"
    ) | Select-Object -Unique
    $safeObservedPaths = foreach ($observedPath in $ObservedWebViewPaths) {
        $resolvedObservedPath = [System.IO.Path]::GetFullPath($observedPath)
        $contained = $false
        foreach ($prefix in $allowedPrefixes) {
            if ($resolvedObservedPath.StartsWith(
                $prefix,
                [System.StringComparison]::OrdinalIgnoreCase
            )) {
                $contained = $true
                break
            }
        }
        if (-not $contained) {
            throw 'A bounded profile operation attempted to observe a path outside its marker-validated profiles.'
        }
        $resolvedObservedPath
    }

    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    $attempts = 0
    $delayMilliseconds = $InitialDelayMilliseconds
    $lastError = $null
    $matchingWebViewProcessIds = @()
    while ($true) {
        $attempts += 1
        try {
            & $Operation $safeSource $safeDestination
            return
        }
        catch {
            $lastError = $_.Exception.Message
            $matchingWebViewProcessIds = @(
                Get-ExactIsolatedWebView2ProcessIds $safeObservedPaths
            )
            if ($timer.ElapsedMilliseconds -ge $TimeoutMilliseconds) {
                $processReceipt = if ($matchingWebViewProcessIds.Count -eq 0) {
                    'none observed'
                }
                else {
                    ($matchingWebViewProcessIds -join ',')
                }
                throw "Timed out after $attempts attempts during $OperationLabel for exact marker-validated profiles. Source and destination are preserved for inspection. Matching isolated WebView2 process IDs: $processReceipt. Last error: $lastError"
            }
            $remainingMilliseconds = $TimeoutMilliseconds - [int]$timer.ElapsedMilliseconds
            Start-Sleep -Milliseconds ([Math]::Min($delayMilliseconds, $remainingMilliseconds))
            $delayMilliseconds = [Math]::Min(
                $MaximumDelayMilliseconds,
                [Math]::Max($delayMilliseconds + 1, $delayMilliseconds * 2)
            )
        }
    }
}

function Copy-FixtureTemplate([string]$Root, [string]$Template, [string]$Name) {
    Assert-GeneratedProfile $Root $Template | Out-Null
    $profile = Join-Path $Root $Name
    if (Test-Path -LiteralPath $profile) {
        throw 'A generated benchmark run profile already exists.'
    }
    New-Item -ItemType Directory -Path $profile -ErrorAction Stop | Out-Null
    Set-Content -LiteralPath (Join-Path $profile $profileMarkerName) `
        -Value 'local synthetic performance fixtures only' -Encoding utf8NoBOM
    Assert-GeneratedProfile $Root $profile | Out-Null
    $copyTemplate = {
        param($safeTemplate, $safeProfile)
        foreach ($item in Get-ChildItem -LiteralPath $safeTemplate -Force) {
            if ($item.Name -eq $profileMarkerName) { continue }
            Copy-Item -LiteralPath $item.FullName -Destination $safeProfile -Recurse -Force -ErrorAction Stop
        }
    }
    Invoke-BoundedProfileOperation `
        $Root $Template $profile 'template clone copy' `
        @(
            (Join-Path $Template 'webview2'),
            (Join-Path $Template 'webview2-seed'),
            (Join-Path $profile 'webview2'),
            (Join-Path $profile 'webview2-seed')
        ) $copyTemplate
    Assert-GeneratedProfile $Root $profile | Out-Null
    return $profile
}

function Promote-WarmedWebViewData([string]$Root, [string]$SourceProfile, [string]$Template) {
    Assert-GeneratedProfile $Root $SourceProfile | Out-Null
    Assert-GeneratedProfile $Root $Template | Out-Null
    $source = Join-Path $SourceProfile 'webview2'
    if (-not (Test-Path -LiteralPath $source -PathType Container)) {
        throw 'The warmup launch did not create its isolated WebView2 profile.'
    }
    $destination = Join-Path $Template 'webview2'
    $promoteWebView = {
        param($safeSourceProfile, $safeTemplate)
        $safeSource = Join-Path $safeSourceProfile 'webview2'
        $safeDestination = Join-Path $safeTemplate 'webview2'
        if (Test-Path -LiteralPath $safeDestination) {
            Remove-Item -LiteralPath $safeDestination -Recurse -Force -ErrorAction Stop
        }
        Copy-Item -LiteralPath $safeSource -Destination $safeDestination -Recurse -Force -ErrorAction Stop
    }
    Invoke-BoundedProfileOperation `
        $Root $SourceProfile $Template 'warmed WebView2 promotion copy' `
        @($source, $destination) $promoteWebView
}

function Stop-BenchmarkProcess([System.Diagnostics.Process]$Process) {
    if (-not $Process.HasExited) {
        $Process.Kill($true)
        $Process.WaitForExit(5000) | Out-Null
    }
    $Process.Dispose()
}

function Get-OptionalProperty([object]$Object, [string]$Name) {
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Invoke-FixturePreseed(
    [string]$Executable,
    [string]$Root,
    [string]$Template,
    [string]$FixtureName
) {
    if ($FixtureName -eq 'empty') { return }

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Executable
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $false
    $startInfo.Environment[$profileEnvironmentName] = $Template
    $startInfo.Environment[$fixtureEnvironmentName] = $FixtureName
    $startInfo.Environment[$fixtureSeedOnlyEnvironmentName] = '1'
    $seedWebViewProfile = Join-Path $Template 'webview2-seed'
    $startInfo.Environment[$webView2ProfileEnvironmentName] = $seedWebViewProfile
    $process = [System.Diagnostics.Process]::Start($startInfo)
    if ($null -eq $process) { throw 'The untimed fixture-seed process did not start.' }

    try {
        $metadataPath = Join-Path $Template $fixtureMetadataName
        $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
        while ([DateTime]::UtcNow -lt $deadline) {
            if ($process.HasExited) {
                if ($process.ExitCode -ne 0) {
                    throw "The fixture-seed process failed (exit $($process.ExitCode))."
                }
                if (-not (Test-Path -LiteralPath $metadataPath -PathType Leaf)) {
                    throw 'The fixture-seed process exited without redacted fixture metadata.'
                }
                $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
                if ($metadata.localOnly -ne $true -or
                    $metadata.containsSensitiveData -ne $false -or
                    $metadata.fixture -ne 'interrupted-256gb' -or
                    [int64]$metadata.declaredTotalBytes -ne 256000000000 -or
                    [int64]$metadata.mediaBytesWritten -ne 0 -or
                    $metadata.persistedStatus -ne 'uploading') {
                    throw 'The untimed synthetic fixture metadata failed its redaction or size contract.'
                }
                return
            }
            Start-Sleep -Milliseconds 10
        }
        throw 'The performance harness did not finish untimed fixture seeding before the timeout.'
    }
    finally {
        Stop-BenchmarkProcess $process
        $removeSeedWebViewData = {
            param($safeTemplate, $unusedDestination)
            $safeSeedWebViewProfile = Join-Path $safeTemplate 'webview2-seed'
            if (Test-Path -LiteralPath $safeSeedWebViewProfile) {
                Remove-Item -LiteralPath $safeSeedWebViewProfile -Recurse -Force -ErrorAction Stop
            }
        }
        Invoke-BoundedProfileOperation `
            $Root $Template $Template 'fixture-seed WebView2 cleanup' `
            @($seedWebViewProfile) $removeSeedWebViewData
    }
}

function Invoke-StartupMeasurement(
    [string]$Executable,
    [string]$Profile,
    [string]$Mode,
    [int]$Ordinal,
    [bool]$Measured
) {
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Executable
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $false
    $startInfo.Environment[$profileEnvironmentName] = $Profile
    $startInfo.Environment[$webView2ProfileEnvironmentName] = Join-Path $Profile 'webview2'
    $startInfo.Environment.Remove($fixtureEnvironmentName) | Out-Null
    $startInfo.Environment.Remove($fixtureSeedOnlyEnvironmentName) | Out-Null

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $process = [System.Diagnostics.Process]::Start($startInfo)
    if ($null -eq $process) { throw 'The benchmark process did not start.' }

    try {
        $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
        $windowReady = $false
        while ([DateTime]::UtcNow -lt $deadline) {
            if ($process.HasExited) {
                throw "The performance-harness process exited before showing a window (exit $($process.ExitCode))."
            }
            $process.Refresh()
            if ($process.MainWindowHandle -ne [IntPtr]::Zero -and $process.Responding) {
                $windowReady = $true
                break
            }
            Start-Sleep -Milliseconds 10
        }
        if (-not $windowReady) {
            throw "No responsive application window appeared within $StartupTimeoutSeconds seconds."
        }
        $startupMs = [Math]::Round($stopwatch.Elapsed.TotalMilliseconds, 2)
        $nativeSnapshotPath = Join-Path $Profile 'native-performance-startup.json'
        $idleStartDeadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
        $idleStartSnapshot = $null
        while ([DateTime]::UtcNow -lt $idleStartDeadline) {
            if ($process.HasExited) {
                throw "The performance-harness process exited before settled idle (exit $($process.ExitCode))."
            }
            if (Test-Path -LiteralPath $nativeSnapshotPath -PathType Leaf) {
                try {
                    $candidate = Get-Content -LiteralPath $nativeSnapshotPath -Raw | ConvertFrom-Json
                    if ($null -ne $candidate.settledIdleMs) {
                        $idleStartSnapshot = $candidate
                        break
                    }
                }
                catch {
                    # A milestone may be atomically replacing the small JSON file.
                }
            }
            Start-Sleep -Milliseconds 10
        }
        if ($null -eq $idleStartSnapshot) {
            throw 'The performance-harness did not mark settled idle before the timeout.'
        }
        $process.Refresh()
        $cpuAtIdleStart = $process.TotalProcessorTime.TotalMilliseconds
        $idleEndDeadline = [DateTime]::UtcNow.AddSeconds([Math]::Max($StartupTimeoutSeconds, $IdleSeconds + 5))
        $nativeSnapshot = $null
        while ([DateTime]::UtcNow -lt $idleEndDeadline) {
            if ($process.HasExited) {
                throw "The performance-harness process exited during the idle sample (exit $($process.ExitCode))."
            }
            try {
                $candidate = Get-Content -LiteralPath $nativeSnapshotPath -Raw | ConvertFrom-Json
                if ($null -ne $candidate.idleSampleDurationMs -and
                    [double]$candidate.idleSampleDurationMs -ge $idleMinimumMilliseconds) {
                    $nativeSnapshot = $candidate
                    break
                }
            }
            catch {
                # Retry if the webview milestone is replacing the snapshot.
            }
            Start-Sleep -Milliseconds 10
        }
        if ($null -eq $nativeSnapshot) {
            throw 'The performance-harness did not complete its settled-idle sample.'
        }
        $process.Refresh()
        $idleCpuMs = [Math]::Round(
            [Math]::Max(0, $process.TotalProcessorTime.TotalMilliseconds - $cpuAtIdleStart),
            2
        )
        if ($nativeSnapshot.containsSensitiveData -ne $false -or $nativeSnapshot.localOnly -ne $true) {
            throw 'The native startup snapshot did not satisfy the redaction contract.'
        }
        foreach ($field in @(
            'settledIdlePeriodicInvokes',
            'settledIdleDatabaseOpens',
            'settledIdleDatabaseStatements',
            'settledIdleEventMessages',
            'settledIdleWorkerThreads',
            'settledIdleFfprobeProcesses'
        )) {
            if ($null -eq $nativeSnapshot.$field) {
                throw "The native startup snapshot omitted the settled-idle $field delta."
            }
            if ([int64]$nativeSnapshot.$field -ne 0) {
                throw "The settled-idle $field delta must be zero in every measured run."
            }
        }
        $idleDurationMilliseconds = [double]$nativeSnapshot.idleSampleDurationMs
        if ($idleDurationMilliseconds -lt $idleMinimumMilliseconds -or
            $idleDurationMilliseconds -gt $idleMaximumMilliseconds) {
            throw "The native startup snapshot did not stay within the explicit two-second settled-idle window ($idleMinimumMilliseconds-$idleMaximumMilliseconds ms)."
        }
        if ($null -eq $nativeSnapshot.nativeReadyMs -or $null -eq $nativeSnapshot.firstBatchPaintMs) {
            throw 'Every measured launch must include native-ready and real Batch-content receipts.'
        }
        return [ordered]@{
            mode = $Mode
            ordinal = $Ordinal
            measured = $Measured
            startupWindowMs = $startupMs
            idleSeconds = $IdleSeconds
            idleCpuMs = $idleCpuMs
            idlePrivateBytes = [int64]$process.PrivateMemorySize64
            idleWorkingSetBytes = [int64]$process.WorkingSet64
            native = [ordered]@{
                schemaVersion = Get-OptionalProperty $nativeSnapshot 'schemaVersion'
                appVersion = Get-OptionalProperty $nativeSnapshot 'appVersion'
                buildProfile = Get-OptionalProperty $nativeSnapshot 'buildProfile'
                initializationStartedMs = $nativeSnapshot.initializationStartedMs
                recoveryClassifiedMs = $nativeSnapshot.recoveryClassifiedMs
                safeShellPaintMs = Get-OptionalProperty $nativeSnapshot 'safeShellPaintMs'
                nativeReadyMs = $nativeSnapshot.nativeReadyMs
                firstBatchPaintMs = $nativeSnapshot.firstBatchPaintMs
                firstInteractionMs = $nativeSnapshot.firstInteractionMs
                firstInteractionResponseMs = Get-OptionalProperty $nativeSnapshot 'firstInteractionResponseMs'
                firstInteractionLatencyMs = Get-OptionalProperty $nativeSnapshot 'firstInteractionLatencyMs'
                firstInteractionKind = Get-OptionalProperty $nativeSnapshot 'firstInteractionKind'
                settledIdleMs = $nativeSnapshot.settledIdleMs
                idleSampleDurationMs = $nativeSnapshot.idleSampleDurationMs
                settledIdlePeriodicInvokes = $nativeSnapshot.settledIdlePeriodicInvokes
                settledIdleDatabaseOpens = $nativeSnapshot.settledIdleDatabaseOpens
                settledIdleDatabaseStatements = $nativeSnapshot.settledIdleDatabaseStatements
                settledIdleEventMessages = $nativeSnapshot.settledIdleEventMessages
                settledIdleWorkerThreads = $nativeSnapshot.settledIdleWorkerThreads
                settledIdleFfprobeProcesses = $nativeSnapshot.settledIdleFfprobeProcesses
                reactCommits = $nativeSnapshot.reactCommits
                longTasks = $nativeSnapshot.longTasks
                maxLongTaskMs = $nativeSnapshot.maxLongTaskMs
                nativeInvokes = $nativeSnapshot.nativeInvokes
                databaseOpens = $nativeSnapshot.databaseOpens
                databaseSchemaBatches = $nativeSnapshot.databaseSchemaBatches
                databaseStatements = $nativeSnapshot.databaseStatements
                workerThreads = $nativeSnapshot.workerThreads
                ffprobeProcesses = $nativeSnapshot.ffprobeProcesses
            }
        }
    }
    finally {
        Stop-BenchmarkProcess $process
    }
}

if ($Help) {
    Show-Usage
    exit 0
}
if (-not $IsWindows) {
    throw 'The packaged Windows benchmark runner can run only on Windows.'
}
if ($IdleSeconds -ne 2) {
    throw 'IdleSeconds must be exactly 2 so every report measures the explicit settled-idle window.'
}
if (-not $ValidateOnly -and $WarmupIterations -lt 5) {
    throw 'Certification requires at least five warmups before both measured blocks.'
}

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$resolvedExecutable = Resolve-ExistingPath $ExecutablePath 'ExecutablePath'
if ([System.IO.Path]::GetExtension($resolvedExecutable) -ne '.exe') {
    throw 'ExecutablePath must point to a Windows .exe file.'
}
Assert-PerformanceHarnessBinary $resolvedExecutable
$resolvedProfileRoot = Assert-DisposableProfile $DisposableProfileDirectory
$resolvedOutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
$profilePrefix = "$($resolvedProfileRoot.TrimEnd('\'))\"
$outputPrefix = "$($resolvedOutputDirectory.TrimEnd('\'))\"
if ($resolvedOutputDirectory.Equals($resolvedProfileRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
    $resolvedOutputDirectory.StartsWith($profilePrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
    $resolvedProfileRoot.StartsWith($outputPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'OutputDirectory and DisposableProfileDirectory must not overlap.'
}
$fixtureMetadata = Get-FixtureMetadata $Fixture

if ($ValidateOnly) {
    [ordered]@{
        valid = $true
        executableBytes = (Get-Item -LiteralPath $resolvedExecutable).Length
        iterations = $Iterations
        warmupIterations = $WarmupIterations
        measurementBlocks = $measurementBlockCount
        measuredRunsPerMode = $Iterations * $measurementBlockCount
        idleSeconds = $IdleSeconds
        idleDurationRangeMs = @($idleMinimumMilliseconds, $idleMaximumMilliseconds)
        webView2Profile = 'isolated-inside-each-cloned-profile'
        percentileMethod = 'nearest-rank'
        outliersRemoved = 0
        profileIsDisposableAndEmpty = $true
        fixture = $fixtureMetadata
        templateCleanupRequired = $true
        measuredFixtureEnvironmentRemoved = $true
        settledIdleDeltaIncludes = @(
            'periodicInvokes',
            'databaseOpens',
            'databaseStatements',
            'eventMessages',
            'workerThreads',
            'ffprobeProcesses'
        )
    } | ConvertTo-Json -Depth 4
    exit 0
}

if ([string]::IsNullOrWhiteSpace($BuildManifestPath)) {
    $BuildManifestPath = Join-Path $repositoryRoot 'output\performance\windows-harness-build-manifest.json'
}
$buildProvenance = Read-And-VerifyBuildManifest $BuildManifestPath $resolvedExecutable $repositoryRoot
$signature = Get-AuthenticodeSignature -LiteralPath $resolvedExecutable
if ($signature.Status.ToString() -ne 'NotSigned') {
    throw 'The instrumented performance harness must remain unsigned and separate from signed production evidence.'
}
$storagePaths = @{
    executable = $resolvedExecutable
    profile = $resolvedProfileRoot
    output = $resolvedOutputDirectory
    repository = $repositoryRoot
    buildManifest = $BuildManifestPath
}
$packageOrdinal = 0
foreach ($artifact in @($buildProvenance.manifest.artifacts | Where-Object { $_.kind -like 'package-*' })) {
    $packageOrdinal += 1
    $storagePaths["package-$packageOrdinal"] = Join-Path $repositoryRoot $artifact.relativePath
}
$storageBefore = Get-StorageProvenance $storagePaths 'before'
Assert-StorageHeadroom $storageBefore
$systemBefore = Get-SystemProvenance

$coldTemplate = New-RunProfile $resolvedProfileRoot 'cold-template'
$warmTemplate = $null
$coldRuns = @()
$warmRuns = @()
$chronologicalRuns = @()
$blockReports = @()
$sequence = 0
$coldTemplateReceipt = $null
$warmTemplateReceipt = $null
$coldTemplateSqlite = $null
$warmTemplateSqlite = $null
try {
    Invoke-FixturePreseed $resolvedExecutable $resolvedProfileRoot $coldTemplate $Fixture
    if ($Fixture -eq 'interrupted-256gb') {
        $warmTemplate = Copy-FixtureTemplate $resolvedProfileRoot $coldTemplate 'warm-template'
    }
    else {
        $warmTemplate = New-RunProfile $resolvedProfileRoot 'warm-template'
    }

    if ($Fixture -eq 'empty') {
        for ($iteration = 1; $iteration -le $WarmupIterations; $iteration += 1) {
            Remove-Item -LiteralPath (Join-Path $warmTemplate 'native-performance-startup.json') `
                -Force -ErrorAction SilentlyContinue
            Invoke-StartupMeasurement $resolvedExecutable $warmTemplate 'warmup' $iteration $false | Out-Null
        }
        Remove-Item -LiteralPath (Join-Path $warmTemplate 'native-performance-startup.json') `
            -Force -ErrorAction SilentlyContinue
    }
    else {
        for ($iteration = 1; $iteration -le $WarmupIterations; $iteration += 1) {
            $profile = Copy-FixtureTemplate $resolvedProfileRoot $warmTemplate ("warmup-{0:D3}" -f $iteration)
            try {
                Invoke-StartupMeasurement $resolvedExecutable $profile 'warmup' $iteration $false | Out-Null
                Promote-WarmedWebViewData $resolvedProfileRoot $profile $warmTemplate
            }
            finally {
                Remove-GeneratedProfile $resolvedProfileRoot $profile
            }
        }
    }
    Remove-Item -LiteralPath (Join-Path $warmTemplate 'native-performance-startup.json') `
        -Force -ErrorAction SilentlyContinue

    $coldTemplateSqlite = Get-SqliteReceipt $coldTemplate
    $warmTemplateSqlite = Get-SqliteReceipt $warmTemplate
    if ($Fixture -eq 'empty') {
        Assert-SqliteReceipt $coldTemplateSqlite 'fresh-empty'
        Assert-SqliteReceipt $warmTemplateSqlite 'initialized-empty'
    }
    else {
        Assert-SqliteReceipt $coldTemplateSqlite 'interrupted-before'
        Assert-SqliteReceipt $warmTemplateSqlite 'interrupted-before'
    }
    $coldTemplateReceipt = Get-DirectoryReceipt $coldTemplate
    $warmTemplateReceipt = Get-DirectoryReceipt $warmTemplate
    $fixtureMetadata['templateVerified'] = $true

    $blockSpecifications = @(
        [ordered]@{ id = 'block-1-cold-first'; order = @('cold', 'warm') },
        [ordered]@{ id = 'block-2-warm-first'; order = @('warm', 'cold') }
    )
    foreach ($block in $blockSpecifications) {
        $blockColdRuns = @()
        $blockWarmRuns = @()
        foreach ($mode in $block.order) {
            $template = if ($mode -eq 'cold') { $coldTemplate } else { $warmTemplate }
            $templateReceipt = if ($mode -eq 'cold') { $coldTemplateReceipt } else { $warmTemplateReceipt }
            $expectedBefore = if ($Fixture -eq 'empty' -and $mode -eq 'cold') {
                'fresh-empty'
            }
            elseif ($Fixture -eq 'empty') {
                'initialized-empty'
            }
            else {
                'interrupted-before'
            }
            for ($iteration = 1; $iteration -le $Iterations; $iteration += 1) {
                $profileName = "{0}-{1}-{2:D3}" -f $block.id, $mode, $iteration
                $profile = Copy-FixtureTemplate $resolvedProfileRoot $template $profileName
                try {
                    $profileReceipt = Get-DirectoryReceipt $profile
                    if ($profileReceipt.sha256 -ne $templateReceipt.sha256 -or
                        $profileReceipt.fileCount -ne $templateReceipt.fileCount -or
                        $profileReceipt.totalBytes -ne $templateReceipt.totalBytes) {
                        throw 'A measured profile clone does not match its closed template.'
                    }
                    $sqliteBefore = Get-SqliteReceipt $profile
                    Assert-SqliteReceipt $sqliteBefore $expectedBefore
                    $sequence += 1
                    $run = Invoke-StartupMeasurement $resolvedExecutable $profile $mode $iteration $true
                    $sqliteAfter = Get-SqliteReceipt $profile
                    if ($Fixture -eq 'empty') {
                        Assert-SqliteReceipt $sqliteAfter 'initialized-empty'
                    }
                    else {
                        Assert-SqliteReceipt $sqliteAfter 'interrupted-after'
                    }
                    if ([int64]$sqliteAfter.schemaVersion -ne [int64]$warmTemplateSqlite.schemaVersion) {
                        throw 'A measured profile did not reach the certified current schema.'
                    }
                    $run['sequence'] = $sequence
                    $run['block'] = $block.id
                    $run['blockOrder'] = @($block.order)
                    $run['profileBefore'] = $profileReceipt
                    $run['sqliteBefore'] = $sqliteBefore
                    $run['sqliteAfter'] = $sqliteAfter
                    $chronologicalRuns += $run
                    if ($mode -eq 'cold') {
                        $coldRuns += $run
                        $blockColdRuns += $run
                    }
                    else {
                        $warmRuns += $run
                        $blockWarmRuns += $run
                    }
                }
                finally {
                    Remove-GeneratedProfile $resolvedProfileRoot $profile
                }
            }
        }
        $blockReports += [ordered]@{
            id = $block.id
            order = @($block.order)
            cold = Get-Summary $blockColdRuns
            warm = Get-Summary $blockWarmRuns
        }
    }
}
finally {
    if ($null -ne $warmTemplate) {
        Remove-GeneratedProfile $resolvedProfileRoot $warmTemplate
    }
    Remove-GeneratedProfile $resolvedProfileRoot $coldTemplate
}

$storageAfter = Get-StorageProvenance $storagePaths 'after'
Assert-StorageHeadroom $storageAfter
$systemAfter = Get-SystemProvenance
$executableReceipt = Get-FileReceipt $resolvedExecutable 'measured-executable'
$report = [ordered]@{
    schemaVersion = 2
    localOnly = $true
    containsSensitiveData = $false
    percentileMethod = 'nearest-rank'
    outliersRemoved = 0
    evidenceBoundaries = [ordered]@{
        sourceAndReleaseTests = 'separate-local-unpackaged-evidence'
        instrumentedPackage = 'unsigned-performance-harness'
        unsignedProductionInstall = 'not-exercised'
        signedProduction = 'not-exercised'
        liveGoogleYouTube = 'not-exercised'
    }
    application = $executableReceipt
    provenance = [ordered]@{
        repository = Get-RepositoryReceipt $repositoryRoot
        buildManifest = $buildProvenance.receipt
        build = $buildProvenance.manifest
        authenticodeStatus = $signature.Status.ToString()
        signed = $false
        liveProvider = $false
    }
    environment = [ordered]@{
        before = $systemBefore
        after = $systemAfter
        storageBefore = $storageBefore
        storageAfter = $storageAfter
        minimumFreeSpaceRule = 'max(20 GiB, 10% of volume capacity), before and after'
    }
    fixture = [ordered]@{
        profile = 'synthetic-disposable-cloned-template'
        webView2Profile = 'isolated-inside-each-template-and-clone'
        id = $fixtureMetadata.id
        initialState = $fixtureMetadata.initialState
        uploadItems = $fixtureMetadata.uploadItems
        interruptedUploads = $fixtureMetadata.interruptedUploads
        declaredTotalBytes = $fixtureMetadata.declaredTotalBytes
        mediaBytesWritten = $fixtureMetadata.mediaBytesWritten
        templatePreseeded = $fixtureMetadata.templatePreseeded
        templateVerified = $fixtureMetadata.templateVerified
        clonedPerLaunch = $fixtureMetadata.clonedPerLaunch
        coldTemplateState = if ($Fixture -eq 'empty') { 'fresh-marker-only' } else { 'pre-existing-uploading' }
        warmTemplateState = if ($Fixture -eq 'empty') { 'closed-initialized-after-warmups' } else { 'pre-existing-uploading' }
        measuredFixtureEnvironmentRemoved = $true
        templates = [ordered]@{
            cold = [ordered]@{ profile = $coldTemplateReceipt; sqlite = $coldTemplateSqlite }
            warm = [ordered]@{ profile = $warmTemplateReceipt; sqlite = $warmTemplateSqlite }
        }
        iterations = $Iterations
        warmupIterations = $WarmupIterations
        measurementBlocks = $measurementBlockCount
        measuredRunsPerMode = $Iterations * $measurementBlockCount
        idleSeconds = $IdleSeconds
        idleDurationMinimumMs = $idleMinimumMilliseconds
        idleDurationMaximumMs = $idleMaximumMilliseconds
    }
    cold = [ordered]@{ summary = Get-Summary $coldRuns; runs = $coldRuns }
    warm = [ordered]@{ summary = Get-Summary $warmRuns; runs = $warmRuns }
    blocks = $blockReports
    chronologicalRuns = $chronologicalRuns
}

New-Item -ItemType Directory -Force -Path $resolvedOutputDirectory | Out-Null
$jsonPath = Join-Path $resolvedOutputDirectory "packaged-windows-$Fixture.json"
$markdownPath = Join-Path $resolvedOutputDirectory "packaged-windows-$Fixture.md"
$report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $jsonPath -Encoding utf8NoBOM

$cold = $report.cold.summary
$warm = $report.warm.summary
$markdown = @"
# Packaged Windows performance baseline

- Local-only: yes
- Sensitive data included: no
- Evidence level: unsigned performance harness; signed production and live Google/YouTube not exercised
- Application: $($report.application.name)
- Artifact bytes: $($report.application.bytes)
- Artifact SHA-256: $($report.application.sha256)
- Fixture: $($report.fixture.id) cloned synthetic template
- Fixture state: $($report.fixture.initialState)
- Declared upload bytes: $($report.fixture.declaredTotalBytes)
- Media bytes written: $($report.fixture.mediaBytesWritten)
- Template preseed verified: $($report.fixture.templateVerified)
- Cold template state: $($report.fixture.coldTemplateState)
- Warm template state: $($report.fixture.warmTemplateState)
- Measured fixture/seed environment removed: $($report.fixture.measuredFixtureEnvironmentRemoved)
- Measurement blocks: 2 (cold-first, then warm-first)
- Measured launches per mode and block: $Iterations
- Total measured launches per mode: $($Iterations * $measurementBlockCount)
- Warmup launches: $WarmupIterations
- Percentiles: nearest-rank; no outliers removed; raw chronological runs retained in JSON
- Settled-idle window: $idleMinimumMilliseconds-$idleMaximumMilliseconds ms; all six native deltas are zero in every run
- WebView2 data: isolated inside every template and clone
- Storage headroom: max(20 GiB, 10% of volume capacity) passed before and after

| Mode | Native ready p50 | p90 | p95 | max | Real Batch p50 | p90 | p95 | max | Interaction response p95 | Interaction latency p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Cold | $($cold.nativeReadyMs.p50) ms | $($cold.nativeReadyMs.p90) ms | $($cold.nativeReadyMs.p95) ms | $($cold.nativeReadyMs.maximum) ms | $($cold.firstBatchPaintMs.p50) ms | $($cold.firstBatchPaintMs.p90) ms | $($cold.firstBatchPaintMs.p95) ms | $($cold.firstBatchPaintMs.maximum) ms | $($cold.firstInteractionResponseMs.p95) ms | $($cold.firstInteractionLatencyMs.p95) ms |
| Warm | $($warm.nativeReadyMs.p50) ms | $($warm.nativeReadyMs.p90) ms | $($warm.nativeReadyMs.p95) ms | $($warm.nativeReadyMs.maximum) ms | $($warm.firstBatchPaintMs.p50) ms | $($warm.firstBatchPaintMs.p90) ms | $($warm.firstBatchPaintMs.p95) ms | $($warm.firstBatchPaintMs.maximum) ms | $($warm.firstInteractionResponseMs.p95) ms | $($warm.firstInteractionLatencyMs.p95) ms |

| Mode | Window p50 | Window p95 | Idle duration p50 | Idle invokes max | Idle DB opens max | Idle SQL max | Idle events max | Idle workers max | Idle probes max | RSS p50 | Private bytes p50 | Idle CPU p50 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Cold | $($cold.startupWindowMs.p50) ms | $($cold.startupWindowMs.p95) ms | $($cold.settledIdleDurationMs.p50) ms | $($cold.settledIdlePeriodicInvokes.maximum) | $($cold.settledIdleDatabaseOpens.maximum) | $($cold.settledIdleDatabaseStatements.maximum) | $($cold.settledIdleEventMessages.maximum) | $($cold.settledIdleWorkerThreads.maximum) | $($cold.settledIdleFfprobeProcesses.maximum) | $($cold.idleWorkingSetBytes.p50) | $($cold.idlePrivateBytes.p50) | $($cold.idleCpuMs.p50) ms |
| Warm | $($warm.startupWindowMs.p50) ms | $($warm.startupWindowMs.p95) ms | $($warm.settledIdleDurationMs.p50) ms | $($warm.settledIdlePeriodicInvokes.maximum) | $($warm.settledIdleDatabaseOpens.maximum) | $($warm.settledIdleDatabaseStatements.maximum) | $($warm.settledIdleEventMessages.maximum) | $($warm.settledIdleWorkerThreads.maximum) | $($warm.settledIdleFfprobeProcesses.maximum) | $($warm.idleWorkingSetBytes.p50) | $($warm.idlePrivateBytes.p50) | $($warm.idleCpuMs.p50) ms |

This report measures an isolated local Windows performance-harness build. It is
not unsigned-production install, signed-production, live Google/YouTube, or
another-platform evidence.
"@
$markdown | Set-Content -LiteralPath $markdownPath -Encoding utf8NoBOM

Write-Output "Packaged Windows baseline recorded: $jsonPath and $markdownPath"
