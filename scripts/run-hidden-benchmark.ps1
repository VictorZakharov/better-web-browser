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
    [ValidateRange(5, 600)]
    [int] $TimeoutSeconds = 120,
    [ValidateRange(0, 120)]
    [int] $ScrollSamples = 0,
    [switch] $EarlyScrollTrace,
    [ValidateRange(320, 7680)]
    [int] $WindowWidth = 1280,
    [ValidateRange(240, 4320)]
    [int] $WindowHeight = 720,
    [ValidatePattern('^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$')]
    [string] $Locale = 'en-US',
    [switch] $FreshProfile,
    [string[]] $DiagnosticSelector = @()
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($Browser)) {
    $Browser = Join-Path $repoRoot 'target\release\better-web-browser.exe'
}
$Browser = (Resolve-Path $Browser).Path
$outputPath = [System.IO.Path]::GetFullPath($Output)
$profilePath = $null
if ($FreshProfile) {
    $profilePath = Join-Path ([IO.Path]::GetTempPath()) ("breeze-benchmark-" + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($profilePath) | Out-Null
}
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
if ($FreshProfile) {
    $startInfo.EnvironmentVariables['BREEZE_PROFILE_DIRECTORY'] = $profilePath
}
$startInfo.EnvironmentVariables['BREEZE_BENCHMARK_LOCALE'] = $Locale

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
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        throw "Hidden Breeze benchmark exceeded the $TimeoutSeconds-second process timeout."
    }
    # Flush redirected asynchronous reads after the bounded wait succeeds.
    $process.WaitForExit()
} finally {
    if (-not $process.HasExited) {
        $process.Kill()
        $process.WaitForExit()
    }
    if ($FreshProfile -and (Test-Path -LiteralPath $profilePath)) {
        $fullProfile = [IO.Path]::GetFullPath($profilePath)
        $expectedPrefix = Join-Path ([IO.Path]::GetTempPath()) 'breeze-benchmark-'
        if (-not $fullProfile.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove an unexpected Breeze benchmark profile path.'
        }
        Remove-Item -LiteralPath $fullProfile -Recurse -Force
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
$record = Get-Content -LiteralPath $outputPath -Raw | ConvertFrom-Json
$record | Add-Member -NotePropertyName locale -NotePropertyValue $Locale
$record | Add-Member -NotePropertyName fresh_profile -NotePropertyValue ([bool] $FreshProfile)
$record | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $outputPath -Encoding UTF8
Write-Output $outputPath
