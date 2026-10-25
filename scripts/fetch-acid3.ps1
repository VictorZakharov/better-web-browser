# Downloads the Acid3 files listed in acid3-files.psd1 into a local directory and verifies their
# hashes. Files already present with the right hash are kept, so repeated runs work offline.
[CmdletBinding()]
param([Parameter(Mandatory)][string] $Directory)
$ErrorActionPreference = 'Stop'
$manifest = Import-PowerShellDataFile (Join-Path $PSScriptRoot 'acid3-files.psd1')
$Directory = [IO.Path]::GetFullPath($Directory)
[IO.Directory]::CreateDirectory($Directory) | Out-Null

function Get-Sha256([byte[]] $Bytes) {
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant()
}

foreach ($entry in $manifest.Files) {
    $name = if ($entry.File) { $entry.File } else { $entry.Path.TrimStart('/') }
    $target = Join-Path $Directory $name
    if ((Test-Path -LiteralPath $target) -and (Get-Sha256 ([IO.File]::ReadAllBytes($target))) -eq $entry.Sha256) { continue }
    $response = Invoke-WebRequest -Uri ($manifest.Origin + $entry.Path.TrimStart('/')) -SkipHttpErrorCheck -TimeoutSec 30
    $bytes = $response.RawContentStream.ToArray()
    $hash = Get-Sha256 $bytes
    if ($hash -ne $entry.Sha256) {
        throw "Acid3 file $($entry.Path) changed upstream: expected SHA-256 $($entry.Sha256), got $hash. Review the new file before updating acid3-files.psd1."
    }
    if ([int]$response.StatusCode -ne $entry.Status) {
        Write-Warning "Acid3 file $($entry.Path) now returns HTTP $($response.StatusCode) (recorded $($entry.Status)); the local server keeps the recorded status."
    }
    [IO.File]::WriteAllBytes($target, $bytes)
}
