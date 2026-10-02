[CmdletBinding()]
param(
    [string] $Browser,
    [string] $OutputDirectory = 'target/elementary-audio-proof',
    [switch] $Chrome,
    [switch] $SkipExport,
    [Nullable[int]] $ExpectedPassed
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$outputRoot = [IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
if (-not $outputRoot.StartsWith('G:\',[StringComparison]::OrdinalIgnoreCase)) {
    throw 'Elementary audio captures and synthetic packet staging must stay on G:.'
}
$stage = Join-Path $outputRoot 'fixture'
$scratch = Join-Path $outputRoot 'temp'
[IO.Directory]::CreateDirectory($stage) | Out-Null
[IO.Directory]::CreateDirectory($scratch) | Out-Null
$previous = @{}
foreach ($name in @('TEMP','TMP','CARGO_HOME','CARGO_TARGET_DIR','BREEZE_AUDIO_CASES_OUTPUT')) {
    $previous[$name] = [Environment]::GetEnvironmentVariable($name,'Process')
}
try {
    $env:TEMP = $scratch
    $env:TMP = $scratch
    $env:CARGO_HOME = Join-Path $repo 'target/cargo-home'
    $env:CARGO_TARGET_DIR = Join-Path $repo 'target'
    $env:BREEZE_AUDIO_CASES_OUTPUT = Join-Path $stage 'cases.js'
    if (-not $SkipExport) {
        Push-Location $repo
        try {
            cargo test --locked --lib export_elementary_audio_cases -- --ignored --exact `
                engine::script::audio_codecs::test_packets::export_elementary_audio_cases
            if ($LASTEXITCODE -ne 0) { throw 'Synthetic elementary packet export failed.' }
        } finally { Pop-Location }
    }
    if (-not (Test-Path -LiteralPath $env:BREEZE_AUDIO_CASES_OUTPUT)) {
        throw 'Generated cases.js is missing; omit -SkipExport for the initial run.'
    }
    Copy-Item -LiteralPath (Join-Path $repo 'tests/audio-codec-fixtures/probe.js') -Destination $stage
    foreach ($name in @('probe.html','decode.js','encode.js','lifecycle.js')) {
        Copy-Item -LiteralPath (Join-Path $repo "tests/elementary-audio-fixtures/$name") -Destination $stage
    }
    # Keep launch policy in the shared, fail-closed runner. In particular this
    # wrapper never launches an interactive browser to collect codec results.
    $arguments = @{OutputDirectory=$OutputDirectory;FixtureRoot=$stage;Chrome=$Chrome;SettleMs=15000}
    if ($Browser) { $arguments.Browser=$Browser }
    if ($null -ne $ExpectedPassed) { $arguments.ExpectedPassed=$ExpectedPassed }
    & (Join-Path $PSScriptRoot 'test-audio-codecs.ps1') @arguments
} finally {
    foreach ($name in $previous.Keys) {
        [Environment]::SetEnvironmentVariable($name,$previous[$name],'Process')
    }
}
