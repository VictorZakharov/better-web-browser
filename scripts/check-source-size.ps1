[CmdletBinding()]
param(
    [ValidateRange(1, 10000)]
    [int] $TargetLines = 400,

    [ValidateRange(1, 10000)]
    [int] $MaximumLines = 500
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

# Files above the target are technical debt, not precedent. These ceilings freeze each reduced
# baseline and must only move downward as modules split. Small coordinator ceilings stay frozen too.
$fileCeilings = @{
    'src/document.rs'                       = 518
    'src/engine/css.rs'                     = 2856
    'src/engine/dom.rs'                     = 986
    'src/engine/layout.rs'                  = 3405
    'src/engine/page.rs'                    = 358
    'src/engine/scheduler.rs'               = 518
    'src/engine/script.rs'                  = 80
    'src/engine/script/execution.rs'        = 453
    'src/engine/script/host_call.rs'        = 422
    'src/windows_app.rs'                    = 191
    'src/winhttp.rs'                        = 760
}

$sourceRoots = @('src', 'tests', 'benchmarks', 'scripts')
$extensions = @('.rs', '.js', '.cs', '.ps1')
$violations = [System.Collections.Generic.List[string]]::new()
$warnings = [System.Collections.Generic.List[string]]::new()

foreach ($sourceRoot in $sourceRoots) {
    $absoluteRoot = Join-Path $repoRoot $sourceRoot
    if (-not (Test-Path -LiteralPath $absoluteRoot)) {
        continue
    }
    foreach ($file in Get-ChildItem -LiteralPath $absoluteRoot -Recurse -File) {
        if ($file.Extension -notin $extensions) {
            continue
        }
        $relativePath = $file.FullName.Substring($repoRoot.Length)
        $relativePath = $relativePath.TrimStart([char[]]@('\', '/')).Replace('\', '/')
        $lineCount = @(Get-Content -LiteralPath $file.FullName).Count
        $ceiling = if ($fileCeilings.ContainsKey($relativePath)) {
            $fileCeilings[$relativePath]
        } else {
            $MaximumLines
        }

        if ($lineCount -gt $ceiling) {
            $violations.Add("$relativePath has $lineCount lines; ceiling is $ceiling")
        } elseif ($lineCount -gt $TargetLines) {
            $warnings.Add("$relativePath has $lineCount lines; target is $TargetLines")
        }
    }
}

foreach ($warning in $warnings) {
    Write-Warning $warning
}
if ($violations.Count -gt 0) {
    $violations | ForEach-Object { Write-Error $_ }
    exit 1
}

Write-Output "Source-size check passed ($($warnings.Count) file(s) above the target, no ceiling violations)."
