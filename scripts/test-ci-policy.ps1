[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$gate = Join-Path $PSScriptRoot 'assert-ci-results.ps1'
$selector = Join-Path $PSScriptRoot 'select-ci-alpha-fixtures.ps1'
$names = @('source', 'lint', 'test', 'wpt', 'alpha', 'dependencies', 'harness')
$tests = 0
function Check-Gate {
    param([hashtable] $Arguments, [bool] $Pass)
    $failed = $false
    try { & $gate @Arguments | Out-Null } catch { $failed = $true }
    if ($failed -eq $Pass) { throw "Unexpected gate result for $($Arguments | ConvertTo-Json -Compress)." }
    $script:tests++
}
foreach ($markdown in @($false, $true)) {
    $workers = @{}
    foreach ($name in $names) { $workers[$name] = $(if ($markdown) { 'skipped' } else { 'success' }) }
    $arguments = @{
        ClassificationResult = 'success'
        RunWindows = (-not $markdown).ToString().ToLowerInvariant()
        MarkdownOnly = $markdown.ToString().ToLowerInvariant()
        Workers = $workers
    }
    Check-Gate $arguments $true
    foreach ($name in $names) {
        foreach ($result in @('failure', 'cancelled', 'pending', '', $(if ($markdown) { 'success' } else { 'skipped' }))) {
            $changed = $workers.Clone(); $changed[$name] = $result
            $case = $arguments.Clone(); $case.Workers = $changed
            Check-Gate $case $false
        }
    }
    $case = $arguments.Clone(); $case.ClassificationResult = 'failure'; Check-Gate $case $false
    $case = $arguments.Clone(); $case.RunWindows = ''; Check-Gate $case $false
    $case = $arguments.Clone(); $case.MarkdownOnly = $case.RunWindows; Check-Gate $case $false
    $case = $arguments.Clone(); $case.Workers = $workers.Clone(); $case.Workers.Remove('harness'); Check-Gate $case $false
    $case = $arguments.Clone(); $case.Workers = $workers.Clone(); $case.Workers.extra = 'success'; Check-Gate $case $false
}
$all = @(& $selector -EventName push)
$smoke = @(& $selector -EventName pull_request)
$matrix = Get-Content (Join-Path $PSScriptRoot '../benchmarks/alpha/matrix.json') -Raw | ConvertFrom-Json
if (($all -join ',') -ne (($matrix.fixtures.id) -join ',') -or $all.Count -lt 12) {
    throw 'Main must retain every matrix fixture.'
}
if (($smoke -join ',') -ne 'encyclopedia-article,layout-matrix,shadow-components,constructed-stylesheets') {
    throw 'PR smoke coverage changed unexpectedly.'
}
foreach ($selection in @(@{ ids=$all }, @{ ids=$smoke })) {
    $left = @(); $right = @()
    for ($index=0; $index -lt $selection.ids.Count; $index++) {
        if ($index % 2 -eq 0) { $left += $selection.ids[$index] } else { $right += $selection.ids[$index] }
    }
    if ($left.Count -eq 0 -or $right.Count -eq 0 -or
        @($left + $right | Sort-Object -Unique).Count -ne $selection.ids.Count) {
        throw 'Two shards must cover each selected fixture once.'
    }
}
Write-Output "CI policy tests passed ($tests gate cases; full/smoke selection and shard coverage)."
