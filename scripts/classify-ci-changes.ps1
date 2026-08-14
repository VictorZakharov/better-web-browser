[CmdletBinding()]
param(
    [AllowEmptyString()]
    [string] $BaseSha,

    [AllowEmptyString()]
    [string] $HeadSha,

    [AllowEmptyCollection()]
    [string[]] $ChangedPath,

    [switch] $ForceFull,

    [string] $GitHubOutputPath
)

$ErrorActionPreference = 'Stop'

function Assert-CommitSha {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string] $Commit,

        [Parameter(Mandatory = $true)]
        [string] $Name
    )

    if ($Commit -notmatch '\A[0-9a-fA-F]{40,64}\z') {
        throw "$Name is not a full Git commit ID."
    }

    git cat-file -e "$Commit`^{commit}" 2>$null
    if ($LASTEXITCODE -ne 0) {
        throw "$Name '$Commit' is unavailable. Ensure checkout uses fetch-depth: 0."
    }
}

function Get-PullRequestChangedPath {
    Assert-CommitSha -Commit $BaseSha -Name 'BaseSha'
    Assert-CommitSha -Commit $HeadSha -Name 'HeadSha'

    $range = "$BaseSha...$HeadSha"
    $paths = @(git diff --name-only --no-renames --diff-filter=ACDMRTUXB $range --)
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to inspect pull-request changes between '$BaseSha' and '$HeadSha'."
    }
    return $paths
}

function Write-ClassificationOutput {
    param(
        [Parameter(Mandatory = $true)]
        [bool] $RunWindows,

        [Parameter(Mandatory = $true)]
        [bool] $MarkdownOnly
    )

    if ([string]::IsNullOrWhiteSpace($GitHubOutputPath)) {
        return
    }

    $runWindowsValue = $RunWindows.ToString().ToLowerInvariant()
    $markdownOnlyValue = $MarkdownOnly.ToString().ToLowerInvariant()
    $content = "run_windows=$runWindowsValue`nmarkdown_only=$markdownOnlyValue`n"
    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::AppendAllText($GitHubOutputPath, $content, $encoding)
}

$markdownOnly = $false
try {
    if ($ForceFull) {
        Write-Host 'A full Windows CI run is required for this event.'
    } else {
        $paths = if ($PSBoundParameters.ContainsKey('ChangedPath')) {
            @($ChangedPath)
        } else {
            @(Get-PullRequestChangedPath)
        }

        if ($paths.Count -eq 0) {
            throw 'The pull-request diff contains no changed paths.'
        }

        $nonMarkdownPaths = @(
            $paths | Where-Object {
                -not $_.EndsWith('.md', [System.StringComparison]::OrdinalIgnoreCase)
            }
        )
        $markdownOnly = $nonMarkdownPaths.Count -eq 0

        Write-Host "Changed paths ($($paths.Count)):"
        $paths | ForEach-Object { Write-Host "  $_" }
        if ($markdownOnly) {
            Write-Host 'Every changed path is Markdown; Windows workers may be skipped.'
        } else {
            Write-Host 'At least one changed path is not Markdown; full Windows CI is required.'
        }
    }
} catch {
    $markdownOnly = $false
    Write-Warning "Change classification was inconclusive: $($_.Exception.Message) Full Windows CI is required."
} finally {
    # GitHub's PowerShell wrapper observes LASTEXITCODE after this script returns. A handled Git
    # probe failure must select full CI without accidentally failing the classifier job.
    $global:LASTEXITCODE = 0
}

Write-ClassificationOutput -RunWindows (-not $markdownOnly) -MarkdownOnly $markdownOnly
