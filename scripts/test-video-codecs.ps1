[CmdletBinding()]
param(
    [string] $Browser,
    [string] $OutputDirectory = 'target/video-codecs-proof',
    [switch] $Chrome,
    [Nullable[int]] $ExpectedPassed
)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
$output=[IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
if(-not $output.StartsWith('G:\',[StringComparison]::OrdinalIgnoreCase)) {
    throw 'Video codec captures, profiles and packet staging must remain on G:.'
}
$stage=Join-Path $output 'fixture'
$scratch=Join-Path $output 'temp'
[IO.Directory]::CreateDirectory($stage)|Out-Null
[IO.Directory]::CreateDirectory($scratch)|Out-Null
$previous=@{}
foreach($name in @('TEMP','TMP','CARGO_HOME','CARGO_TARGET_DIR','BREEZE_VIDEO_CASES_OUTPUT')) {
    $previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process')
}
try {
    $env:TEMP=$scratch;$env:TMP=$scratch
    $env:CARGO_HOME=Join-Path $repo 'target/cargo-home'
    $env:CARGO_TARGET_DIR=Join-Path $repo 'target'
    $env:BREEZE_VIDEO_CASES_OUTPUT=Join-Path $stage 'cases.js'
    Push-Location $repo
    try {
        cargo test --locked --lib engine::script::video_codecs::test_packets::export_video_codec_cases -- --ignored --exact
        if($LASTEXITCODE-ne 0){throw 'Original video packet export failed.'}
    }finally{Pop-Location}
    Copy-Item -LiteralPath (Join-Path $repo 'tests/audio-codec-fixtures/probe.js') -Destination $stage
    foreach($name in @('probe.html','video.js')) {
        Copy-Item -LiteralPath (Join-Path $repo "tests/video-codec-fixtures/$name") -Destination $stage
    }
    $arguments=@{OutputDirectory=$OutputDirectory;FixtureRoot=$stage;Chrome=$Chrome;SettleMs=5000}
    if($Browser){$arguments.Browser=$Browser}
    if($null-ne $ExpectedPassed){$arguments.ExpectedPassed=$ExpectedPassed}
    # This shared runner verifies hidden Breeze arguments and delegates Chromium
    # to the unified-headless/CreateNoWindow launcher; no ad-hoc launch is added.
    & (Join-Path $PSScriptRoot 'test-audio-codecs.ps1') @arguments
}finally{
    foreach($name in $previous.Keys){[Environment]::SetEnvironmentVariable($name,$previous[$name],'Process')}
}
