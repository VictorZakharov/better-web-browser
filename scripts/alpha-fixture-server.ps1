# Owned hidden fixture-server lifecycle. Reuse the required PowerShell 7 runtime.
function Start-AlphaFixtureServer {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $OutputDirectory,
        [string] $Root,
        [string] $Script = (Join-Path $PSScriptRoot 'serve-alpha-fixtures.ps1'),
        [ValidateRange(1, 60)][int] $TimeoutSeconds = 10
    )
    [IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
    $ready = Join-Path $OutputDirectory 'fixture-server.txt'
    if (Test-Path -LiteralPath $ready) { Remove-Item -LiteralPath $ready -Force }
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = Join-Path $PSHOME 'pwsh.exe'
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in @('-NoLogo', '-NoProfile', '-NonInteractive', '-File', $Script, '-ReadyFile', $ready)) {
        $startInfo.ArgumentList.Add($argument)
    }
    if ($Root) {
        $startInfo.ArgumentList.Add('-Root')
        $startInfo.ArgumentList.Add($Root)
    }
    $process = [Diagnostics.Process]::Start($startInfo)
    $server = [pscustomobject]@{
        Process = $process
        Output = $process.StandardOutput.ReadToEndAsync()
        Error = $process.StandardError.ReadToEndAsync()
        Directory = $OutputDirectory
        Url = $null
    }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    try {
        while (-not (Test-Path -LiteralPath $ready)) {
            if ($process.HasExited) { throw "Fixture server exited with code $($process.ExitCode)." }
            if ($clock.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
                throw "Fixture server startup exceeded $TimeoutSeconds seconds (PID $($process.Id))."
            }
            Start-Sleep -Milliseconds 20
        }
        $server.Url = (Get-Content -LiteralPath $ready -Raw).Trim()
        $address = [Uri]$server.Url
        if ($address.Scheme -ne 'http' -or $address.Host -ne '127.0.0.1' -or $address.Port -le 0) {
            throw 'Fixture server supplied an invalid loopback URL.'
        }
        Write-Host ('Fixture server ready in {0:N0} ms (PID {1})' -f $clock.Elapsed.TotalMilliseconds, $process.Id)
        return $server
    } catch {
        $failure = $_
        Stop-AlphaFixtureServer $server
        $detail = Get-Content -LiteralPath (Join-Path $OutputDirectory 'fixture-server.err') -Raw
        throw "$failure`n$detail"
    }
}

function Stop-AlphaFixtureServer {
    param($Server)
    if ($null -eq $Server) { return }
    try {
        if (-not $Server.Process.HasExited) { $Server.Process.Kill($true) }
        $Server.Process.WaitForExit()
        [IO.File]::WriteAllText((Join-Path $Server.Directory 'fixture-server.out'), $Server.Output.GetAwaiter().GetResult())
        [IO.File]::WriteAllText((Join-Path $Server.Directory 'fixture-server.err'), $Server.Error.GetAwaiter().GetResult())
    } finally {
        $Server.Process.Dispose()
        $ready = Join-Path $Server.Directory 'fixture-server.txt'
        if (Test-Path -LiteralPath $ready) { Remove-Item -LiteralPath $ready -Force }
    }
}
