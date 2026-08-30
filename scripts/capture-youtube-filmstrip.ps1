[CmdletBinding()]
param(
    [string] $OutputDirectory,
    [string] $Browser,
    [string] $ChromiumHarness,
    [ValidateRange(100, 10000)]
    [int] $IntervalMs = 500,
    [ValidateRange(100, 60000)]
    [int] $DurationMs = 15000,
    [ValidateRange(0, 60000)]
    [int] $NavigationDelayMs = 2500,
    [ValidateRange(320, 7680)]
    [int] $WindowWidth = 1280,
    [ValidateRange(240, 4320)]
    [int] $WindowHeight = 720,
    [ValidatePattern('^\d+\s*,\s*\d+$')]
    [string] $ClickPoint = '657,325',
    [string] $BreezePlaySelector = '.ytmCuedOverlayPlayButton',
    [string] $TargetUrl = 'https://www.youtube-nocookie.com/embed/jNQXAC9IVRw?autoplay=1&mute=1'
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')
    $OutputDirectory = Join-Path $repoRoot "target\youtube-filmstrip-$stamp"
}
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) {
    throw "Output directory already exists: $output"
}
[IO.Directory]::CreateDirectory($output) | Out-Null

if ([string]::IsNullOrWhiteSpace($Browser)) {
    $Browser = Join-Path $repoRoot 'target\release\better-web-browser.exe'
}
if ([string]::IsNullOrWhiteSpace($ChromiumHarness)) {
    $ChromiumHarness = Join-Path $repoRoot 'benchmarks\chromium\bin\Release\net8.0\ChromiumBaseline.exe'
}
$Browser = (Resolve-Path $Browser).Path
$ChromiumHarness = (Resolve-Path $ChromiumHarness).Path

if ($DurationMs % $IntervalMs -ne 0) {
    throw '-DurationMs must be a multiple of -IntervalMs.'
}
if ([string]::IsNullOrWhiteSpace($BreezePlaySelector)) {
    throw '-BreezePlaySelector cannot be empty.'
}

if (-not ('Breeze.YouTubeEvidenceServer' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

#pragma warning disable 4014
namespace Breeze {
    public sealed class YouTubeEvidenceServer : IDisposable {
        private readonly TcpListener listener;
        private readonly CancellationTokenSource cancellation = new CancellationTokenSource();
        private readonly byte[] body;
        public int Port { get; private set; }

        public YouTubeEvidenceServer(string html) {
            body = new UTF8Encoding(false).GetBytes(html);
            listener = new TcpListener(IPAddress.Loopback, 0);
            listener.Start();
            Port = ((IPEndPoint)listener.LocalEndpoint).Port;
            ServeAsync();
        }

        private async Task ServeAsync() {
            while (!cancellation.IsCancellationRequested) {
                TcpClient client;
                try { client = await listener.AcceptTcpClientAsync(); }
                catch (ObjectDisposedException) { return; }
                catch (SocketException) {
                    if (cancellation.IsCancellationRequested) return;
                    throw;
                }
                RespondAsync(client);
            }
        }

        private async Task RespondAsync(TcpClient client) {
            using (client) {
                var stream = client.GetStream();
                using (var reader = new StreamReader(
                    stream, Encoding.ASCII, false, 1024, true)) {
                    string line;
                    do { line = await reader.ReadLineAsync() ?? string.Empty; }
                    while (line.Length != 0);
                }
                var header = new UTF8Encoding(false).GetBytes(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n" +
                    "Cache-Control: no-store\r\nConnection: close\r\nContent-Length: " +
                    body.Length + "\r\n\r\n");
                await stream.WriteAsync(header, 0, header.Length);
                await stream.WriteAsync(body, 0, body.Length);
            }
        }

        public void Dispose() {
            cancellation.Cancel();
            listener.Stop();
            cancellation.Dispose();
        }
    }
}
'@
}

function Assert-Filmstrip {
    param([Parameter(Mandatory)][string] $Directory)
    $manifestPath = Join-Path $Directory 'manifest.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $expected = [int] ($DurationMs / $IntervalMs)
    if ($manifest.frames.Count -ne $expected) {
        throw "Filmstrip $Directory has $($manifest.frames.Count) frames; expected $expected."
    }
    foreach ($frame in $manifest.frames) {
        if (-not [string]::IsNullOrWhiteSpace([string] $frame.error)) {
            throw "Filmstrip frame failed at $($frame.scheduled_ms) ms: $($frame.error)"
        }
        if (-not (Test-Path -LiteralPath (Join-Path $Directory $frame.file))) {
            throw "Filmstrip frame is missing: $($frame.file)"
        }
    }
    $manifest
}

$target = $null
try { $target = [Uri] $TargetUrl }
catch { throw "Invalid YouTube target URL: $TargetUrl" }
$allowedTargetHosts = @('youtube.com', 'www.youtube.com', 'm.youtube.com', 'www.youtube-nocookie.com')
if (-not $target.IsAbsoluteUri -or $target.Scheme -ne 'https' -or
    $allowedTargetHosts -notcontains $target.DnsSafeHost.ToLowerInvariant()) {
    throw "YouTube target must use HTTPS on an approved YouTube host: $TargetUrl"
}
$targetHref = [Net.WebUtility]::HtmlEncode($target.AbsoluteUri)
$launcher = @"
<!doctype html><title>YouTube evidence launcher</title>
<a href="$targetHref">Open the public non-DRM YouTube reference video</a>
"@
$server = [Breeze.YouTubeEvidenceServer]::new($launcher)
try {
    $url = "http://127.0.0.1:$($server.Port)/youtube-evidence"
    $breezeDirectory = Join-Path $output 'breeze'
    $chromiumDirectory = Join-Path $output 'chromium'
    [IO.Directory]::CreateDirectory($breezeDirectory) | Out-Null
    [IO.Directory]::CreateDirectory($chromiumDirectory) | Out-Null

    & (Join-Path $repoRoot 'scripts\run-hidden-benchmark.ps1') `
        -Url $url `
        -Browser $Browser `
        -Output (Join-Path $breezeDirectory 'report.json') `
        -Screenshot (Join-Path $breezeDirectory 'final.png') `
        -FilmstripDirectory (Join-Path $breezeDirectory 'frames') `
        -FilmstripIntervalMs $IntervalMs `
        -FilmstripDurationMs $DurationMs `
        -SettleMs 100 `
        -TimeoutSeconds 120 `
        -WindowWidth $WindowWidth `
        -WindowHeight $WindowHeight `
        -FreshProfile `
        -DiagnosticSelector @('video', '#movie_player', 'button', $BreezePlaySelector) `
        -LinkActivationTarget $target `
        -SelectorActivationTarget $BreezePlaySelector `
        -NavigationDelayMs $NavigationDelayMs

    $breeze = Get-Content -LiteralPath (Join-Path $breezeDirectory 'report.json') -Raw |
        ConvertFrom-Json
    if ($null -ne $breeze.error -or -not [bool] $breeze.media.playing -or
        [double] $breeze.media.current_time_seconds -le 0) {
        throw "Breeze did not reach advancing media playback: $($breeze.error)"
    }

    $chromiumArguments = @(
        '--url', $url,
        '--output', (Join-Path $chromiumDirectory 'report.json'),
        '--screenshot', (Join-Path $chromiumDirectory 'final.png'),
        '--filmstrip-directory', (Join-Path $chromiumDirectory 'frames'),
        '--filmstrip-interval-ms', [string] $IntervalMs,
        '--filmstrip-duration-ms', [string] $DurationMs,
        '--settle-ms', '100',
        '--timeout-ms', '120000',
        '--viewport-width', [string] ([int] $breeze.viewport_width_css_px),
        '--viewport-height', [string] ([int] $breeze.viewport_height_css_px),
        '--device-scale-factor', ([double] $breeze.device_scale_factor).ToString(
            [Globalization.CultureInfo]::InvariantCulture),
        '--activate-link-after-ready', $target,
        '--click-after-ready', $ClickPoint,
        '--navigation-delay-ms', [string] $NavigationDelayMs
    )
    & $ChromiumHarness @chromiumArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Chromium evidence run exited with $LASTEXITCODE."
    }
    $chromium = Get-Content -LiteralPath (Join-Path $chromiumDirectory 'report.json') -Raw |
        ConvertFrom-Json
    if ($null -ne $chromium.error -or [double] $chromium.media.current_time_seconds -le 0) {
        throw "Chromium did not reach advancing media playback: $($chromium.error)"
    }

    $breezeFilmstrip = Assert-Filmstrip (Join-Path $breezeDirectory 'frames')
    $chromiumFilmstrip = Assert-Filmstrip (Join-Path $chromiumDirectory 'frames')
    [ordered]@{
        captured_at_utc = [DateTime]::UtcNow.ToString('O')
        launcher_url = $url
        target_url = $target.AbsoluteUri
        interval_ms = $IntervalMs
        duration_ms = $DurationMs
        click_point = $ClickPoint
        breeze_play_selector = $BreezePlaySelector
        navigation_delay_ms = $NavigationDelayMs
        breeze = [ordered]@{
            report = 'breeze/report.json'
            final_screenshot = 'breeze/final.png'
            filmstrip_manifest = 'breeze/frames/manifest.json'
            frames = $breezeFilmstrip.frames.Count
            current_time_seconds = $breeze.media.current_time_seconds
        }
        chromium = [ordered]@{
            report = 'chromium/report.json'
            final_screenshot = 'chromium/final.png'
            filmstrip_manifest = 'chromium/frames/manifest.json'
            frames = $chromiumFilmstrip.frames.Count
            current_time_seconds = $chromium.media.current_time_seconds
        }
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'manifest.json')
    Write-Host "YouTube comparison evidence written to $output"
}
finally {
    $server.Dispose()
}
