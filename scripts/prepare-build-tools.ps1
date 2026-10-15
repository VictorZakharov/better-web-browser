[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This native toolchain preflight requires Windows.' }
$command = Get-Command cmake.exe -CommandType Application -ErrorAction SilentlyContinue
if (-not [string]::IsNullOrWhiteSpace($env:CMAKE)) {
    if (-not [IO.Path]::IsPathFullyQualified($env:CMAKE) -or
        -not (Test-Path -LiteralPath $env:CMAKE -PathType Leaf)) {
        throw 'CMAKE must name an existing absolute executable path.'
    }
    $selected = $env:CMAKE
} elseif ($command) {
    $selected = $command.Source
} else {
    # Reuse the installed Visual Studio component; never install tools mid-build.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { throw 'CMake is missing; install the Visual Studio CMake tools or add cmake.exe to PATH.' }
    $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($installation)) { throw 'Visual Studio C++ build tools were not found.' }
    $selected = Join-Path $installation 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe'
    if (-not (Test-Path -LiteralPath $selected -PathType Leaf)) { throw 'Visual Studio CMake tools are missing; install that component before building.' }
    $env:PATH = (Split-Path -Parent $selected) + [IO.Path]::PathSeparator + $env:PATH
}
$version = & $selected --version
if ($LASTEXITCODE -ne 0) { throw 'The selected CMake executable could not start.' }
Write-Host ("Native build tool: {0} ({1})" -f $selected, @($version)[0])
