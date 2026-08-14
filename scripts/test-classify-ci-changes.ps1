[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$classifier = Join-Path $PSScriptRoot 'classify-ci-changes.ps1'
$testsRun = 0

function Invoke-Classification {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name,

        [Parameter(Mandatory = $true)]
        [hashtable] $Arguments,

        [Parameter(Mandatory = $true)]
        [bool] $ExpectedRunWindows,

        [Parameter(Mandatory = $true)]
        [bool] $ExpectedMarkdownOnly,

        [string] $WorkingDirectory
    )

    $outputPath = [System.IO.Path]::GetTempFileName()
    $originalLocation = Get-Location
    try {
        if (-not [string]::IsNullOrWhiteSpace($WorkingDirectory)) {
            Set-Location -LiteralPath $WorkingDirectory
        }

        & $classifier @Arguments -GitHubOutputPath $outputPath
        $values = @{}
        foreach ($line in Get-Content -LiteralPath $outputPath) {
            $key, $value = $line.Split('=', 2)
            $values[$key] = $value
        }

        $expectedRunWindowsValue = $ExpectedRunWindows.ToString().ToLowerInvariant()
        $expectedMarkdownOnlyValue = $ExpectedMarkdownOnly.ToString().ToLowerInvariant()
        if ($values.run_windows -ne $expectedRunWindowsValue) {
            throw "$Name expected run_windows=$expectedRunWindowsValue, got '$($values.run_windows)'."
        }
        if ($values.markdown_only -ne $expectedMarkdownOnlyValue) {
            throw "$Name expected markdown_only=$expectedMarkdownOnlyValue, got '$($values.markdown_only)'."
        }
        $script:testsRun++
    } finally {
        Set-Location -LiteralPath $originalLocation
        Remove-Item -LiteralPath $outputPath -Force -ErrorAction SilentlyContinue
    }
}

Invoke-Classification `
    -Name 'case-insensitive Markdown paths' `
    -Arguments @{ ChangedPath = @('README.md', 'docs/Architecture.MD') } `
    -ExpectedRunWindows $false `
    -ExpectedMarkdownOnly $true

Invoke-Classification `
    -Name 'mixed Markdown and source paths' `
    -Arguments @{ ChangedPath = @('docs/design.md', 'src/lib.rs') } `
    -ExpectedRunWindows $true `
    -ExpectedMarkdownOnly $false

Invoke-Classification `
    -Name 'empty change set' `
    -Arguments @{ ChangedPath = @() } `
    -ExpectedRunWindows $true `
    -ExpectedMarkdownOnly $false

Invoke-Classification `
    -Name 'forced full run' `
    -Arguments @{ ForceFull = $true } `
    -ExpectedRunWindows $true `
    -ExpectedMarkdownOnly $false

Invoke-Classification `
    -Name 'invalid base commit' `
    -Arguments @{ BaseSha = 'not-a-commit'; HeadSha = '0' * 40 } `
    -ExpectedRunWindows $true `
    -ExpectedMarkdownOnly $false

Invoke-Classification `
    -Name 'missing base commit' `
    -Arguments @{ BaseSha = ''; HeadSha = '0' * 40 } `
    -ExpectedRunWindows $true `
    -ExpectedMarkdownOnly $false

$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$testRepository = Join-Path $temporaryRoot "breeze-ci-classifier-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $testRepository | Out-Null
try {
    Push-Location -LiteralPath $testRepository
    try {
        git init --quiet
        git config user.email 'ci-classifier@example.invalid'
        git config user.name 'CI classifier test'
        New-Item -ItemType Directory -Path 'docs' | Out-Null
        Set-Content -LiteralPath 'README.md' -Value 'base'
        Set-Content -LiteralPath 'docs/delete.md' -Value 'delete me'
        Set-Content -LiteralPath 'docs/old-name.md' -Value 'rename me'
        git add --all
        git commit --quiet -m 'base'
        $baseSha = git rev-parse HEAD

        Remove-Item -LiteralPath 'docs/delete.md'
        git mv 'docs/old-name.md' 'docs/New-Name.MD'
        Set-Content -LiteralPath 'docs/added.md' -Value 'added'
        git add --all
        git commit --quiet -m 'Markdown only'
        $markdownHeadSha = git rev-parse HEAD

        Invoke-Classification `
            -Name 'added, deleted, and renamed Markdown files' `
            -Arguments @{ BaseSha = $baseSha; HeadSha = $markdownHeadSha } `
            -ExpectedRunWindows $false `
            -ExpectedMarkdownOnly $true `
            -WorkingDirectory $testRepository

        New-Item -ItemType Directory -Path 'src' | Out-Null
        Set-Content -LiteralPath 'src/change.rs' -Value '// source change'
        Set-Content -LiteralPath 'docs/added.md' -Value 'updated docs'
        git add --all
        git commit --quiet -m 'Mixed change'
        $mixedHeadSha = git rev-parse HEAD

        Invoke-Classification `
            -Name 'Git diff containing Markdown and source files' `
            -Arguments @{ BaseSha = $markdownHeadSha; HeadSha = $mixedHeadSha } `
            -ExpectedRunWindows $true `
            -ExpectedMarkdownOnly $false `
            -WorkingDirectory $testRepository

        Invoke-Classification `
            -Name 'valid empty Git diff' `
            -Arguments @{ BaseSha = $mixedHeadSha; HeadSha = $mixedHeadSha } `
            -ExpectedRunWindows $true `
            -ExpectedMarkdownOnly $false `
            -WorkingDirectory $testRepository
    } finally {
        Pop-Location
    }
} finally {
    $resolvedTestRepository = [System.IO.Path]::GetFullPath($testRepository)
    if (-not $resolvedTestRepository.StartsWith(
        $temporaryRoot,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "Refusing to remove unexpected test path '$resolvedTestRepository'."
    }
    Remove-Item -LiteralPath $resolvedTestRepository -Recurse -Force
}

Write-Output "CI change-classifier tests passed ($testsRun scenarios)."
