[CmdletBinding()]
param(
    [string] $Root,
    [ValidateRange(0, 65535)]
    [int] $Port = 0,
    [Parameter(Mandatory)]
    [string] $ReadyFile
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($Root)) {
    $Root = Join-Path $PSScriptRoot '..\benchmarks\alpha\fixtures'
}
$rootPath = (Resolve-Path -LiteralPath $Root).Path
$rootPrefix = $rootPath.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$readyPath = [IO.Path]::GetFullPath($ReadyFile)

if ($Port -eq 0) {
    $probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $probe.Start()
    try {
        $Port = ([Net.IPEndPoint] $probe.LocalEndpoint).Port
    } finally {
        $probe.Stop()
    }
}

$mimeTypes = @{
    '.css' = 'text/css; charset=utf-8'
    '.html' = 'text/html; charset=utf-8'
    '.js' = 'text/javascript; charset=utf-8'
    '.json' = 'application/json; charset=utf-8'
    '.svg' = 'image/svg+xml'
    '.ttf' = 'font/ttf'
}

function Write-Response {
    param(
        [Parameter(Mandatory)] $Context,
        [Parameter(Mandatory)] [int] $Status,
        [byte[]] $Body = @(),
        [string] $ContentType = 'text/plain; charset=utf-8'
    )

    $Context.Response.StatusCode = $Status
    $Context.Response.ContentType = $ContentType
    $Context.Response.ContentLength64 = $Body.Length
    $Context.Response.Headers['Cache-Control'] = 'no-store'
    $Context.Response.Headers['X-Content-Type-Options'] = 'nosniff'
    if ($Context.Request.HttpMethod -ne 'HEAD' -and $Body.Length -gt 0) {
        $Context.Response.OutputStream.Write($Body, 0, $Body.Length)
    }
    $Context.Response.Close()
}

function Resolve-FixturePath {
    param([Parameter(Mandatory)] [string] $RequestPath)

    $decoded = [Uri]::UnescapeDataString($RequestPath).Replace('/', [IO.Path]::DirectorySeparatorChar)
    $relative = $decoded.TrimStart([IO.Path]::DirectorySeparatorChar)
    if ([string]::IsNullOrWhiteSpace($relative) -or $relative -eq '.') {
        $relative = 'encyclopedia-main.html'
    }
    $candidate = [IO.Path]::GetFullPath((Join-Path $rootPath $relative))
    if (-not $candidate.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        return $null
    }
    return $candidate
}

$listener = [Net.HttpListener]::new()
$prefix = "http://127.0.0.1:$Port/"
$listener.Prefixes.Add($prefix)
$listener.Start()
try {
    $readyDirectory = Split-Path -Parent $readyPath
    if (-not [string]::IsNullOrWhiteSpace($readyDirectory)) {
        [IO.Directory]::CreateDirectory($readyDirectory) | Out-Null
    }
    [IO.File]::WriteAllText($readyPath, $prefix, [Text.UTF8Encoding]::new($false))
    Write-Output "Alpha fixture server: $prefix"

    while ($listener.IsListening) {
        $context = $listener.GetContext()
        try {
            if ($context.Request.HttpMethod -notin @('GET', 'HEAD')) {
                Write-Response -Context $context -Status 405
                continue
            }

            if ($context.Request.Url.AbsolutePath -eq '/system-font.ttf') {
                $font = Join-Path $env:WINDIR 'Fonts\arial.ttf'
                if (-not (Test-Path -LiteralPath $font -PathType Leaf)) {
                    Write-Response -Context $context -Status 404
                    continue
                }
                Write-Response -Context $context -Status 200 -Body ([IO.File]::ReadAllBytes($font)) -ContentType $mimeTypes['.ttf']
                continue
            }

            $path = Resolve-FixturePath -RequestPath $context.Request.Url.AbsolutePath
            if ($null -eq $path -or -not (Test-Path -LiteralPath $path -PathType Leaf)) {
                Write-Response -Context $context -Status 404
                continue
            }
            $extension = [IO.Path]::GetExtension($path).ToLowerInvariant()
            $contentType = if ($mimeTypes.ContainsKey($extension)) { $mimeTypes[$extension] } else { 'application/octet-stream' }
            Write-Response -Context $context -Status 200 -Body ([IO.File]::ReadAllBytes($path)) -ContentType $contentType
        } catch {
            if ($context.Response.OutputStream.CanWrite) {
                $body = [Text.Encoding]::UTF8.GetBytes('Fixture server error')
                Write-Response -Context $context -Status 500 -Body $body
            }
            Write-Error $_
        }
    }
} finally {
    if ($listener.IsListening) { $listener.Stop() }
    $listener.Close()
    if (Test-Path -LiteralPath $readyPath) { Remove-Item -LiteralPath $readyPath -Force }
}
