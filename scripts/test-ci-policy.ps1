[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$gate = Join-Path $PSScriptRoot 'assert-ci-results.ps1'
$names = @('source', 'lint', 'test', 'dependencies', 'harness')
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
        $workers[$name] = if ($policy.markdown) {
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
# Test the actual workflow as well as the result gate. A runtime libtest filter
# does not save compilation of the full test tree; CI must use its small target.
$workflow = Get-Content (Join-Path $PSScriptRoot '../.github/workflows/ci.yml') -Raw
if ($workflow -notmatch 'cargo test --locked --test ci_smoke -- --test-threads=1' -or
    $workflow -notmatch 'cargo clippy --lib --bin better-web-browser --locked -- -D warnings' -or
    $workflow -notmatch '(?m)^    name: windows\r?$' -or
    $workflow -notmatch '(?m)^    name: Linear PR history\r?$') {
    throw 'Required smoke targets or protected check names are missing.'
}
if ($workflow -match 'cargo test[^\r\n]*(--lib|--all-targets|--test renderer_process)' -or
    $workflow -match 'run-wpt\.ps1|test-live-runtime\.ps1|run-alpha\.ps1|test-renderer-smoke\.ps1') {
    throw 'Full test or standards suites must run locally, not in the automatic smoke workflow.'
}
$nativeAction = Get-Content (Join-Path $PSScriptRoot '../.github/actions/windows-rust/action.yml') -Raw
if ($nativeAction -notmatch 'run: ./scripts/prepare-native-cache.ps1') {
    throw 'The Windows action must configure the native compiler cache.'
}
$angleSetup = Get-Content (Join-Path $PSScriptRoot 'prepare-angle.ps1') -Raw
if ($angleSetup -notmatch '\$env:CLANG_PATH = Join-Path \$selected ''clang.exe''' -or
    $angleSetup -notmatch '"CLANG_PATH=\$\(\$env:CLANG_PATH\)"') {
    throw 'Bindgen must use the matching Clang executable/library across Actions steps.'
}
Write-Output "CI policy tests passed ($tests result cases plus automatic smoke-target policy)."
