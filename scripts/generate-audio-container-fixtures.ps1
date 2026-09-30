[CmdletBinding()]
param(
    [string] $OutputDirectory,
    [string] $FixtureDirectory,
    [string] $Ffmpeg = 'ffmpeg',
    [string] $Ffprobe = 'ffprobe'
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot 'target/audio-containers/fixtures'
}
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
if ([IO.Path]::GetPathRoot($taskOutput) -ine 'G:\') {
    throw 'Generated audio fixtures and scratch files must stay on G:.'
}
[void] (New-Item -ItemType Directory -Force -Path $taskOutput)
$fixtureOutput = if ([string]::IsNullOrWhiteSpace($FixtureDirectory)) {
    $taskOutput
} else { [IO.Path]::GetFullPath($FixtureDirectory) }
if ([IO.Path]::GetPathRoot($fixtureOutput) -ine 'G:\') {
    throw 'Generated Base64 fixture files must stay on G:.'
}
[void] (New-Item -ItemType Directory -Force -Path $fixtureOutput)

function Invoke-HiddenTool([string] $Executable, [string[]] $ToolArguments) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = (Get-Command $Executable -ErrorAction Stop).Source
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in $ToolArguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw "Could not start $Executable." }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $output = $stdout.GetAwaiter().GetResult()
        $errors = $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "$Executable failed: $errors" }
        return $output
    } finally { $process.Dispose() }
}

$version = (Invoke-HiddenTool $Ffmpeg @('-version')).Split("`n")[0].Trim()
$formats = @(
    @{ Extension = 'aac'; Codec = 'aac'; Container = 'adts'; Options = @('-b:a', '96k') },
    @{ Extension = 'webm'; Codec = 'libvorbis'; Container = 'webm'; Options = @('-q:a', '2') },
    @{ Extension = 'oga'; Codec = 'flac'; Container = 'ogg'; Options = @('-sample_fmt', 's16') },
    @{ Extension = 'webm'; Name = 'test-0.4s-opus'; Codec = 'libopus'; Container = 'webm'; Options = @('-b:a', '32k') },
    @{ Extension = 'webm'; Name = 'test-0.4s-mixed'; Codec = 'libvorbis'; Container = 'webm'; Video = $true; Options = @('-q:a', '2', '-c:v', 'libvpx', '-deadline', 'realtime', '-b:v', '20k') }
)
foreach ($format in $formats) {
    $baseName = if ($format.Name) { $format.Name } else { 'test-0.4s-tone' }
    $name = "$baseName.$($format.Extension)"
    $binary = Join-Path $taskOutput $name
    $arguments = @(
        '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
        'sine=frequency=440:sample_rate=44100:duration=0.4'
    )
    if ($format.Video) {
        $arguments += @('-f', 'lavfi', '-i', 'color=c=black:size=16x16:rate=10:duration=0.4')
    }
    $arguments += @('-ac', '1',
        '-map_metadata', '-1', '-fflags', '+bitexact', '-flags:a', '+bitexact',
        '-c:a', $format.Codec
    ) + $format.Options + @('-f', $format.Container, '-y', $binary)
    [void] (Invoke-HiddenTool $Ffmpeg $arguments)
    $probe = Invoke-HiddenTool $Ffprobe @(
        '-v', 'error', '-show_entries', 'stream=codec_name,sample_rate,channels:format=duration',
        '-of', 'json', $binary
    ) | ConvertFrom-Json
    $bytes = [IO.File]::ReadAllBytes($binary)
    $base64 = [Convert]::ToBase64String($bytes)
    $lines = for ($offset = 0; $offset -lt $base64.Length; $offset += 100) {
        $base64.Substring($offset, [Math]::Min(100, $base64.Length - $offset))
    }
    [IO.File]::WriteAllText(
        (Join-Path $fixtureOutput "$name.base64"),
        (($lines -join "`n") + "`n"), [Text.UTF8Encoding]::new($false)
    )
    [pscustomobject]@{
        File = $name
        Bytes = $bytes.Length
        Sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
        Codec = $probe.streams.codec_name -join ','
        SampleRate = $probe.streams[0].sample_rate
        Channels = $probe.streams[0].channels
        ContainerDuration = $probe.format.duration
        Generator = $version
    }
}
