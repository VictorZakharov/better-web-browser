[CmdletBinding()]
param(
    [switch] $LibFuzzer,
    [ValidateRange(1, 3600)]
    [int] $Seconds = 10
)

$ErrorActionPreference = 'Stop'
$toolchain = 'nightly-2026-08-15'
$inputTimeoutSeconds = 5
$rssLimitMb = 1024
$targets = @(
    'html_document',
    'html_fragment',
    'css_stylesheet',
    'url_resolution',
    'dom_mutations',
    'javascript_host_bindings'
)

if (-not $LibFuzzer) {
    & cargo test --test hostile_input
    if ($LASTEXITCODE -ne 0) { throw 'Stable hostile-input corpus replay failed.' }
    return
}

if (-not $IsLinux) {
    throw 'Coverage-guided cargo-fuzz runs require Linux or WSL; use stable corpus replay on Windows.'
}

foreach ($target in $targets) {
    & cargo "+$toolchain" fuzz run $target "fuzz/corpus/$target" -- `
        "-max_total_time=$Seconds" "-timeout=$inputTimeoutSeconds" "-rss_limit_mb=$rssLimitMb"
    if ($LASTEXITCODE -ne 0) { throw "Fuzz target failed: $target" }
}
