[CmdletBinding()]
param(
    [string] $Ffmpeg = 'd:\Programs\ffmpeg\bin\ffmpeg.exe',
    [string] $OutputDirectory = 'target/av1-fixture-generation',
    [string] $FixtureDirectory = 'tests/video-codec-fixtures'
)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$output=[IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
$fixtures=[IO.Path]::GetFullPath((Join-Path $repo $FixtureDirectory))
foreach($path in @($output,$fixtures)) {
    if(-not $path.StartsWith('G:\',[StringComparison]::OrdinalIgnoreCase)) {
        throw 'Generated AV1 fixture data must remain on G:.'
    }
    [IO.Directory]::CreateDirectory($path)|Out-Null
}
$source=Join-Path $output 'original.rgba'
$encoded=Join-Path $output 'original.ivf'
$reference=Join-Path $output 'reference.rgba'
$pixels=[byte[]]::new(16*16*4*8)
for($frame=0;$frame-lt 8;$frame++) {
    for($y=0;$y-lt 16;$y++) {
        for($x=0;$x-lt 16;$x++) {
            $offset=(($frame*16+$y)*16+$x)*4
            $pixels[$offset]=if($x-lt 8){200}else{20}
            $pixels[$offset+1]=32+$frame*16
            $pixels[$offset+2]=if($y-lt 8){40}else{180}
            $pixels[$offset+3]=255
        }
    }
}
[IO.File]::WriteAllBytes($source,$pixels)
function Invoke-FixtureTool([string[]] $Arguments) {
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=$Ffmpeg
    $info.UseShellExecute=$false
    $info.CreateNoWindow=$true
    foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
    $process=[Diagnostics.Process]::Start($info)
    try {
        if(-not $process.WaitForExit(30000)){$process.Kill($true);throw 'AV1 fixture encoder exceeded 30 seconds.'}
        if($process.ExitCode-ne 0){throw "FFmpeg fixture tool failed: $($process.ExitCode)"}
    }finally{$process.Dispose()}
}
Invoke-FixtureTool @('-hide_banner','-loglevel','error','-nostdin','-y','-f','rawvideo',
    '-pixel_format','rgba','-video_size','16x16','-framerate','4','-i',$source,
    '-an','-c:v','libaom-av1','-threads','1','-cpu-used','8','-crf','0','-b:v','0',
    '-g','8','-pix_fmt','yuv420p','-color_range','pc','-colorspace','bt709',
    '-color_primaries','bt709','-color_trc','iec61966-2-1','-f','ivf',$encoded)
Invoke-FixtureTool @('-hide_banner','-loglevel','error','-nostdin','-y','-threads','1',
    '-i',$encoded,'-an','-pix_fmt','rgba','-f','rawvideo',$reference)
foreach($pair in @(@('original.ivf','eight-frames.ivf.base64'),@('reference.rgba','eight-frames.rgba.base64'))) {
    $bytes=[IO.File]::ReadAllBytes((Join-Path $output $pair[0]))
    $text=[Convert]::ToBase64String($bytes)
    $lines=[Collections.Generic.List[string]]::new()
    for($offset=0;$offset-lt $text.Length;$offset+=76) {
        $lines.Add($text.Substring($offset,[Math]::Min(76,$text.Length-$offset)))
    }
    [IO.File]::WriteAllLines((Join-Path $fixtures $pair[1]),$lines)
}
Write-Host "Original eight-frame AV1 fixture and independent reference: $fixtures"
