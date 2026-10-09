# Parameter-binding checks deliberately never launch a browser.
$ErrorActionPreference = 'Stop'
$runner = Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1'
$tokens = $null
$errors = $null
[void] [System.Management.Automation.Language.Parser]::ParseFile($runner, [ref] $tokens, [ref] $errors)
if ($errors.Count -ne 0) { throw "Hidden launcher parse errors: $errors" }
. (Join-Path $PSScriptRoot 'hidden-benchmark-actions.ps1')
$actions = @('scroll:5000', 'move:500,5300', 'wheel:500,300,126', 'pause:500', 'move:500,5426', 'wheel:500,300,-126', 'pause:0')
$expected = @('--scroll-after-ready', '5000', '--move-after-ready', '500,5300', '--wheel-after-ready', '500,300,126',
    '--pause-after-ready', '500', '--move-after-ready', '500,5426', '--wheel-after-ready', '500,300,-126', '--pause-after-ready', '0')
$actual = @(Get-HiddenBenchmarkActionArguments -ActionSequence $actions)
if (($actual -join '|') -ne ($expected -join '|')) { throw 'Ordered actions lost their original order or values.' }
if (@(Get-HiddenBenchmarkActionArguments).Count -ne 0) { throw 'Empty ordered actions invented an argument.' }
foreach ($action in @('move:0,0', 'move:2147483647,2147483647', 'wheel:7680,4320,-10000', 'scroll:2147483647', 'pause:60000')) {
    if (@(Get-HiddenBenchmarkActionArguments -ActionSequence @($action)).Count -ne 2) { throw "Valid action rejected: $action" }
}
foreach ($action in @('', ' ', 'move:', 'move:-1,0', 'move:1,2,3', 'move:2147483648,0',
    'wheel:7681,0,1', 'wheel:0,4321,1', 'wheel:1,2,10001', 'wheel:1,2,-2147483648', 'wheel:1,2,NaN',
    'scroll:-1', 'scroll:2147483648', 'pause:-1', 'pause:60001', 'pause:1.5', 'pause:1e3', 'unknown:1',
    'pause:1;--task-manager', 'move:1,2 --screenshot')) {
    $rejected = $false
    try { [void] (Get-HiddenBenchmarkActionArguments -ActionSequence @($action)) } catch { $rejected = $true }
    if (-not $rejected) { throw "Malformed ordered action accepted: $action" }
    # The real wrapper must reject before resolving a browser or touching output.
    $rejected = $false
    try { & $runner -Url 'about:blank' -Output 'G:\invalid-action-unused.json' -Browser 'G:\nonexistent-hidden-browser.exe' -ActionSequence @($action) }
    catch { $rejected = $_.Exception.Message -notmatch 'nonexistent-hidden-browser' }
    if (-not $rejected) { throw "Wrapper did not validate before executable resolution: $action" }
}
foreach ($delay in @(-1, 60001)) {
    $rejected = $false
    try {
        & $runner -Url 'about:blank' -Output 'G:\invalid-delay-unused.json' -InitialActionDelayMs $delay
    } catch [System.Management.Automation.ParameterBindingException] {
        $rejected = $true
    }
    if (-not $rejected) { throw "Invalid initial action delay was accepted: $delay" }
}
foreach ($budget in @('standard', 'graphics')) {
    foreach ($profile in @($null, 'G:\existing-profile-must-not-change')) {
        $options = @{
            Url = 'about:blank'; Output = 'G:\invalid-budget-unused.json'
            Browser = 'G:\nonexistent-hidden-browser.exe'; RendererMemoryBudget = $budget
        }
        if ($null -ne $profile) { $options.ProfileDirectory = $profile }
        $rejected = $false
        try { & $runner @options }
        catch { $rejected = $_.Exception.Message -match 'requires -FreshProfile' }
        if (-not $rejected) { throw "Memory budget could mutate a non-fresh profile: $budget" }
    }
}
foreach ($budget in @('', '2g', 'unlimited', 'GRAPHICS --benchmark')) {
    $rejected = $false
    try {
        & $runner -Url 'about:blank' -Output 'G:\invalid-budget-unused.json' -FreshProfile -RendererMemoryBudget $budget
    } catch [System.Management.Automation.ParameterBindingException] {
        $rejected = $true
    }
    if (-not $rejected) { throw "Unknown memory budget accepted: $budget" }
}
Write-Host 'Hidden wheel option validation passed without browser execution.'
