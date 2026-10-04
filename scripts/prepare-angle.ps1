[CmdletBinding()]
param([string] $CompilerDirectory)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows -and $PSVersionTable.PSEdition -eq 'Core') {
    throw 'The current ANGLE backend is configured only for Windows.'
}

# Do not download/install a compiler or search arbitrary user profiles. The native
# dependency invokes bindgen, whose parser must understand current MSVC STL headers.
# LLVM 19+ is required by the VS 2022 14.44 headers; do not disable their version check.
function Get-CompatibleClang {
    param([string] $Directory)
    if ([string]::IsNullOrWhiteSpace($Directory)) { return $null }
    $library = Join-Path $Directory 'libclang.dll'
    if (-not (Test-Path -LiteralPath $library -PathType Leaf)) { return $null }
    if (-not (Test-Path -LiteralPath (Join-Path $Directory 'clang.exe') -PathType Leaf)) { return $null }
    $version = (Get-Item -LiteralPath $library).VersionInfo.ProductVersion
    if ($version -notmatch '^(\d+)\.') { return $null }
    if ([int] $Matches[1] -lt 19) { return $null }
    return (Resolve-Path -LiteralPath $Directory).Path
}

$explicit = if (-not [string]::IsNullOrWhiteSpace($CompilerDirectory)) {
    $CompilerDirectory
} else { $env:LIBCLANG_PATH }
if (-not [string]::IsNullOrWhiteSpace($explicit)) {
    $selected = Get-CompatibleClang $explicit
    if ($null -eq $selected) {
        throw "Expected LLVM 19+ libclang.dll and matching clang.exe in '$explicit'. Select a complete LLVM bin directory with -CompilerDirectory or LIBCLANG_PATH."
    }
} else {
    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($env:ProgramFiles)) {
        $candidates += Join-Path $env:ProgramFiles 'LLVM/bin'
    }
    $clang = Get-Command clang.exe -ErrorAction SilentlyContinue
    if ($null -ne $clang) { $candidates += Split-Path -Parent $clang.Source }
    $selected = $candidates | ForEach-Object { Get-CompatibleClang $_ } | Select-Object -First 1
    if ($null -eq $selected) {
        throw 'LLVM 19+ libclang.dll with matching clang.exe was not found. Obtain LLVM from https://github.com/llvm/llvm-project/releases and set LIBCLANG_PATH to its bin directory. On this workstation keep extracted build tooling on G:.'
    }
}

$env:LIBCLANG_PATH = $selected
# Bindgen also invokes clang.exe to discover intrinsic/system include paths.
# A developer shell may put Visual Studio's older Clang ahead of this DLL's
# installation. Pair the executable with the selected library, not PATH order.
$env:CLANG_PATH = Join-Path $selected 'clang.exe'
# Each Actions step has a new shell; persist only the validated path, not arbitrary
# compiler flags. The supported ANGLE worker policy lives in .cargo/config.toml.
if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
    if ($selected.Contains("`r") -or $selected.Contains("`n")) { throw 'Invalid compiler directory.' }
    Add-Content -LiteralPath $env:GITHUB_ENV -Value "LIBCLANG_PATH=$selected" -Encoding utf8
    Add-Content -LiteralPath $env:GITHUB_ENV -Value "CLANG_PATH=$($env:CLANG_PATH)" -Encoding utf8
}
Write-Host "ANGLE bindgen parser: $selected (LLVM 19+)"
