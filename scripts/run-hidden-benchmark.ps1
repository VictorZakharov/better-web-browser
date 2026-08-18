[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Url,
    [Parameter(Mandatory)]
    [string] $Output,
    [string] $Browser,
    [string] $Screenshot,
    [ValidateRange(100, 60000)]
    [int] $SettleMs = 2000,
    [ValidateRange(0, 120)]
    [int] $ScrollSamples = 0,
    [switch] $EarlyScrollTrace,
    [ValidateRange(320, 7680)]
    [int] $WindowWidth = 1280,
    [ValidateRange(240, 4320)]
    [int] $WindowHeight = 720,
    [string[]] $DiagnosticSelector = @()
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($Browser)) {
    $Browser = Join-Path $repoRoot 'target\release\better-web-browser.exe'
}
$Browser = (Resolve-Path $Browser).Path
$outputPath = [System.IO.Path]::GetFullPath($Output)
$outputDirectory = Split-Path -Parent $outputPath
if (-not [string]::IsNullOrEmpty($outputDirectory)) {
    [System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
}

function ConvertTo-WindowsCommandLineArgument {
    param([AllowEmptyString()][string] $Value)

    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') {
        return $Value
    }
    $builder = [System.Text.StringBuilder]::new()
    [void] $builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $slashes++
            continue
        }
        if ($character -eq '"') {
            [void] $builder.Append(('\' * ($slashes * 2 + 1)))
            [void] $builder.Append('"')
            $slashes = 0
            continue
        }
        if ($slashes -gt 0) {
            [void] $builder.Append(('\' * $slashes))
            $slashes = 0
        }
        [void] $builder.Append($character)
    }
    if ($slashes -gt 0) {
        [void] $builder.Append(('\' * ($slashes * 2)))
    }
    [void] $builder.Append('"')
    $builder.ToString()
}

$arguments = [System.Collections.Generic.List[string]]::new()
$arguments.Add('--benchmark')
$arguments.Add($Url)
$arguments.Add('--output')
$arguments.Add($outputPath)
$arguments.Add('--settle-ms')
$arguments.Add($SettleMs.ToString([System.Globalization.CultureInfo]::InvariantCulture))
$arguments.Add('--window-width')
$arguments.Add($WindowWidth.ToString([System.Globalization.CultureInfo]::InvariantCulture))
$arguments.Add('--window-height')
$arguments.Add($WindowHeight.ToString([System.Globalization.CultureInfo]::InvariantCulture))
if ($ScrollSamples -gt 0) {
    $arguments.Add('--scroll-samples')
    $arguments.Add($ScrollSamples.ToString([System.Globalization.CultureInfo]::InvariantCulture))
}
if ($EarlyScrollTrace) {
    $arguments.Add('--early-scroll-trace')
}
if (-not [string]::IsNullOrWhiteSpace($Screenshot)) {
    $arguments.Add('--screenshot')
    $arguments.Add([System.IO.Path]::GetFullPath($Screenshot))
}
foreach ($selector in $DiagnosticSelector) {
    $arguments.Add('--diagnostic-selector')
    $arguments.Add($selector)
}

$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $Browser
$startInfo.Arguments = ($arguments | ForEach-Object {
    ConvertTo-WindowsCommandLineArgument $_
}) -join ' '
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
$startInfo.EnvironmentVariables['BREEZE_REQUIRE_HIDDEN_BENCHMARK'] = '1'

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $startInfo
if (-not $process.Start()) {
    throw 'Could not start the hidden Breeze benchmark.'
}
$stdout = $process.StandardOutput.ReadToEndAsync()
$stderr = $process.StandardError.ReadToEndAsync()
try {
    $actual = Get-CimInstance Win32_Process -Filter "ProcessId = $($process.Id)"
    if ($null -eq $actual -or $actual.CommandLine -notmatch '(?i)(?:^|\s)--benchmark(?:\s|$)') {
        if (-not $process.HasExited) {
            $process.Kill()
        }
        throw 'Refusing benchmark run: the actual Breeze child command line lacks --benchmark.'
    }
    $process.WaitForExit()
} finally {
    if (-not $process.HasExited) {
        $process.Kill()
        $process.WaitForExit()
    }
}

$standardOutput = $stdout.GetAwaiter().GetResult()
$standardError = $stderr.GetAwaiter().GetResult()
if (-not [string]::IsNullOrWhiteSpace($standardOutput)) {
    Write-Output $standardOutput.TrimEnd()
}
if ($process.ExitCode -ne 0) {
    $detail = if ([string]::IsNullOrWhiteSpace($standardError)) {
        'no stderr was reported'
    } else {
        $standardError.Trim()
    }
    throw "Hidden Breeze benchmark failed with exit code $($process.ExitCode): $detail"
}
if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
    throw "Hidden Breeze benchmark did not create $outputPath."
}
if (-not [string]::IsNullOrWhiteSpace($standardError)) {
    Write-Warning $standardError.Trim()
}
Write-Output $outputPath
