[CmdletBinding()]
param(
    [ValidateSet('performance', 'release')][string] $Profile = 'release',
    [ValidateRange(1, 64)][int] $Jobs = 4,
    [string] $CompilerDirectory,
    [string] $ReportDirectory
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'The native browser build currently requires Windows.' }
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$target = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) }
    else { Join-Path $repo 'target' }
if ([IO.Path]::GetPathRoot($target) -ne 'G:\') {
    throw 'Set CARGO_TARGET_DIR to an absolute G: directory for local build outputs.'
}
if (-not $ReportDirectory) {
    $ReportDirectory = Join-Path $target ('build-reports/' + [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$ReportDirectory = [IO.Path]::GetFullPath($ReportDirectory)
if ([IO.Path]::GetPathRoot($ReportDirectory) -ne 'G:\') {
    throw 'Build reports must be stored on G:.'
}
[IO.Directory]::CreateDirectory($ReportDirectory) | Out-Null
$started = [DateTimeOffset]::UtcNow
$clock = [Diagnostics.Stopwatch]::StartNew()
$stages = [Collections.Generic.List[object]]::new()
$prior = @{}
foreach ($name in @('CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'TEMP', 'TMP', 'LIBCLANG_PATH', 'CLANG_PATH', 'PATH')) {
    $prior[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
function Invoke-BuildStage([string] $Name, [scriptblock] $Action) {
    $stageClock = [Diagnostics.Stopwatch]::StartNew()
    $success = $false
    try { & $Action; $success = $true }
    finally {
        $stages.Add([ordered]@{ name = $Name; seconds = $stageClock.Elapsed.TotalSeconds; success = $success })
        Write-Host ('{0}: {1:N2}s ({2})' -f $Name, $stageClock.Elapsed.TotalSeconds, $success)
    }
}
$success = $false
$revision = $null
$workingTreeClean = $null
Push-Location -LiteralPath $repo
try {
    $env:CARGO_TARGET_DIR = $target
    $env:CARGO_BUILD_JOBS = [string]$Jobs
    $env:TEMP = Join-Path $target 'build-temp'
    $env:TMP = $env:TEMP
    [IO.Directory]::CreateDirectory($env:TEMP) | Out-Null
    $revision = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify the build revision.' }
    $workingTreeClean = @(& git status --porcelain).Count -eq 0
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify the build working-tree state.' }
    # Fail before Cargo/native compilation when the matching parser is missing.
    Invoke-BuildStage 'toolchain preflight' {
        & "$PSScriptRoot/prepare-angle.ps1" -CompilerDirectory $CompilerDirectory
        & "$PSScriptRoot/prepare-build-tools.ps1"
    }
    Invoke-BuildStage 'V8 preparation' { & "$PSScriptRoot/prepare-v8.ps1" -Profile $Profile }
    Invoke-BuildStage 'Cargo build' {
        & cargo build --profile $Profile --locked --bin better-web-browser --jobs $Jobs --timings
        if ($LASTEXITCODE -ne 0) { throw "Cargo build failed with exit code $LASTEXITCODE." }
    }
    $success = $true
    Write-Output (Join-Path $target "$Profile/better-web-browser.exe")
} finally {
    $timing = [ordered]@{
        revision = $revision; working_tree_clean = $workingTreeClean
        profile = $Profile; jobs = $Jobs; success = $success
        started = $started.ToString('o'); ended = [DateTimeOffset]::UtcNow.ToString('o')
        seconds = $clock.Elapsed.TotalSeconds; stages = $stages.ToArray()
    }
    $timing | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'build.json') -Encoding utf8
    $cargoTiming = Join-Path $target 'cargo-timings/cargo-timing.html'
    if (Test-Path -LiteralPath $cargoTiming -PathType Leaf) {
        # Do not attach a stale report when preflight failed before Cargo ran.
        if ((Get-Item -LiteralPath $cargoTiming).LastWriteTimeUtc -ge $started.UtcDateTime) {
            Copy-Item -LiteralPath $cargoTiming -Destination (Join-Path $ReportDirectory 'cargo-timing.html')
        }
    }
    foreach ($name in $prior.Keys) {
        if ($null -eq $prior[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $prior[$name], 'Process') }
    }
    Pop-Location
    Write-Host ('Total build wall time: {0:N2}s; report: {1}' -f $clock.Elapsed.TotalSeconds, $ReportDirectory)
}
