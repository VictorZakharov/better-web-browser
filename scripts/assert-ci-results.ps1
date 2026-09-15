[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $ClassificationResult,
    [Parameter(Mandatory)] [AllowEmptyString()] [string] $RunWindows,
    [Parameter(Mandatory)] [AllowEmptyString()] [string] $MarkdownOnly,
    [Parameter(Mandatory)] [hashtable] $Workers
)

$ErrorActionPreference = 'Stop'
$required = @('source', 'lint', 'test', 'wpt', 'alpha', 'dependencies', 'harness')
if ($ClassificationResult -ne 'success') { throw 'Change classification did not succeed.' }
if ($Workers.Count -ne $required.Count -or
    @($required | Where-Object { -not $Workers.ContainsKey($_) }).Count -ne 0) {
    throw 'The CI gate must receive every required worker exactly once.'
}
$expected = if ($RunWindows -ceq 'false' -and $MarkdownOnly -ceq 'true') {
    'skipped'
} elseif ($RunWindows -ceq 'true' -and $MarkdownOnly -ceq 'false') {
    'success'
} else {
    throw "Invalid change-classification outputs: run_windows='$RunWindows', markdown_only='$MarkdownOnly'."
}
foreach ($name in $required) {
    if ($Workers[$name] -cne $expected) {
        throw "Worker '$name' must be '$expected', received '$($Workers[$name])'."
    }
}
Write-Output "CI gate passed: all required workers are $expected."
