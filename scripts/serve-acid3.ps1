# Serves a fetched Acid3 copy on loopback with the recorded status codes and content types.
# One hook is added after the test loop finishes: it POSTs the score and failure log to
# /__report?label=<name>, which is written to <name>.json beside the ready file, and then sets the
# document title so hidden runs can stop as soon as the report is stored.
[CmdletBinding()]
param([Parameter(Mandatory)][string] $ReadyFile, [Parameter(Mandatory)][string] $Root)
$ErrorActionPreference = 'Stop'
$manifest = Import-PowerShellDataFile (Join-Path $PSScriptRoot 'acid3-files.psd1')
$routes = @{}
foreach ($entry in $manifest.Files) {
    $name = if ($entry.File) { $entry.File } else { $entry.Path.TrimStart('/') }
    $routes[$entry.Path] = @{ Status = $entry.Status; ContentType = $entry.ContentType; Body = [IO.File]::ReadAllBytes((Join-Path $Root $name)) }
}
$missing = $manifest.NotFound
$notFound = @{ Status = $missing.Status; ContentType = $missing.ContentType; Body = [IO.File]::ReadAllBytes((Join-Path $Root $missing.Body)) }

# The end of Acid3's update() loop; the hook goes after it, outside the `if (errors == 0)` branch.
$endOfRun = "      if (errors == 0)`n        log += `"\nNo JS errors and no timing issues.\nWas the rendering pixel-for-pixel perfect too?`";`n"
$hook = "      (function () { try { var x = new XMLHttpRequest(); x.open('POST', '/__report' + location.search, true);" +
    " x.setRequestHeader('Content-Type', 'text/plain');" +
    " x.onloadend = function () { document.title = 'Acid3 reported'; };" +
    " x.send(JSON.stringify({ score: score, total: tests.length, log: log })); } catch (e) {} })();`n"
$page = [Text.Encoding]::UTF8.GetString($routes['/'].Body)
$at = $page.IndexOf($endOfRun, [StringComparison]::Ordinal)
if ($at -lt 0 -or $page.IndexOf($endOfRun, $at + 1, [StringComparison]::Ordinal) -ge 0) { throw 'Acid3 end-of-run marker was not found exactly once.' }
$routes['/'].Body = [Text.Encoding]::UTF8.GetBytes($page.Insert($at + $endOfRun.Length, $hook))

$probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$probe.Start()
$port = ([Net.IPEndPoint]$probe.LocalEndpoint).Port
$probe.Stop()
$listener = [Net.HttpListener]::new()
$prefix = "http://127.0.0.1:$port/"
$listener.Prefixes.Add($prefix)
$listener.Start()
$directory = Split-Path -Parent ([IO.Path]::GetFullPath($ReadyFile))
[IO.File]::WriteAllText($ReadyFile + '.tmp', $prefix)
[IO.File]::Move($ReadyFile + '.tmp', $ReadyFile)
try {
    while ($listener.IsListening) {
        $context = $listener.GetContext()
        try {
            $request = $context.Request
            $response = $context.Response
            $response.Headers['Cache-Control'] = 'no-store'
            if ($request.HttpMethod -eq 'POST' -and $request.Url.AbsolutePath -eq '/__report') {
                $label = $request.QueryString['label']
                if ($label -notmatch '^[A-Za-z0-9-]{1,40}$') { $response.StatusCode = 400; continue }
                $reader = [IO.StreamReader]::new($request.InputStream, [Text.Encoding]::UTF8)
                $report = $reader.ReadToEnd()
                $reader.Dispose()
                [IO.File]::WriteAllText((Join-Path $directory "$label.json.tmp"), $report)
                [IO.File]::Move((Join-Path $directory "$label.json.tmp"), (Join-Path $directory "$label.json"), $true)
                $response.StatusCode = 204
                continue
            }
            $route = $routes[$request.Url.AbsolutePath]
            if ($null -eq $route) { $route = $notFound }
            $response.StatusCode = $route.Status
            $response.ContentType = $route.ContentType
            $response.ContentLength64 = $route.Body.Length
            $response.OutputStream.Write($route.Body, 0, $route.Body.Length)
        } catch {
            [Console]::Error.WriteLine("Acid3 request failed: $_")
        } finally { $context.Response.Close() }
    }
} finally { $listener.Close() }
