# Preserve notices carried by the pinned native graphics package, not just Cargo's wrapper.
# These helpers are dot-sourced by package-technical-alpha.ps1 after Copy-ReleaseText exists.
function Copy-NativeGraphicsNotices {
    param([Parameter(Mandatory)][string] $PackageName,
        [Parameter(Mandatory)][string] $SourceDirectory,
        [Parameter(Mandatory)][string] $Destination,
        [Parameter(Mandatory)][string] $RepositoryRoot)
    if ($PackageName -eq 'libz-sys') {
        $notice = Join-Path $SourceDirectory 'src/zlib/LICENSE'
        if (-not (Test-Path -LiteralPath $notice -PathType Leaf)) {
            throw 'Pinned libz-sys stock-zlib notice is missing; review native dependency provenance.'
        }
        Copy-ReleaseText $notice (Join-Path $Destination 'LICENSE.native-zlib')
        return
    }
    if ($PackageName -ne 'mozangle') { return }
    $upstream = Join-Path $SourceDirectory 'UPSTREAM'
    $expected = 'gfx/angle is taken from FIREFOX_153_3_0esr_RELEASE: 861fdeb0d32fe1bd101fea886687e680f612d735'
    if (-not (Test-Path -LiteralPath $upstream -PathType Leaf) -or
        [IO.File]::ReadAllText($upstream).Trim() -ne $expected) {
        throw 'ANGLE upstream provenance changed; review licensing before packaging.'
    }
    Copy-ReleaseText $upstream (Join-Path $Destination 'UPSTREAM')
    $nativeRoot = Join-Path $SourceDirectory 'gfx/angle/checkout'
    Copy-ReleaseText (Join-Path $nativeRoot 'LICENSE') (Join-Path $Destination 'LICENSE.native-angle')
    # Chromium-derived files reference its BSD license. Retain the full license together with
    # each original copyright header below; never substitute a Cargo SPDX expression.
    Copy-ReleaseText (Join-Path $RepositoryRoot 'third_party/accesskit/LICENSE.chromium') `
        (Join-Path $Destination 'LICENSE.chromium')
    $groups = @('src/common/third_party', 'third_party/zlib/google', 'include/KHR',
        'include/EGL', 'include/GLES2', 'src/third_party/trace_event', 'src/third_party/systeminfo')
    foreach ($group in $groups) {
        $directory = Join-Path $nativeRoot $group
        if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
            throw "ANGLE notice source disappeared: $group"
        }
        foreach ($file in @(Get-ChildItem -LiteralPath $directory -File -Recurse | Sort-Object FullName)) {
            if ($file.Extension -notin @('.h', '.c', '.cc', '.cpp')) { continue }
            # Some upstream licenses occur after include guards rather than in the first
            # comment. Preserve complete original text instead of guessing license boundaries.
            $relative = $file.FullName.Substring($nativeRoot.Length + 1)
            $target = Join-Path (Join-Path $Destination 'native-source-notices') ($relative + '.txt')
            [IO.Directory]::CreateDirectory((Split-Path -Parent $target)) | Out-Null
            Copy-ReleaseText $file.FullName $target
        }
    }
}
