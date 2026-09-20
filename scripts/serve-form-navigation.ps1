[CmdletBinding()]
param([Parameter(Mandatory)][string] $ReadyFile)
$ErrorActionPreference = 'Stop'
$probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$probe.Start()
$port = ([Net.IPEndPoint]$probe.LocalEndpoint).Port
$probe.Stop()
$listener = [Net.HttpListener]::new()
$prefix = "http://127.0.0.1:$port/"
$listener.Prefixes.Add($prefix)
$listener.Start()
$directory = Split-Path -Parent ([IO.Path]::GetFullPath($ReadyFile))
[IO.Directory]::CreateDirectory($directory) | Out-Null
[IO.File]::WriteAllText((Join-Path $directory 'requests.json'), '[]')
[IO.File]::WriteAllText($ReadyFile + '.tmp', $prefix)
[IO.File]::Move($ReadyFile + '.tmp', $ReadyFile)
$requests = [Collections.Generic.List[object]]::new()
function Escape([string] $value) { [Net.WebUtility]::HtmlEncode($value) }
try {
    while ($listener.IsListening) {
        $context = $listener.GetContext()
        try {
            $request = $context.Request
            $reader = [IO.StreamReader]::new($request.InputStream, [Text.Encoding]::UTF8)
            $body = $reader.ReadToEnd()
            $reader.Dispose()
            $requests.Add([ordered]@{ method=$request.HttpMethod; path=$request.RawUrl; body=$body;
                content_type=$request.ContentType; origin=$request.Headers['Origin']; referer=$request.Headers['Referer'] })
            [IO.File]::WriteAllText((Join-Path $directory 'requests.json'), (ConvertTo-Json -Depth 5 -InputObject @($requests.ToArray())))
            $path = $request.Url.AbsolutePath
            $case = $request.QueryString['case']
            $q = $request.QueryString['q']
            $javascript = $path -eq '/relay.js'
            if ($path -eq '/iframe-initial') {
                $html = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../tests/fixtures/iframe-initial-document.html') -Raw
            } elseif ($path -eq '/redirect') {
                $context.Response.StatusCode = 303
                $context.Response.RedirectLocation = '/echo?redirected=yes'
                $html = 'Redirecting'
            } elseif ($path -eq '/echo') {
                $html = '<h1 id="result">Request received</h1><pre id="request">' + (Escape ($requests[$requests.Count - 1] | ConvertTo-Json)) + '</pre>'
            } elseif ($path -eq '/destination') {
                $html = '<h1 id="result">Destination</h1><p>Use Back, then search again.</p>'
            } elseif ($path -eq '/frame-relay') {
                $html = '<script>addEventListener("message",e=>{if(e.origin===location.origin && e.source===parent && e.data==="continue")parent.location.href="/destination"});parent.postMessage("ready",location.origin);</script>'
            } elseif ($path -eq '/frame-external') {
                $context.Response.Headers['Content-Security-Policy'] = "default-src 'none'; script-src 'self'; frame-ancestors 'self'"
                $html = '<script src="/relay.js"></script>'
            } elseif ($javascript) {
                $html = 'addEventListener("message",e=>{if(e.origin===location.origin && e.source===parent && e.data==="continue")parent.location.href="/destination"});parent.postMessage("ready",location.origin);'
            } elseif ($path -eq '/flow') {
                $html = '<form action="/flow?discard=yes"><input id="query" name="q" value="' + (Escape $q) + '"><button>Search</button></form>'
                $html += '<script>const query=document.getElementById("query");let state=query.value;query.oninput=()=>{if(query.type==="text")state=query.value};query.form.onsubmit=()=>{query.value=state};</script>'
                if ($q) {
                    $html += '<h1 id="result">Results: ' + (Escape $q) + '</h1><a id="result-link" href="/destination">Open result</a>'
                    $html += '<script>document.querySelector("a").onclick=e=>{e.preventDefault();const frame=document.createElement("iframe");frame.src="/frame-external";addEventListener("message",e=>{if(e.origin===location.origin && e.source===frame.contentWindow && e.data==="ready")frame.contentWindow.postMessage("continue",location.origin)});document.body.append(frame)};</script>'
                } else {
                    $html += '<script>document.querySelector("input").value="first";document.querySelector("form").submit();</script>'
                }
            } else {
                $method = if ($case -in @('post','plain','multipart','redirect')) { 'post' } else { 'get' }
                $encoding = if ($case -eq 'plain') { 'text/plain' } elseif ($case -eq 'multipart') { 'multipart/form-data' } else { 'application/x-www-form-urlencoded' }
                $action = if ($case -eq 'redirect') { '/redirect' } else { '/echo?old=discard#fragment' }
                $html = '<button id="start" type="button">Run</button><form id="f" method="' + $method + '" enctype="' + $encoding + '" action="' + $action + '">' +
                    '<input name="q" value="before"><textarea name="text">line1&#10;line2</textarea><button name="go" value="yes">Submit</button></form>'
                $code = switch ($case) {
                    'direct' { 'f.onsubmit=()=>{throw Error("unexpected submit")};f.submit();' }
                    'cancel' { 'f.onsubmit=e=>e.preventDefault();f.requestSubmit();document.title="Canceled";' }
                    'removed' { 'f.submit();queueMicrotask(()=>f.remove());document.title="Canceled";' }
                    'removed-during-submit' { 'f.onsubmit=()=>f.remove();f.requestSubmit();document.title="Canceled";' }
                    'synthetic' { 'const a=document.createElement("a");a.href="/destination";a.click();' }
                    'iframe' { 'const frame=document.createElement("iframe");frame.src="/frame-relay";addEventListener("message",e=>{if(e.origin===location.origin && e.source===frame.contentWindow && e.data==="ready")frame.contentWindow.postMessage("continue",location.origin)});document.body.appendChild(frame);' }
                    'iframe-external' { 'const frame=document.createElement("iframe");frame.src="/frame-external";addEventListener("message",e=>{if(e.origin===location.origin && e.source===frame.contentWindow && e.data==="ready")frame.contentWindow.postMessage("continue",location.origin)});document.body.appendChild(frame);' }
                    default { 'f.requestSubmit(f.querySelector("button"));' }
                }
                $html += '<script>document.getElementById("start").onclick=()=>{const f=document.getElementById("f");f.querySelector("input").value="standards 🦀";' + $code + '};</script>'
            }
            if (-not $javascript) { $html = '<!doctype html><meta charset="utf-8"><title>Form navigation fixture</title><style>body{margin:20px;font:16px Arial}#start{width:100px;height:40px}form{margin-top:20px}</style>' + $html }
            $bytes = [Text.Encoding]::UTF8.GetBytes($html)
            $context.Response.ContentType = if ($javascript) { 'text/javascript; charset=utf-8' } else { 'text/html; charset=utf-8' }
            $context.Response.Headers['Cache-Control'] = 'no-store'
            $context.Response.ContentLength64 = $bytes.Length
            $context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
        } finally { $context.Response.Close() }
    }
} finally { $listener.Close() }
