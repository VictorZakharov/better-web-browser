[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$gate = Join-Path $PSScriptRoot 'assert-ci-results.ps1'
$names = @('source', 'lint', 'test', 'live', 'wpt', 'alpha', 'dependencies', 'harness')
$tests = 0
function Check-Gate {
    param([hashtable] $Arguments, [bool] $Pass)
    $failed = $false
    try { & $gate @Arguments | Out-Null } catch { $failed = $true }
    if ($failed -eq $Pass) { throw "Unexpected gate result for $($Arguments | ConvertTo-Json -Compress)." }
    $script:tests++
}
foreach ($policy in @(
    @{ event='push'; markdown=$false },
    @{ event='pull_request'; markdown=$false },
    @{ event='pull_request'; markdown=$true }
)) {
    $workers = @{}
    foreach ($name in $names) {
        $workers[$name] = if ($policy.markdown -or ($name -in @('alpha', 'wpt', 'live') -and $policy.event -eq 'pull_request')) {
            'skipped'
        } else { 'success' }
    }
    $arguments = @{
        EventName = $policy.event
        ClassificationResult = 'success'
        RunWindows = (-not $policy.markdown).ToString().ToLowerInvariant()
        MarkdownOnly = $policy.markdown.ToString().ToLowerInvariant()
        Workers = $workers
    }
    Check-Gate $arguments $true
    foreach ($name in $names) {
        $opposite = if ($workers[$name] -eq 'success') { 'skipped' } else { 'success' }
        foreach ($result in @('failure', 'cancelled', 'pending', '', $opposite)) {
            $changed = $workers.Clone(); $changed[$name] = $result
            $case = $arguments.Clone(); $case.Workers = $changed
            Check-Gate $case $false
        }
    }
    $case = $arguments.Clone(); $case.ClassificationResult = 'failure'; Check-Gate $case $false
    $case = $arguments.Clone(); $case.RunWindows = ''; Check-Gate $case $false
    $case = $arguments.Clone(); $case.MarkdownOnly = $case.RunWindows; Check-Gate $case $false
    $case = $arguments.Clone(); $case.EventName = 'workflow_dispatch'; Check-Gate $case $false
    $case = $arguments.Clone(); $case.Workers = $workers.Clone(); $case.Workers.Remove('harness'); Check-Gate $case $false
    $case = $arguments.Clone(); $case.Workers = $workers.Clone(); $case.Workers.extra = 'success'; Check-Gate $case $false
    if ($policy.markdown) {
        $case = $arguments.Clone(); $case.EventName = 'push'; Check-Gate $case $false
    }
}
Write-Output "CI policy tests passed ($tests cases: main, source PR, Markdown-only PR, and fail-closed gates)."
