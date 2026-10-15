Set-StrictMode -Version Latest

function Get-CpuTestCost([string] $Name) {
    # Relative scheduling weights from the October baseline, not measured time.
    # Parser/layout cases are much cheaper than a V8 isolate; real deadline and
    # pixel/codec endurance cases must not all land on one count-balanced shard.
    if ($Name -cmatch '^engine::(css|dom|layout)::') { return 0.05 }
    if ($Name -cmatch '^renderer_protocol::') { return 0.1 }
    if ($Name -match '::(propagates_uncatchable_runtime_limits_from_promise_jobs|timed_out_timer_preserves_partial_mutations_and_allows_later_tasks|terminated_microtask_records_samples_even_when_checkpoint_returns_ok)$') { return 100.0 }
    if ($Name -match '::media_recorder::opus::tests::exhaustion::') { return 65.0 }
    if ($Name -match '::tests::canvas_(text_shadow|clipped_solid|shadow_path)::') { return 25.0 }
    if ($Name -match '::ten_minute_stereo_flac_decodes_through_its_final_samples$') { return 40.0 }
    if ($Name -match '::bounded_document_callback_longer_than_old_budget_completes_normally$') { return 25.0 }
    return 1.0
}

function Get-FullLibraryTestPlan {
    param([Parameter(Mandatory)][string[]] $Names, [ValidateRange(1, 3)][int] $CpuWorkers = 3)
    if (-not $Names.Count -or @($Names | Select-Object -Unique).Count -ne $Names.Count) {
        throw 'Library discovery must contain nonempty, unique test names.'
    }
    # Only these existing in-process responsibilities may enter CPU workers.
    # Unknown responsibilities stay in the single native worker. Each CPU
    # process has one libtest thread, so process-wide budgets remain isolated.
    $cpuRoots = @('engine::css::', 'engine::dom::', 'engine::layout::',
        'engine::pattern_eval::', 'engine::script::canvas_host::',
        'engine::script::engine::', 'engine::script::runtime::tests::',
        'engine::script::tests::', 'engine::script::worker_runtime::tests::',
        'engine::script::audio_codecs::', 'engine::script::audio_decode::',
        'engine::script::media_recorder::', 'media_process::backend::opus::',
        'storage::', 'cache_storage::', 'renderer_protocol::')
    $nativeHints = 'webgl|gpu|capture|video|camera|bitmap_renderer|canvas_bitmap|::tests::canvas::|::runtime::tests::canvas_'
    $eligible = @{}
    foreach ($name in $Names) {
        $eligible[$name] = $false
        $module = $name.Substring(0, $name.LastIndexOf('::', [StringComparison]::Ordinal) + 2)
        foreach ($root in $cpuRoots) {
            if ($name.StartsWith($root, [StringComparison]::Ordinal) -and $module -notmatch $nativeHints) {
                $eligible[$name] = $true
                break
            }
        }
    }
    # Filters match substrings in libtest. Reject a parent filter whenever any
    # descendant needs native resources; its direct tests conservatively stay
    # native while independently safe child modules can still run on the CPU.
    $modules = [Collections.Generic.SortedDictionary[string,object]]::new([StringComparer]::Ordinal)
    foreach ($name in $Names) {
        $separator = $name.IndexOf('::', [StringComparison]::Ordinal)
        while ($separator -ge 0) {
            $module = $name.Substring(0, $separator + 2)
            if (-not $modules.ContainsKey($module)) {
                $modules.Add($module, [pscustomobject]@{Names=[Collections.Generic.List[string]]::new();Eligible=$true})
            }
            $modules[$module].Names.Add($name)
            $modules[$module].Eligible = $modules[$module].Eligible -and $eligible[$name]
            $separator = $name.IndexOf('::', $separator + 2, [StringComparison]::Ordinal)
        }
    }
    $prefixes = [Collections.Generic.List[object]]::new()
    foreach ($module in $modules.Keys) {
        if (-not $modules[$module].Eligible) { continue }
        if (@($prefixes | Where-Object { $module.StartsWith($_.Prefix, [StringComparison]::Ordinal) }).Count) { continue }
        # libtest filters are substrings, including inside an unrelated root.
        # Reject that ambiguity before execution rather than relying on the
        # post-run exact-coverage check to discover an unsafe extra selection.
        $ambiguous = $false
        foreach ($name in $Names) {
            if ($name.Contains($module, [StringComparison]::Ordinal) -and
                -not $name.StartsWith($module, [StringComparison]::Ordinal)) { $ambiguous = $true; break }
        }
        if ($ambiguous) { continue }
        $cost = 0.0
        foreach ($name in $modules[$module].Names) { $cost += Get-CpuTestCost $name }
        $prefixes.Add([pscustomobject]@{ Prefix = $module; Names = $modules[$module].Names.ToArray(); Cost = $cost })
    }
    $shards = @(for ($index = 0; $index -lt $CpuWorkers; $index++) {
        [pscustomobject]@{ Name = "cpu-$($index + 1)"; Prefixes = [Collections.Generic.List[string]]::new(); Names = [Collections.Generic.List[string]]::new(); Cost = 0.0 }
    })
    foreach ($prefix in @($prefixes | Sort-Object @{Expression={$_.Cost};Descending=$true}, Prefix)) {
        $shard = $shards | Sort-Object Cost, Name | Select-Object -First 1
        $shard.Prefixes.Add($prefix.Prefix)
        $shard.Cost += $prefix.Cost
        foreach ($name in $prefix.Names) { $shard.Names.Add($name) }
    }
    $cpuPrefixes = @($prefixes | ForEach-Object Prefix)
    $cpuNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($shard in $shards) { foreach ($name in $shard.Names) { $cpuNames.Add($name) | Out-Null } }
    $native = @($Names | Where-Object { -not $cpuNames.Contains($_) })
    $claimed = @($native) + @($shards | ForEach-Object { $_.Names.ToArray() })
    if ($claimed.Count -ne $Names.Count -or @($claimed | Select-Object -Unique).Count -ne $Names.Count -or
        @(Compare-Object ($Names | Sort-Object) ($claimed | Sort-Object) -CaseSensitive).Count) {
        throw 'Test partitions must cover the discovered library exactly once.'
    }
    $exclusive = @($native | Where-Object { $_.StartsWith('winhttp::tests::fetch_pipeline::', [StringComparison]::Ordinal) })
    [pscustomobject]@{ Cpu = $shards; NativeNames = $native; CpuPrefixes = $cpuPrefixes; ExclusiveNames = $exclusive; Total = $Names.Count }
}

function Get-BrowserTestPlan {
    param([Parameter(Mandatory)][string[]] $Names, [ValidateRange(1, 2)][int] $Workers = 2,
        [ValidateSet('live_runtime', 'renderer_process')][string] $Target = 'live_runtime')
    if (-not $Names.Count -or @($Names | Select-Object -Unique).Count -ne $Names.Count) {
        throw 'Browser discovery must contain nonempty, unique test names.'
    }
    # These fixtures use isolated profiles/ports and assert functional results.
    # Unknown, memory, cadence, latency and heavy GPU cases stay exclusive.
    $functionalRoots = @('authored_controls', 'beacon', 'cache_storage', 'deferred_scripts',
        'document_lifecycle', 'dom_results', 'dynamic_modules',
        'event_source', 'file_upload', 'flex_axes', 'history_group', 'history_ordering',
        'history_queue', 'history_redirect', 'history_scroll_restoration', 'history_traversal',
        'indexed_db', 'inline_handlers', 'inserted_scripts', 'keepalive',
        'intersection_observers', 'navigation', 'network', 'parser_notifications',
        'parser_writes', 'parsing', 'prefetch', 'reload', 'resize_observers',
        'stacking', 'storage_manager', 'stylesheets', 'table_scrolling', 'websocket')
    if ($Target -eq 'renderer_process') {
        $functionalRoots = @('accessibility', 'async_scripts', 'checkable', 'color_paint',
            'crypto', 'csp_ordering', 'drag_drop', 'event_notifications', 'form_review',
            'form_validation', 'form_validation_presentation', 'geolocation', 'glyph_resources',
            'history_traversal', 'hover', 'input', 'notifications', 'pointer_buttons',
            'pointer_lock', 'resize_observers', 'runtime_reporting', 'sensors', 'state',
            'storage_values', 'streaming_fetch', 'streaming_navigation', 'title_navigation',
            'user_agent', 'viewport_observers', 'xhr_reuse')
    }
    $prefix = if ($Target -eq 'renderer_process') { 'renderer' } else { 'browser' }
    $shards = @(for ($index = 0; $index -lt $Workers; $index++) {
        [pscustomobject]@{ Name = "$prefix-functional-$($index + 1)"; Names = [Collections.Generic.List[string]]::new(); Cost = 0.0 }
    })
    $exclusive = [Collections.Generic.List[string]]::new()
    $functional = [Collections.Generic.List[object]]::new()
    foreach ($name in $Names) {
        $root = ($name -split '::', 2)[0]
        if ($root -cin $functionalRoots -and $name -notmatch 'latency|peak_memory|working_set|cadence|performance|benchmark|webgl|gpu|canvas|capture|video') {
            # Weights reflect retained fixture delays, not measured durations.
            $cost = 1.0
            if ($Target -eq 'live_runtime') {
                if ($name -match '^network::slow_worker_') { $cost = 5.0 }
                elseif ($name -match '^document_lifecycle::document_lifecycle_') { $cost = 6.0 }
                elseif ($name -match '^deferred_scripts::deferred_scripts_match_') { $cost = 5.0 }
                elseif ($name -match '^websocket::') { $cost = 3.0 }
                elseif ($name -match '^(inline_handlers|prefetch|stylesheets)::') { $cost = 2.0 }
                elseif ($name -match '^(keepalive|file_upload)::') { $cost = 2.2 }
                elseif ($name -match '^(parser_writes|parser_notifications)::|^document_lifecycle::streams::') { $cost = 0.6 }
            }
            $functional.Add([pscustomobject]@{Name=$name;Cost=$cost})
        } else { $exclusive.Add($name) }
    }
    foreach ($test in @($functional | Sort-Object @{Expression={$_.Cost};Descending=$true}, Name)) {
        $shard = $shards | Sort-Object Cost, Name | Select-Object -First 1
        $shard.Names.Add($test.Name)
        $shard.Cost += $test.Cost
    }
    # Serial native functionality can overlap guarded CPU work. Actual timing,
    # cadence, process-memory and unknown cases still wait for all CPU workers.
    $earlyNative = @()
    if ($Target -eq 'renderer_process') {
        $nativeRoots = @('audio_codecs', 'backpressure', 'canvas_frames', 'canvas_presentation',
            'capture', 'clock_backpressure', 'media', 'media_devices', 'state_backpressure',
            'storage_sync', 'video_codecs', 'webgl')
        $earlyNative = @($exclusive | Where-Object {
            ($_ -split '::', 2)[0] -cin $nativeRoots -and $_ -notmatch 'cadence|latency|peak_memory|working_set|performance|benchmark'
        })
    }
    [pscustomobject]@{ Functional = $shards; Exclusive = $exclusive.ToArray(); EarlyNative = $earlyNative; Total = $Names.Count }
}
