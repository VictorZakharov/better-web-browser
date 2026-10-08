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

# A browser can close while a large resource response is already being sent.
# Force TCP resets after receiving response bytes, then prove the same server
# still handles later requests. No browser or visible console is launched here.
$disconnectRoot = Join-Path $testRoot 'client disconnect'
[IO.Directory]::CreateDirectory($disconnectRoot) | Out-Null
$resourceBytes = 8 * 1024 * 1024
[IO.File]::WriteAllBytes((Join-Path $disconnectRoot 'large.bin'), [byte[]]::new($resourceBytes))
$server = Start-AlphaFixtureServer -OutputDirectory (Join-Path $testRoot 'disconnect server') -Root $disconnectRoot
$serverId = $server.Process.Id
try {
    $address = [Uri]$server.Url
    for ($iteration = 0; $iteration -lt 3; $iteration++) {
        $client = [Net.Sockets.TcpClient]::new()
        try {
            $client.ReceiveBufferSize = 1024
            $client.ReceiveTimeout = 5000
            $client.LingerState = [Net.Sockets.LingerOption]::new($true, 0)
            $client.Connect($address.Host, $address.Port)
            $stream = $client.GetStream()
            $request = [Text.Encoding]::ASCII.GetBytes("GET /large.bin HTTP/1.1`r`nHost: $($address.Host):$($address.Port)`r`nConnection: close`r`n`r`n")
            $stream.Write($request, 0, $request.Length)
            $prefix = [byte[]]::new(512)
            if ($stream.Read($prefix, 0, $prefix.Length) -le 0) { throw 'No response before forced client reset.' }
        } finally { $client.Dispose() }
        $response = Invoke-WebRequest ($server.Url + 'large.bin') -Method Head -TimeoutSec 5
        if ($response.StatusCode -ne 200 -or [long]$response.Headers['Content-Length'][0] -ne $resourceBytes) {
            throw 'Fixture server stopped serving after an abandoned response.'
        }
        if ($server.Process.HasExited) { throw 'Client disconnect terminated the fixture server.' }
    }
} finally { Stop-AlphaFixtureServer $server }
if (Get-Process -Id $serverId -ErrorAction SilentlyContinue) { throw 'Disconnect-test fixture server leaked.' }
Write-Output 'Fixture lifecycle, startup failures, timeout cleanup and abandoned responses passed.'
