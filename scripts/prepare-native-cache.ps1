[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native compiler caching requires Windows.' }
# cc-rs' automatic MSVC discovery bypasses its RUSTC_WRAPPER fallback.
# Explicit wrapping also requires the genuine MSVC headers/libraries in the
# environment; setting CXX to a bare cl.exe without a developer shell fails.
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { throw 'Visual Studio discovery tool is missing.' }
$installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($installation)) { throw 'MSVC x64 installation is missing.' }
$module = Join-Path $installation 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll'
if (-not (Test-Path -LiteralPath $module -PathType Leaf)) { throw 'Visual Studio developer shell module is missing.' }
Import-Module $module
Enter-VsDevShell -VsInstallPath $installation -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
Get-Command cl.exe -ErrorAction Stop | Out-Null
Get-Command sccache.exe -ErrorAction Stop | Out-Null
if ([string]::IsNullOrWhiteSpace($env:INCLUDE) -or [string]::IsNullOrWhiteSpace($env:LIB)) {
    throw 'MSVC headers/libraries were not configured.'
}
$env:CXX_x86_64_pc_windows_msvc = 'sccache cl.exe'
if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
    # Preserve only toolchain configuration across Actions steps, not the rest
    # of the process environment (which can include credentials).
    $names = @('PATH', 'INCLUDE', 'LIB', 'LIBPATH', 'VCINSTALLDIR', 'VSINSTALLDIR',
        'VCToolsInstallDir', 'VCToolsVersion', 'WindowsSdkDir', 'WindowsSDKVersion',
        'WindowsSDKLibVersion', 'UniversalCRTSdkDir', 'UCRTVersion', 'VisualStudioVersion',
        'VSCMD_VER', 'VSCMD_ARG_TGT_ARCH', 'VSCMD_ARG_HOST_ARCH', 'CXX_x86_64_pc_windows_msvc')
    foreach ($name in $names) {
        $value = [Environment]::GetEnvironmentVariable($name, 'Process')
        if ([string]::IsNullOrEmpty($value)) { continue }
        if ($value.Contains("`r") -or $value.Contains("`n")) { throw "Invalid compiler environment: $name." }
        Add-Content -LiteralPath $env:GITHUB_ENV -Value "$name=$value" -Encoding utf8
    }
}
Write-Host 'Native C++ cache configured: sccache with MSVC x64 developer environment.'
