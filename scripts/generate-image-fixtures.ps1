[CmdletBinding()]
param(
    [string] $OutputDirectory,
    [string] $FixtureDirectory,
    [string] $Ffmpeg = 'ffmpeg'
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repoRoot 'target/modern-images/fixtures' }
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
$fixtureOutput = if ($FixtureDirectory) { [IO.Path]::GetFullPath($FixtureDirectory) } else { $taskOutput }
foreach ($directory in @($taskOutput, $fixtureOutput)) {
    if ([IO.Path]::GetPathRoot($directory) -ine 'G:\') {
        throw 'Generated image fixtures and scratch files must stay on G:.'
    }
    [void] (New-Item -ItemType Directory -Force -Path $directory)
}

function Invoke-FixtureEncoder([string[]] $ToolArguments) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = (Get-Command $Ffmpeg -ErrorAction Stop).Source
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in $ToolArguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw 'Could not start the fixture encoder.' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) {
            $process.Kill($true)
            throw 'The image fixture encoder exceeded 30 seconds.'
        }
        $output = $stdout.GetAwaiter().GetResult()
        $errors = $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "Image fixture encoder failed: $errors" }
        return $output
    } finally { $process.Dispose() }
}

function Write-Base64Fixture([string] $Binary, [string] $Name) {
    $bytes = [IO.File]::ReadAllBytes($Binary)
    $base64 = [Convert]::ToBase64String($bytes)
    $lines = for ($offset = 0; $offset -lt $base64.Length; $offset += 100) {
        $base64.Substring($offset, [Math]::Min(100, $base64.Length - $offset))
    }
    [IO.File]::WriteAllText((Join-Path $fixtureOutput "$Name.base64"),
        (($lines -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
    [pscustomobject]@{
        File = $Name
        Bytes = $bytes.Length
        Sha256 = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

# Six owned pixels, not downloaded photographs or third-party assets. The
# rectangular dimensions exercise chroma rounding and orientation axis swaps.
$opaquePixels = [byte[]] @(
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255,
    255, 255, 255, 255, 0, 0, 0, 255, 128, 64, 32, 255
)
$alphaPixels = [byte[]] @(
    255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64,
    255, 255, 255, 0, 0, 0, 0, 255, 128, 64, 32, 192
)
$opaque = Join-Path $taskOutput 'six-colors.rgba'
$alpha = Join-Path $taskOutput 'six-colors-alpha.rgba'
[IO.File]::WriteAllBytes($opaque, $opaquePixels)
[IO.File]::WriteAllBytes($alpha, $alphaPixels)
$version = (Invoke-FixtureEncoder @('-version')).Split("`n")[0].Trim()
Write-Output "Generator: $version"

$formats = @(
    @{ Name = 'rgb-lossless.avif'; Codec = 'libaom-av1'; PixelFormat = 'gbrp'; Extra = @('-colorspace', 'rgb') },
    @{ Name = 'yuv420-full.avif'; Codec = 'libaom-av1'; PixelFormat = 'yuv420p'; Extra = @('-colorspace', 'bt709') },
    @{ Name = 'yuv422-limited.avif'; Codec = 'libaom-av1'; PixelFormat = 'yuv422p'; Limited = $true; Extra = @('-colorspace', 'smpte170m') },
    @{ Name = 'yuv444-10.avif'; Codec = 'libaom-av1'; PixelFormat = 'yuv444p10le'; Extra = @('-colorspace', 'bt709') },
    @{ Name = 'yuv420-12.avif'; Codec = 'libaom-av1'; PixelFormat = 'yuv420p12le'; Extra = @('-colorspace', 'bt709') },
    @{ Name = 'gray.avif'; Codec = 'libaom-av1'; PixelFormat = 'gray'; Extra = @('-colorspace', 'bt709') },
    @{ Name = 'gray10.avif'; Codec = 'libaom-av1'; PixelFormat = 'gray10le'; Extra = @('-colorspace', 'bt709') },
    @{ Name = 'rgb-lossless.jxl'; Codec = 'libjxl'; PixelFormat = 'rgb24'; Extra = @() },
    @{ Name = 'rgba-lossless.jxl'; Codec = 'libjxl'; PixelFormat = 'rgba'; Alpha = $true; Extra = @() },
    @{ Name = 'gray-lossless.jxl'; Codec = 'libjxl'; PixelFormat = 'gray'; Extra = @() },
    @{ Name = 'graya-lossless.jxl'; Codec = 'libjxl'; PixelFormat = 'ya8'; Alpha = $true; Extra = @() },
    @{ Name = 'rgb16-lossless.jxl'; Codec = 'libjxl'; PixelFormat = 'rgb48le'; Extra = @() },
    @{ Name = 'rgba16-lossless.jxl'; Codec = 'libjxl'; PixelFormat = 'rgba64le'; Alpha = $true; Extra = @() },
    @{ Name = 'rgb-lossy.jxl'; Codec = 'libjxl'; PixelFormat = 'rgb24'; Lossy = $true; Extra = @() }
)
foreach ($format in $formats) {
    $binary = Join-Path $taskOutput $format.Name
    $arguments = @('-hide_banner', '-loglevel', 'error', '-f', 'rawvideo', '-pixel_format', 'rgba',
        '-video_size', '3x2', '-i', $(if ($format.Alpha) { $alpha } else { $opaque }),
        '-frames:v', '1', '-map_metadata', '-1', '-threads', '1', '-c:v', $format.Codec,
        '-pix_fmt', $format.PixelFormat)
    if ($format.Codec -eq 'libaom-av1') {
        $arguments += @('-crf', '0', '-b:v', '0', '-cpu-used', '8', '-still-picture', '1',
            '-color_primaries', 'bt709', '-color_trc', 'iec61966-2-1',
            '-color_range', $(if ($format.Limited) { 'tv' } else { 'pc' }), '-f', 'avif')
    } else {
        $arguments += @('-effort', '3', '-distance', $(if ($format.Lossy) { '1' } else { '0' }))
    }
    [void] (Invoke-FixtureEncoder ($arguments + $format.Extra + @('-y', $binary)))
    Write-Base64Fixture $binary $format.Name
    # Store the independent native decoder's output as a reference, not the
    # output of Breeze's decoder under test. YUV comparisons allow small rounding.
    $reference = Join-Path $taskOutput ($format.Name + '.rgba')
    [void] (Invoke-FixtureEncoder @('-hide_banner', '-loglevel', 'error', '-i', $binary,
        '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgba', '-y', $reference))
    Write-Base64Fixture $reference ($format.Name + '.rgba')
}

# AVIF's alpha is a separate monochrome auxiliary AV1 item, not a fourth
# interleaved channel. Both streams are still pictures encoded losslessly.
$alphaAvif = Join-Path $taskOutput 'rgba-lossless.avif'
[void] (Invoke-FixtureEncoder @('-hide_banner', '-loglevel', 'error', '-f', 'rawvideo',
    '-pixel_format', 'rgba', '-video_size', '3x2', '-i', $alpha,
    '-filter_complex', '[0:v]split=2[color][coverage];[color]format=gbrp[rgb];[coverage]alphaextract[alpha]',
    '-map', '[rgb]', '-map', '[alpha]', '-frames:v', '1', '-threads', '1',
    '-c:v', 'libaom-av1', '-crf', '0', '-b:v', '0', '-cpu-used', '8', '-still-picture', '1',
    '-color_primaries', 'bt709', '-color_trc', 'iec61966-2-1', '-color_range', 'pc',
    '-colorspace:v:0', 'rgb', '-pix_fmt:v:0', 'gbrp', '-pix_fmt:v:1', 'gray',
    '-f', 'avif', '-y', $alphaAvif))
Write-Base64Fixture $alphaAvif 'rgba-lossless.avif'
# This reference is the owned input pattern, not FFmpeg's legacy AVIF demuxer
# (which exposes color and alpha as separate streams instead of an RGBA image).
Write-Base64Fixture $alpha 'rgba-lossless.avif.rgba'
