[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'alpha-fixture-server.ps1')
$repo = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $repo ('target/fixture server tests/' + [Guid]::NewGuid().ToString('N'))
for ($iteration = 0; $iteration -lt 3; $iteration++) {
    $server = Start-AlphaFixtureServer -OutputDirectory (Join-Path $testRoot "success $iteration")
    $serverId = $server.Process.Id
    try {
        $response = Invoke-WebRequest ($server.Url + 'inline-script-insertion.html')
        if ($response.StatusCode -ne 200) { throw 'Fixture server response failed.' }
    } finally { Stop-AlphaFixtureServer $server }
    if (Get-Process -Id $serverId -ErrorAction SilentlyContinue) { throw 'Fixture server leaked.' }
}
$failure = $null
try {
    Start-AlphaFixtureServer -OutputDirectory (Join-Path $testRoot 'invalid root') -Root (Join-Path $testRoot 'missing')
} catch { $failure = $_ }
if ($null -eq $failure -or "$failure" -notmatch 'exited with code' -or "$failure" -notmatch 'does not exist') {
    throw "Fixture startup failure did not retain stderr: $failure"
}
$failure = $null
$timeoutRoot = Join-Path $testRoot 'timeout'
try {
    Start-AlphaFixtureServer -OutputDirectory $timeoutRoot -TimeoutSeconds 2 -Script (Join-Path $PSScriptRoot 'tests/fixture-server-never-ready.ps1')
} catch { $failure = $_ }
if ($null -eq $failure -or "$failure" -notmatch 'startup exceeded') { throw "Missing startup timeout: $failure" }
$serverId = [int](Get-Content -LiteralPath (Join-Path $timeoutRoot 'fixture-server.txt.pid'))
if (Get-Process -Id $serverId -ErrorAction SilentlyContinue) { throw 'Timed-out fixture server leaked.' }
Write-Output 'Fixture launcher success, spaced paths, startup failure and timeout cleanup passed.'
