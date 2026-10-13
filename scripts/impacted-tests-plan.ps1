# Responsibility ownership and downstream contracts, not a proof of non-impact.
# First matching owner wins; shared coordinators and unknown files stay broad.
function Get-TestImpactCatalog {
    $images=@('image-regions','image-rows','image-bindings','video-sources','owned-uploads','queued-uploads','float-images','image-api')
    $groups = [ordered]@{
        'canvas-lib' = @{ Target='lib'; Filter='canvas'; Covers=@() }
        'image-regions' = @{ Target='lib'; Filter='webgl_canvas'; Covers=@() }
        'image-rows' = @{ Target='lib'; Filter='webgl_image'; Covers=@() }
        'image-bindings' = @{ Target='lib'; Filter='webgl2_image'; Covers=@() }
        'video-sources' = @{ Target='lib'; Filter='webgl_video_source_tests'; Covers=@() }
        'owned-uploads' = @{ Target='lib'; Filter='webgl_owned'; Covers=@() }
        'queued-uploads' = @{ Target='lib'; Filter='webgl_queued_upload_tests'; Covers=@() }
        'float-images' = @{ Target='lib'; Filter='webgl_float_textures'; Covers=@() }
        'image-api' = @{ Target='lib'; Filter='engine::script::tests::webgl::'; Covers=@() }
        'webgl-lib' = @{ Target='lib'; Filter='webgl'; Covers=$images }
        'script-lib' = @{ Target='lib'; Filter='engine::script::'; Covers=@('canvas-lib','webgl-lib')+$images }
        'canvas-renderer' = @{ Target='renderer_process'; Filter='canvas'; Covers=@() }
        'webgl-renderer' = @{ Target='renderer_process'; Filter='webgl'; Covers=@() }
        'renderer' = @{ Target='renderer_process'; Filter=''; Covers=@('canvas-renderer','webgl-renderer') }
        'live-runtime' = @{ Target='live_runtime'; Filter=''; Covers=@() }
        'selector' = @{ Target='selector'; Filter=''; Covers=@() }
    }
    $owners = @(
        @{ Id='canvas'; Paths=@('src/engine/script/bootstrap/canvas*.js','src/engine/script/bootstrap/offscreen*.js',
            'src/engine/script/canvas_host.rs','src/engine/script/canvas_host/*',
            'src/engine/script/engine/bridge/canvas_inspect.rs',
            'src/engine/script/engine/runtime/*canvas*tests.rs','src/engine/script/tests/canvas*',
            'tests/canvas/*','benchmarks/ui/fixtures/canvas-label-atlas.html',
            'benchmarks/ui/fixtures/canvas-frame-publication.html');
            Groups=@('canvas-lib','canvas-renderer'); Consumers=@('webgl-images') }
        @{ Id='webgl-images'; Paths=@('src/engine/script/bootstrap/webgl2_image*.js',
            'src/engine/script/bootstrap/webgl_video_sources.js',
            'src/engine/script/engine/runtime/webgl_image*tests.rs',
            'src/engine/script/engine/runtime/webgl2_image*tests.rs',
            'src/engine/script/engine/runtime/webgl_video_source_tests.rs');
            Groups=$images+@('webgl-renderer'); Consumers=@() }
        @{ Id='webgl'; Paths=@('src/engine/webgl.rs','src/engine/webgl/*','src/engine/script/bootstrap/webgl*.js',
            'src/engine/script/engine/runtime/*webgl*tests.rs','src/engine/script/tests/webgl*',
            'src/engine/script/engine/bridge/gpu*','tests/renderer_process/webgl.rs',
            'tests/renderer_process/canvas_presentation/webgl.rs');
            Groups=@('webgl-lib','webgl-renderer'); Consumers=@() }
        @{ Id='script'; Paths=@('src/engine/script.rs','src/engine/script/*');
            Groups=@('script-lib','renderer','live-runtime'); Consumers=@() }
        @{ Id='selector'; Paths=@('scripts/impacted-tests-plan.ps1','scripts/run-impacted-tests.ps1',
            'scripts/test-impacted-tests.ps1'); Groups=@('selector'); Consumers=@() }
    )
    return @{ Groups=$groups; Owners=$owners }
}

function Get-ImpactedTestPlan {
    param([AllowEmptyCollection()][string[]] $ChangedPath, [switch] $Final)
    $catalog = Get-TestImpactCatalog
    $pending = [Collections.Generic.Queue[string]]::new()
    $selected = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $reasons = [Collections.Generic.List[string]]::new()
    $unknown = [Collections.Generic.List[string]]::new()
    $owners = @{}
    foreach ($owner in $catalog.Owners) { $owners.Add($owner.Id,$owner) }
    foreach ($raw in $ChangedPath) {
        $path = if ($null -eq $raw) { '' } else { $raw.Replace('\','/') }
        if ([string]::IsNullOrWhiteSpace($path) -or $path -match '[\x00-\x1f]' -or
            $path -match '(^/|^[A-Za-z]:|(^|/)\.\.(/|$))') {
            $unknown.Add("Invalid changed path: $raw"); continue
        }
        if ($path.EndsWith('.md',[StringComparison]::OrdinalIgnoreCase)) { continue }
        $matched = $null
        foreach ($owner in $catalog.Owners) {
            if (@($owner.Paths | Where-Object { $path -like $_ }).Count) { $matched=$owner; break }
        }
        if ($null -eq $matched) { $unknown.Add($path); continue }
        $pending.Enqueue($matched.Id)
        $reasons.Add("$path -> $($matched.Id)")
    }
    while ($pending.Count) {
        $id = $pending.Dequeue()
        if (-not $owners.ContainsKey($id)) { throw "Unknown test-impact consumer: $id" }
        if (-not $selected.Add($id)) { continue }
        foreach ($consumer in $owners[$id].Consumers) {
            $reasons.Add("$id -> downstream $consumer")
            $pending.Enqueue($consumer)
        }
    }
    $ids = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($id in $selected) {
        foreach ($group in $owners[$id].Groups) {
            if (-not $catalog.Groups.Contains($group)) { throw "Unknown test-impact group: $group" }
            [void]$ids.Add($group)
        }
    }
    foreach ($id in @($ids)) {
        foreach ($covered in $catalog.Groups[$id].Covers) { [void]$ids.Remove($covered) }
    }
    $full = $Final.IsPresent -or $unknown.Count -gt 0
    $commands = if ($full) {
        @(@{ Id='full'; Target='all'; Filter='' },@{ Id='selector'; Target='selector'; Filter='' })
    } else {
        @($catalog.Groups.Keys | Where-Object { $ids.Contains($_) } | ForEach-Object {
            @{ Id=$_; Target=$catalog.Groups[$_].Target; Filter=$catalog.Groups[$_].Filter }
        })
    }
    return [pscustomobject]@{ Full=$full; Commands=$commands; Reasons=@($reasons); Unknown=@($unknown) }
}

function Get-TestImpactChangedPaths {
    param([string] $Base, [string] $RepoRoot)
    Push-Location -LiteralPath $RepoRoot
    try {
        $sha = & git rev-parse --verify --end-of-options "$Base`^{commit}"
        if ($LASTEXITCODE -ne 0 -or $sha -notmatch '\A[0-9a-f]{40,64}\z') { throw 'Invalid test-impact base.' }
        # Include deletions and both sides of renames, plus new untracked files.
        # NUL separators preserve spaces; unusual control-character paths fail broad.
        $tracked = @(& git diff --name-only --no-renames -z $sha --)
        if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect tracked changes.' }
        $untracked = @(& git ls-files --others --exclude-standard -z --)
        if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect untracked changes.' }
        if ($tracked.Count -gt 1 -or $untracked.Count -gt 1) {
            throw 'Native Git output contains unexpected line breaks; cannot narrow tests safely.'
        }
        $stream = (@($tracked) + @($untracked)) -join ''
        return @($stream -split "`0" | Where-Object { $_ -ne '' })
    } finally { Pop-Location }
}
