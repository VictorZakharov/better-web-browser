[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Archive,
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')] [string] $Version,
    [Parameter(Mandatory)] [ValidatePattern('^[0-9a-fA-F]{40}$')] [string] $Commit
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$archivePath = (Resolve-Path -LiteralPath $Archive).Path
$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("breeze-release-smoke-" + [Guid]::NewGuid().ToString('N'))
$extractRoot = Join-Path $workRoot 'package'
$profile = Join-Path $workRoot 'profile'
$evidence = Join-Path $workRoot 'evidence'
$readyFile = Join-Path $workRoot 'fixture-server.txt'
$serverOutput = Join-Path $workRoot 'fixture-server.out'
$serverError = Join-Path $workRoot 'fixture-server.err'
$server = $null
$summary = $null

try {
    [IO.Directory]::CreateDirectory($extractRoot) | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
    try {
        foreach ($entry in $zip.Entries) {
            $destination = [IO.Path]::GetFullPath((Join-Path $extractRoot $entry.FullName))
            $prefix = $extractRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
            if (-not $destination.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
                throw "Archive entry escapes extraction root: $($entry.FullName)"
            }
            if ($entry.FullName -like '*.pdb' -or $entry.FullName -match '(?i)(?:^|/)(?:cargo|rustc|rustup)(?:\.exe)?(?:$|/)') {
                throw "Development/toolchain file was shipped: $($entry.FullName)"
            }
        }
    } finally { $zip.Dispose() }
    [IO.Compression.ZipFile]::ExtractToDirectory($archivePath, $extractRoot)

    $packageName = "breeze-v$Version-x86_64-pc-windows-msvc"
    $packageRoot = Join-Path $extractRoot $packageName
    $browser = Join-Path $packageRoot 'better-web-browser.exe'
    $manifestPath = Join-Path $packageRoot 'artifact-manifest.json'
    foreach ($required in @($browser, $manifestPath, (Join-Path $packageRoot 'LICENSE.txt'), (Join-Path $packageRoot 'THIRD_PARTY_NOTICES.md'), (Join-Path $packageRoot 'TECHNICAL_ALPHA.md'), (Join-Path $packageRoot 'accessibility.md'), (Join-Path $packageRoot 'VERSION.txt'))) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) { throw "Release archive is missing $required." }
    }
    $accessKitDirectories = @(Get-ChildItem -LiteralPath (Join-Path $packageRoot 'licenses') -Directory | Where-Object Name -like 'accesskit*')
    if ($accessKitDirectories.Count -eq 0) { throw 'Release archive has no AccessKit package notices.' }
    foreach ($directory in $accessKitDirectories) {
        if (-not (Test-Path -LiteralPath (Join-Path $directory.FullName 'LICENSE.chromium') -PathType Leaf)) {
            throw "Release archive is missing the Chromium notice for $($directory.Name)."
        }
    }

    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.package_version -ne $Version -or $manifest.commit -ne $Commit.ToLowerInvariant() -or $manifest.target -ne 'x86_64-pc-windows-msvc' -or $manifest.signed) {
        throw 'Artifact manifest version, commit, target, or signing status is invalid.'
    }
    $executableRecord = @($manifest.files | Where-Object path -eq 'better-web-browser.exe')
    $executableHash = (Get-FileHash -LiteralPath $browser -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($executableRecord.Count -ne 1 -or $executableRecord[0].sha256 -ne $executableHash) {
        throw 'Packaged executable does not match artifact-manifest.json.'
    }
    if (Test-Path -LiteralPath $profile) { throw 'Smoke profile unexpectedly exists before first launch.' }

    $server = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        (Join-Path $repoRoot 'scripts\serve-alpha-fixtures.ps1'), '-ReadyFile', $readyFile
    ) -WindowStyle Hidden -RedirectStandardOutput $serverOutput -RedirectStandardError $serverError -PassThru
    $deadline = (Get-Date).AddSeconds(10)
    while (-not (Test-Path -LiteralPath $readyFile)) {
        if ($server.HasExited) { throw 'Release-smoke fixture server exited during startup.' }
        if ((Get-Date) -gt $deadline) { throw 'Release-smoke fixture server did not become ready.' }
        Start-Sleep -Milliseconds 50
    }
    $url = (Get-Content -LiteralPath $readyFile -Raw).Trim().TrimEnd('/') + '/release-smoke.html'
    [IO.Directory]::CreateDirectory($evidence) | Out-Null
    $reportPath = Join-Path $evidence 'result.json'
    $capturePath = Join-Path $evidence 'capture.png'

    $originalPath = $env:PATH
    try {
        $env:PATH = (($env:PATH -split ';') | Where-Object { $_ -notmatch '(?i)\\\.cargo\\bin|\\rustup(?:\\|$)' }) -join ';'
        & (Join-Path $repoRoot 'scripts\run-hidden-benchmark.ps1') `
            -Url $url -Output $reportPath -Browser $browser -Screenshot $capturePath `
            -SettleMs 500 -TimeoutSeconds 45 -ProfileDirectory $profile `
            -DiagnosticSelector '#main','#profile-ready' | Write-Host
    } finally { $env:PATH = $originalPath }

    $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
    if ($null -ne $report.error -or [int] $report.http_status -ne 200 -or -not $report.headless -or -not $report.isolated_profile) {
        throw "Packaged navigation failed: status $($report.http_status), $($report.error)"
    }
    $main = @($report.diagnostics | Where-Object selector -eq '#main')
    $storage = @($report.diagnostics | Where-Object selector -eq '#profile-ready')
    if ($main.Count -ne 1 -or [int] $main[0].total_matches -ne 1 -or $storage.Count -ne 1 -or [int] $storage[0].total_matches -ne 1) {
        throw 'Packaged navigation did not produce the required major regions.'
    }
    foreach ($profileFile in @('cookies.json', 'local-storage.json')) {
        $path = Join-Path $profile $profileFile
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "First run did not create $profileFile." }
        Get-Content -LiteralPath $path -Raw | ConvertFrom-Json | Out-Null
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $browser
    $summary = [pscustomobject]@{
        version = $Version
        commit = $Commit.ToLowerInvariant()
        http_status = [int] $report.http_status
        retained_draw_items = [int] $report.retained_draw_items
        profile_files = @('cookies.json', 'local-storage.json')
        signature_status = [string] $signature.Status
        toolchain_paths_removed = $true
        portable_cleanup = $true
    }
} finally {
    if ($null -ne $server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
        $server.WaitForExit()
    }
    $fullWorkRoot = [IO.Path]::GetFullPath($workRoot)
    $expectedPrefix = Join-Path ([IO.Path]::GetTempPath()) 'breeze-release-smoke-'
    if (-not $fullWorkRoot.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to remove an unexpected release-smoke path.'
    }
    if (Test-Path -LiteralPath $fullWorkRoot) { Remove-Item -LiteralPath $fullWorkRoot -Recurse -Force }
}
if (Test-Path -LiteralPath $workRoot) { throw 'Portable release-smoke cleanup left files behind.' }
$summary
