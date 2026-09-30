# Parameter-binding checks deliberately never launch a browser.
$ErrorActionPreference = 'Stop'
$runner = Join-Path $PSScriptRoot 'run-hidden-benchmark.ps1'
$tokens = $null
$errors = $null
[void] [System.Management.Automation.Language.Parser]::ParseFile($runner, [ref] $tokens, [ref] $errors)
if ($errors.Count -ne 0) { throw "Hidden launcher parse errors: $errors" }
foreach ($delay in @(-1, 60001)) {
    $rejected = $false
    try {
        & $runner -Url 'about:blank' -Output 'G:\invalid-delay-unused.json' -InitialActionDelayMs $delay
    } catch [System.Management.Automation.ParameterBindingException] {
        $rejected = $true
    }
    if (-not $rejected) { throw "Invalid initial action delay was accepted: $delay" }
}
Write-Host 'Hidden wheel option validation passed without browser execution.'
