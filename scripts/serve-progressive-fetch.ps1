[CmdletBinding()]
param([Parameter(Mandatory)][string] $ReadyFile)

# Small owned deterministic transport: first/last are not copies of site responses.
$ErrorActionPreference = 'Stop'
$fixtureRoot = (Resolve-Path (Join-Path $PSScriptRoot '../benchmarks/alpha/fixtures')).Path
$probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$probe.Start()
$port = ([Net.IPEndPoint] $probe.LocalEndpoint).Port
$probe.Stop()
$listener = [Net.HttpListener]::new()
$prefix = "http://127.0.0.1:$port/"
$listener.Prefixes.Add($prefix)
$listener.Start()
$readyPath = [IO.Path]::GetFullPath($ReadyFile)
[IO.Directory]::CreateDirectory((Split-Path -Parent $readyPath)) | Out-Null
[IO.File]::WriteAllText($readyPath, $prefix)
Write-Output $prefix
$phases = @{}
$streams = [Collections.Generic.List[object]]::new()

function Send-Complete($context, [byte[]]$bytes, [string]$mime) {
    $context.Response.ContentType = $mime
    $context.Response.ContentLength64 = $bytes.Length
    $context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
    $context.Response.Close()
}

try {
    $accept = $listener.GetContextAsync()
    while ($listener.IsListening) {
        if ($accept.IsCompleted) {
            $context = $accept.GetAwaiter().GetResult()
            $accept = $listener.GetContextAsync()
            $path = $context.Request.Url.AbsolutePath
            $token = [string]$context.Request.QueryString['token']
            $context.Response.Headers['Cache-Control'] = 'no-store'
            if ($path -eq '/chunks') {
                $context.Response.ContentType = 'application/octet-stream'
                # HttpListener buffers small fixed-length bodies until Close. Explicit
                # chunking makes first-byte timing a transport fact, not a timer assumption.
                $context.Response.SendChunked = $true
                $context.Response.Headers['X-Content-Type-Options'] = 'nosniff'
                $context.Response.OutputStream.Flush()
                $phases[$token] = 0
                $streams.Add(@{ Context=$context; Token=$token; Stage=0; Due=[DateTime]::UtcNow.AddMilliseconds(300) })
            } elseif ($path -eq '/phase') {
                $phase = if ($phases.ContainsKey($token)) { [string]$phases[$token] } else { '-1' }
                Send-Complete $context ([Text.Encoding]::UTF8.GetBytes($phase)) 'application/json'
            } elseif ($path -in @('/progressive-fetch.html','/progressive-fetch.js')) {
                $mime = if ($path.EndsWith('.js')) { 'application/javascript' } else { 'text/html; charset=utf-8' }
                Send-Complete $context ([IO.File]::ReadAllBytes((Join-Path $fixtureRoot $path.TrimStart('/')))) $mime
            } else {
                $context.Response.StatusCode = 404
                Send-Complete $context @() 'text/plain'
            }
        }
        for ($index = $streams.Count - 1; $index -ge 0; $index--) {
            $stream = $streams[$index]
            if ([DateTime]::UtcNow -lt $stream.Due) { continue }
            try {
                if ($stream.Stage -eq 0) {
                    $bytes = [Text.Encoding]::ASCII.GetBytes(('f' * 65536))
                    $stream.Context.Response.OutputStream.Write($bytes,0,$bytes.Length)
                    $stream.Context.Response.OutputStream.Flush()
                    $stream.Stage = 1
                    $phases[$stream.Token] = 1
                    $stream.Due = [DateTime]::UtcNow.AddMilliseconds(1500)
                } else {
                    $bytes = [Text.Encoding]::ASCII.GetBytes('last')
                    $phases[$stream.Token] = 2
                    $stream.Context.Response.OutputStream.Write($bytes,0,$bytes.Length)
                    $stream.Context.Response.Close()
                    $streams.RemoveAt($index)
                }
            } catch {
                $stream.Context.Response.Abort()
                $streams.RemoveAt($index)
            }
        }
        [Threading.Thread]::Sleep(5)
    }
} finally {
    foreach ($stream in $streams) { $stream.Context.Response.Abort() }
    $listener.Stop()
    $listener.Close()
}
