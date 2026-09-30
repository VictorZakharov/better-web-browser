[CmdletBinding()]
param(
    [ValidateSet('debug', 'release', 'both')]
    [string] $Profile = 'both',
    [string] $TargetDirectory = $env:CARGO_TARGET_DIR
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'assert-opus-build-policy.ps1')
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $TargetDirectory) { $TargetDirectory = Join-Path $repoRoot 'target' }
$TargetDirectory = [IO.Path]::GetFullPath($TargetDirectory)
# Local artifacts belong on G:. Hosted CI may explicitly choose its runner's scratch volume.
if ($env:GITHUB_ACTIONS -ne 'true' -and [IO.Path]::GetPathRoot($TargetDirectory) -ne 'G:\') {
    throw 'Local Opus policy builds require a target directory on G:.'
}
$temporary = Join-Path $TargetDirectory 'opus-policy/temp'
[IO.Directory]::CreateDirectory($temporary) | Out-Null
$saved = @{ TEMP = $env:TEMP; TMP = $env:TMP; CARGO_TARGET_DIR = $env:CARGO_TARGET_DIR }
$profiles = if ($Profile -eq 'both') { @('debug', 'release') } else { @($Profile) }
Push-Location $repoRoot
try {
    $env:TEMP = $temporary; $env:TMP = $temporary; $env:CARGO_TARGET_DIR = $TargetDirectory
    foreach ($selectedProfile in $profiles) {
        $arguments = @('build', '--locked', '-p', 'opusic-sys', '--message-format=json')
        if ($selectedProfile -eq 'release') { $arguments += '--release' }
        Write-Host "Checking pinned Opus native policy ($selectedProfile)..."
        # Native stdout is read as data, never transformed into shell commands. Stderr remains diagnostic.
        $lines = [Collections.Generic.List[string]]::new()
        $outputSize = 0
        & cargo @arguments | ForEach-Object {
            $line = [string] $_
            $outputSize += $line.Length
            if ($line.Length -gt 1MB -or $outputSize -gt 16MB) { throw 'Cargo JSON output exceeds policy bounds.' }
            $lines.Add($line)
        }
        if ($LASTEXITCODE -ne 0) { throw "Cargo Opus build failed ($selectedProfile): $LASTEXITCODE" }
        $out = Get-OpusCargoOutput -Lines $lines -TargetDirectory $TargetDirectory -Profile $selectedProfile
        $result = Assert-OpusBuildPolicy -OutDirectory $out
        Write-Host "Verified $($result.Generator), native $($result.NativeConfiguration): $($result.Archive)"
    }
} finally {
    $env:TEMP = $saved.TEMP; $env:TMP = $saved.TMP; $env:CARGO_TARGET_DIR = $saved.CARGO_TARGET_DIR
    Pop-Location
}
