[CmdletBinding()]
param(
    [switch] $LibFuzzer,
    [ValidateRange(1, 3600)]
    [int] $Seconds = 10,
    [ValidateSet(
        'html_document',
        'html_fragment',
        'css_stylesheet',
        'url_resolution',
        'dom_mutations',
        'javascript_host_bindings'
    )]
    [string[]] $Target,
    [switch] $Quiet,
    [string] $LogDirectory
)

$ErrorActionPreference = 'Stop'
$toolchain = 'nightly-2026-08-15'
$inputTimeoutSeconds = 5
$rssLimitMb = 1024
$allTargets = @(
    'html_document',
    'html_fragment',
    'css_stylesheet',
    'url_resolution',
    'dom_mutations',
    'javascript_host_bindings'
)
$targets = if ($Target) { $Target } else { $allTargets }

if (-not $LibFuzzer) {
    & cargo test --test hostile_input
    if ($LASTEXITCODE -ne 0) { throw 'Stable hostile-input corpus replay failed.' }
    return
}

if (-not $IsLinux) {
    throw 'Coverage-guided cargo-fuzz runs require Linux or WSL; use stable corpus replay on Windows.'
}

if ($Quiet) {
    if (-not $LogDirectory) {
        $LogDirectory = Join-Path ([System.IO.Path]::GetTempPath()) "better-web-browser-fuzz-$PID"
    }
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

foreach ($target in $targets) {
    $arguments = @(
        "+$toolchain",
        'fuzz',
        'run',
        $target,
        "fuzz/corpus/$target",
        '--',
        "-max_total_time=$Seconds",
        "-timeout=$inputTimeoutSeconds",
        "-rss_limit_mb=$rssLimitMb",
        '-print_final_stats=1'
    )

    if (-not $Quiet) {
        & cargo @arguments
        if ($LASTEXITCODE -ne 0) { throw "Fuzz target failed: $target" }
        continue
    }

    $logPath = Join-Path $LogDirectory "$target.log"
    Write-Host "Running $target for $Seconds seconds; full output: $logPath"
    & cargo @arguments *> $logPath
    $exitCode = $LASTEXITCODE

    if ($exitCode -ne 0) {
        Write-Host "Fuzz target failed; showing the final 200 log lines."
        Get-Content -Path $logPath -Tail 200
        throw "Fuzz target failed: $target (exit code $exitCode)"
    }

    $summary = Get-Content -Path $logPath | Where-Object {
        $_ -match '^INFO: Seed:' -or $_ -match '\sDONE\s' -or $_ -match '^stat::'
    }
    Write-Host "Fuzz target passed: $target"
    if ($summary) {
        $summary | Select-Object -Last 20 | ForEach-Object { Write-Host $_ }
    }
}
