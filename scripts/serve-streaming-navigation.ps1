[CmdletBinding()]
param([Parameter(Mandatory)][string] $ReadyFile)

# Owned navigation fixture: the main response has a deliberately withheld tail.
$ErrorActionPreference = 'Stop'
$fixture = Get-Content (Join-Path $PSScriptRoot '../benchmarks/alpha/fixtures/streaming-navigation.html') -Raw
$parts = $fixture -split '<!-- RESPONSE TAIL -->', 2
$probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$probe.Start()
$port = ([Net.IPEndPoint]$probe.LocalEndpoint).Port
$probe.Stop()
$listener = [Net.HttpListener]::new()
$prefix = "http://127.0.0.1:$port/"
$listener.Prefixes.Add($prefix)
$listener.Start()
$readyPath = [IO.Path]::GetFullPath($ReadyFile)
[IO.Directory]::CreateDirectory((Split-Path -Parent $readyPath)) | Out-Null
[IO.File]::WriteAllText($readyPath, $prefix)
Write-Output $prefix
$streams = [Collections.Generic.List[object]]::new()

function Send-Bytes($context, [string]$text, [string]$mime) {
    $bytes = [Text.Encoding]::UTF8.GetBytes($text)
    $context.Response.ContentType = $mime
    $context.Response.ContentLength64 = $bytes.Length
    $context.Response.OutputStream.Write($bytes,0,$bytes.Length)
    $context.Response.Close()
}

try {
    $accept = $listener.GetContextAsync()
    while ($listener.IsListening) {
        if ($accept.IsCompleted) {
            $context = $accept.GetAwaiter().GetResult()
            $accept = $listener.GetContextAsync()
            switch ($context.Request.Url.AbsolutePath) {
                '/streaming-navigation.css' {
                    Send-Bytes $context 'body{font:20px system-ui;margin:40px;color:#132a36;max-width:1000px}section{padding:24px;margin:24px 0;border:2px solid #16803a;border-radius:12px}#prefix{background:rgb(220,252,231)}#tail{background:#e0f2fe}#result{font:18px monospace}' 'text/css; charset=utf-8'
                }
                '/streaming-navigation.js' {
                    $token = $context.Request.QueryString['navigation']
                    $early = if ($streams | Where-Object { $_.Token -eq $token }) { 'true' } else { 'false' }
                    Send-Bytes $context "window.externalBeforeTail=$early;trace.push('external-'+document.readyState);" 'text/javascript; charset=utf-8'
                }
                '/favicon.ico' { $context.Response.StatusCode = 204; $context.Response.Close() }
                default {
                    $context.Response.ContentType = 'text/html; charset=utf-8'
                    $context.Response.SendChunked = $true
                    $token = [Guid]::NewGuid().ToString('N')
                    $source = $parts[0].Replace('/streaming-navigation.js', "/streaming-navigation.js?navigation=$token")
                    $bytes = [Text.Encoding]::UTF8.GetBytes($source)
                    $context.Response.OutputStream.Write($bytes,0,$bytes.Length)
                    $context.Response.OutputStream.Flush()
                    $streams.Add(@{Context=$context;Token=$token;Due=[DateTime]::UtcNow.AddMilliseconds(2500)})
                }
            }
        }
        for ($index=$streams.Count-1; $index -ge 0; $index--) {
            if ([DateTime]::UtcNow -lt $streams[$index].Due) { continue }
            $context = $streams[$index].Context
            try {
                $bytes = [Text.Encoding]::UTF8.GetBytes($parts[1])
                $context.Response.OutputStream.Write($bytes,0,$bytes.Length)
                $context.Response.Close()
            } catch { $context.Response.Abort() }
            $streams.RemoveAt($index)
        }
        [Threading.Thread]::Sleep(5)
    }
} finally {
    foreach ($stream in $streams) { $stream.Context.Response.Abort() }
    $listener.Stop()
    $listener.Close()
}
