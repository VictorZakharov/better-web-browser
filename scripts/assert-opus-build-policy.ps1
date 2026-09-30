# Pure readers: never execute a command, project, or path obtained from Cargo/CMake output.
# Cargo emits build-script-executed even for a fresh cached build:
# https://doc.rust-lang.org/cargo/reference/external-tools.html#build-script-output
function Read-OpusPolicyFile {
    param([string] $Path)
    $file = Get-Item -LiteralPath $Path -ErrorAction Stop
    if ($file.PSIsContainer -or $file.Length -eq 0 -or $file.Length -gt 8MB) {
        throw "Missing, empty, or oversized Opus policy input: $Path"
    }
    return [IO.File]::ReadAllText($file.FullName)
}

function Get-OpusFullPath {
    param([string] $Path)
    if ([string]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathFullyQualified($Path)) {
        throw "Opus artifact path must be absolute: $Path"
    }
    return [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
}

function Get-OpusCargoOutput {
    param([string[]] $Lines, [string] $TargetDirectory, [ValidateSet('debug', 'release')] [string] $Profile)
    $messages = @(); $size = 0
    foreach ($line in $Lines) {
        $size += $line.Length
        if ($line.Length -gt 1MB -or $size -gt 16MB) { throw 'Cargo JSON output exceeds policy bounds.' }
        if ($line.StartsWith('{')) { $messages += ($line | ConvertFrom-Json -ErrorAction Stop) }
    }
    $finished = @($messages | Where-Object reason -eq 'build-finished')
    if ($finished.Count -ne 1 -or $finished[0].success -ne $true) { throw 'Cargo did not report one successful build.' }
    # Fail closed when the pinned dependency changes; update this gate alongside its provenance review.
    $package = 'registry+https://github.com/rust-lang/crates.io-index#opusic-sys@0.7.5'
    $scripts = @($messages | Where-Object { $_.reason -eq 'build-script-executed' -and $_.package_id -eq $package })
    $artifacts = @($messages | Where-Object {
        $_.reason -eq 'compiler-artifact' -and $_.package_id -eq $package -and $_.target.name -eq 'opusic_sys' -and
        'lib' -in $_.target.kind
    })
    if ($scripts.Count -ne 1 -or $artifacts.Count -ne 1) { throw 'Cargo must identify exactly one pinned Opus build and library.' }
    $artifact = $artifacts[0]
    if ('bundled' -notin $artifact.features -or @($artifact.features | Where-Object {
        $_ -in @('no-hardening', 'no-stack-protector', 'no-runtime-feature-detection', 'no-simd')
    }).Count -ne 0 -or $artifact.profile.test -ne $false -or
        $artifact.profile.debug_assertions -ne ($Profile -eq 'debug')) { throw 'Unexpected Opus features or Cargo profile.' }
    $out = Get-OpusFullPath $scripts[0].out_dir
    $target = Get-OpusFullPath $TargetDirectory
    $prefix = (Join-Path $target "$Profile/build") + [IO.Path]::DirectorySeparatorChar
    if (-not $out.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or
        $out -notmatch '[\\/]opusic-sys-[a-f0-9]+[\\/]out$') { throw "Unexpected exact Opus out_dir: $out" }
    if (@($scripts[0].linked_libs).Count -ne 1 -or $scripts[0].linked_libs[0] -ne 'static=opus' -or
        @($scripts[0].linked_paths).Count -ne 1 -or
        $scripts[0].linked_paths[0] -ne ('native=' + (Join-Path $out 'lib'))) { throw 'Opus link metadata does not match its exact out_dir.' }
    return $out
}

function Assert-OpusCompilePolicy {
    param([string[]] $Definitions, [string] $Options, [string] $Source, [bool] $Protected)
    $macros = @{}
    foreach ($definition in $Definitions) {
        $definition = $definition.Trim()
        if ($definition -match '^([A-Za-z_][A-Za-z_0-9]*)(?:=(.*))?$') {
            if ($macros.ContainsKey($Matches[1])) { throw "Duplicate Opus macro: $definition" }
            $macros[$Matches[1]] = [string] $Matches[2]
        } elseif ($definition -and $definition -ne '%(PreprocessorDefinitions)') { throw "Unrecognized Opus definition: $definition" }
    }
    foreach ($required in @('OPUS_BUILD', 'ENABLE_HARDENING', 'USE_ALLOCA', 'OPUS_HAVE_RTCD')) {
        if (-not $macros.ContainsKey($required) -or $macros[$required] -notin @('', '1')) { throw "Missing actual Opus macro: $required ($Source)" }
    }
    foreach ($unsafe in @('NONTHREADSAFE_PSEUDOSTACK', 'OPUS_X86_PRESUME_AVX2', 'OPUS_X86_PRESUME_SSE4_1')) {
        if ($macros.ContainsKey($unsafe)) { throw "Unsafe Opus macro: $unsafe ($Source)" }
    }
    if (-not $Protected -or $Options -match '(?i)(?:^|\s)"?[/-]GS-' -or
        $Options -match '(?i)(?:^|\s)"?[/-][DU]\s*"?(?:ENABLE_HARDENING|USE_ALLOCA|OPUS_HAVE_RTCD|NONTHREADSAFE_PSEUDOSTACK|OPUS_X86_PRESUME_(?:AVX2|SSE4_1))') {
        throw "Disabled or overridden Opus hardening: $Source"
    }
    # Upstream builds dispatch-selected AVX2 translation units; these are not baseline AVX2.
    # opus/CMakeLists.txt sets the flag only on the *_avx.c / *_avx2.c source groups.
    if ($Options -match '(?i)(?:[/-]arch:AVX\S*|-mavx\S*|-mfma)' -and $Source -notmatch '(?i)_avx2?\.c$') {
        throw "Non-baseline Opus CPU option on $Source"
    }
    if ($Options -match '\$|%\(|@[^\s]+') { throw "Unresolved Opus compile options: $Source" }
}

function Test-OpusProjectCondition {
    param([string] $Condition, [string] $Configuration)
    if (-not $Condition) { return $true }
    if ($Condition -notmatch "^'\`$\(Configuration\)\|\`$\(Platform\)'\s*==\s*'(Debug|Release|MinSizeRel|RelWithDebInfo)\|x64'\s*$") {
        throw "Unrecognized Opus MSBuild condition: $Condition"
    }
    return $Matches[1] -eq $Configuration
}

function Get-OpusProjectSettings {
    param([Xml.XmlElement] $Element, [hashtable] $Inherited, [string] $Configuration)
    $settings = @{} + $Inherited
    foreach ($child in $Element.ChildNodes) {
        if ($child -isnot [Xml.XmlElement] -or -not (Test-OpusProjectCondition $child.GetAttribute('Condition') $Configuration)) { continue }
        if ($child.LocalName -in @('PreprocessorDefinitions', 'AdditionalOptions', 'BufferSecurityCheck', 'EnableEnhancedInstructionSet')) {
            $value = $child.InnerText
            $value = $value.Replace('%(' + $child.LocalName + ')', [string] $Inherited[$child.LocalName])
            $settings[$child.LocalName] = $value
        }
    }
    return $settings
}

function Assert-OpusVisualStudio {
    param([string] $BuildDirectory, [string] $Configuration)
    $text = Read-OpusPolicyFile (Join-Path $BuildDirectory 'opus.vcxproj')
    $readerSettings = [Xml.XmlReaderSettings]::new()
    $readerSettings.DtdProcessing = [Xml.DtdProcessing]::Prohibit
    $readerSettings.XmlResolver = $null
    $readerSettings.MaxCharactersInDocument = 8MB
    $reader = [Xml.XmlReader]::Create([IO.StringReader]::new($text), $readerSettings)
    try { $project = [Xml.XmlDocument]::new(); $project.XmlResolver = $null; $project.Load($reader) } finally { $reader.Dispose() }
    $ns = [Xml.XmlNamespaceManager]::new($project.NameTable)
    $ns.AddNamespace('m', 'http://schemas.microsoft.com/developer/msbuild/2003')
    $global = @($project.SelectNodes('/m:Project/m:ItemDefinitionGroup', $ns) | Where-Object {
        Test-OpusProjectCondition $_.GetAttribute('Condition') $Configuration
    })
    if ($global.Count -ne 1) { throw 'Expected one active Opus MSVC compile configuration.' }
    $compile = $global[0].SelectSingleNode('m:ClCompile', $ns)
    if ($null -eq $compile) { throw 'Missing Opus ClCompile policy.' }
    $settings = Get-OpusProjectSettings $compile @{} $Configuration
    Assert-OpusCompilePolicy ($settings.PreprocessorDefinitions -split ';') $settings.AdditionalOptions '<target>' ($settings.BufferSecurityCheck -eq 'true')
    if ($settings.EnableEnhancedInstructionSet -and $settings.EnableEnhancedInstructionSet -notin @('StreamingSIMDExtensions', 'StreamingSIMDExtensions2')) {
        throw 'Opus target-wide instruction-set override is not baseline x64.'
    }
    $sources = @($project.SelectNodes('/m:Project/m:ItemGroup/m:ClCompile', $ns))
    $activeSources = @()
    foreach ($source in $sources) {
        if (-not (Test-OpusProjectCondition $source.ParentNode.GetAttribute('Condition') $Configuration) -or
            -not (Test-OpusProjectCondition $source.GetAttribute('Condition') $Configuration)) { continue }
        $name = $source.GetAttribute('Include')
        if ($name -notmatch '(?i)\.c$') { throw "Unknown Opus native source: $name" }
        foreach ($excluded in $source.SelectNodes('m:ExcludedFromBuild', $ns)) {
            if ((Test-OpusProjectCondition $excluded.GetAttribute('Condition') $Configuration) -and $excluded.InnerText -ne 'false') {
                throw "Excluded or unknown Opus source compile policy: $name"
            }
        }
        $activeSources += $name
        $local = Get-OpusProjectSettings $source $settings $Configuration
        $options = $local.AdditionalOptions
        if ($local.EnableEnhancedInstructionSet -and $local.EnableEnhancedInstructionSet -notin @('StreamingSIMDExtensions', 'StreamingSIMDExtensions2')) {
            if ($local.EnableEnhancedInstructionSet -ne 'AdvancedVectorExtensions2') { throw "Unknown Opus instruction set: $name" }
            $options += ' /arch:AVX2'
        }
        Assert-OpusCompilePolicy ($local.PreprocessorDefinitions -split ';') $options $name ($local.BufferSecurityCheck -eq 'true')
    }
    if ($activeSources.Count -lt 3) { throw 'Opus has no complete active native source compile list.' }
    foreach ($core in @('opus_decoder.c', 'opus_encoder.c', 'bands.c')) {
        if (-not @($activeSources | Where-Object { $_ -match ('[\\/]' + [regex]::Escape($core) + '$') }).Count) { throw "Missing Opus core source: $core" }
    }
    return (Join-Path $BuildDirectory "$Configuration/opus.lib")
}

function Assert-OpusNinja {
    param([string] $BuildDirectory)
    $ninja = Read-OpusPolicyFile (Join-Path $BuildDirectory 'build.ninja')
    $rules = Read-OpusPolicyFile (Join-Path $BuildDirectory 'CMakeFiles/rules.ninja')
    # Only these two CMake-owned files supply rule/global defaults. An unexamined
    # include could otherwise define an apparently absent optional launcher.
    $includes = [regex]::Matches($ninja, '(?m)^\s*(?:include|subninja)\s+([^\r\n]+)')
    if ($includes.Count -ne 1 -or $includes[0].Value.Trim() -ne 'include CMakeFiles/rules.ninja' -or
        $rules -match '(?m)^\s*(?:include|subninja)\s+') { throw 'Unknown Opus Ninja include/default ownership.' }
    foreach ($optional in [regex]::Matches($ninja + "`n" + $rules, '(?m)^\s*(LAUNCHER|CODE_CHECK)\s*=([^\r\n]*)')) {
        if ($optional.Groups[2].Value.Trim()) { throw "Nonempty or unevaluable Opus Ninja optional command: $($optional.Groups[1].Value)" }
    }
    # CMake emits concrete per-edge DEFINES/FLAGS; do not evaluate Ninja commands or expand arbitrary variables.
    $blocks = [regex]::Matches($ninja, '(?m)^build CMakeFiles/opus\.dir/[^\r\n]+\.obj:[^\r\n]*(?:\r?\n[ \t]+[^\r\n]*)*')
    if ($blocks.Count -lt 3) { throw 'Ninja has no complete Opus C compile edge list.' }
    $sources = @()
    foreach ($block in $blocks) {
        if ($block.Value -notmatch '^build CMakeFiles/opus\.dir/(.+\.c)\.obj:\s+(C_COMPILER__opus_\w+)\s+[^\r\n]+') { throw 'Unknown Opus Ninja compile edge.' }
        $source = $Matches[1]; $rule = $Matches[2]; $sources += $source
        $ruleBlocks = [regex]::Matches($rules, '(?m)^rule ' + [regex]::Escape($rule) + '\r?\n(?:[ \t]+[^\r\n]*\r?\n)*')
        if ($ruleBlocks.Count -ne 1 -or $ruleBlocks[0].Value -notmatch '(?m)^\s+command = (.+)$') { throw "Missing exact Opus Ninja rule: $rule" }
        $command = $Matches[1]
        if ($command -notmatch '(?i)(?:^|[\\/])cl\.exe(?:"|\s)' -or
            $command -match '(?i)(?:[/-]GS-|[/-]arch:AVX|[/-][DU]\s*"?(?:ENABLE_HARDENING|USE_ALLOCA|OPUS_HAVE_RTCD|NONTHREADSAFE_PSEUDOSTACK|OPUS_X86_PRESUME_(?:AVX2|SSE4_1)))') {
            throw "Unknown or unsafe MSVC Ninja rule: $rule"
        }
        $variablePattern = '\$(?:\{([A-Za-z_][A-Za-z_0-9]*)\}|([A-Za-z_][A-Za-z_0-9]*))'
        $variables = @()
        foreach ($variable in [regex]::Matches($command, $variablePattern)) {
            $name = if ($variable.Groups[1].Success) { $variable.Groups[1].Value } else { $variable.Groups[2].Value }
            if ($name -cnotin @('DEFINES', 'INCLUDES', 'FLAGS', 'in', 'out', 'TARGET_PDB', 'TARGET_COMPILE_PDB', 'DEP_FILE', 'LAUNCHER', 'CODE_CHECK')) {
                throw "Unrecognized Opus Ninja command variable: $($variable.Value)"
            }
            $variables += $name
        }
        if ('DEFINES' -cnotin $variables -or 'FLAGS' -cnotin $variables -or
            [regex]::Replace($command, $variablePattern, '') -match '\$') { throw 'Missing or unevaluable Opus Ninja command expansion.' }
        $defs = [regex]::Matches($block.Value, '(?m)^\s+DEFINES = (.*)$')
        $flags = [regex]::Matches($block.Value, '(?m)^\s+FLAGS = (.*)$')
        if ($defs.Count -ne 1 -or $flags.Count -ne 1) { throw "Missing concrete Opus Ninja flags: $source" }
        $definitionText = $defs[0].Groups[1].Value.Trim()
        if ($definitionText -match '\$' -or $definitionText -notmatch '^(?:-D\S+(?:\s+|$))+$') { throw "Unknown Opus Ninja definitions: $source" }
        $definitions = @($definitionText -split '\s+' | ForEach-Object { $_.Substring(2) })
        $options = $flags[0].Groups[1].Value
        Assert-OpusCompilePolicy $definitions $options $source ($options -match '(?i)(?:^|\s)[/-]GS(?:\s|$)')
    }
    foreach ($core in @('opus_decoder.c', 'opus_encoder.c', 'bands.c')) {
        if (-not @($sources | Where-Object { $_ -match ('(?:^|/)' + [regex]::Escape($core) + '$') }).Count) { throw "Missing Opus core Ninja source: $core" }
    }
    return (Join-Path $BuildDirectory 'opus.lib')
}

function Get-OpusVisualStudioConfiguration {
    param([string] $OutDirectory)
    # cmake 0.1.58 passes get_profile() to --config independently of an explicit
    # CMAKE_BUILD_TYPE. Multi-config MSVC must use the Cargo-owned install transcript,
    # including cached (fresh=true) Cargo builds, not the cache or directory timestamps.
    $transcript = Read-OpusPolicyFile (Join-Path (Split-Path $OutDirectory) 'output')
    $records = [regex]::Matches($transcript, '(?m)^\s*-- Install configuration:[^\r\n]*')
    $configurations = @()
    foreach ($record in $records) {
        if ($record.Value.Trim() -notmatch '^-- Install configuration: "(Debug|Release|MinSizeRel|RelWithDebInfo)"$') {
            throw 'Unknown Opus Visual Studio install configuration.'
        }
        $configurations += $Matches[1]
    }
    $configurations = @($configurations | Select-Object -Unique)
    if ($configurations.Count -ne 1) { throw 'Missing or conflicting Opus install configurations in exact Cargo transcript.' }
    $roots = [regex]::Matches($transcript, '(?m)^cargo:root=([^\r\n]+)')
    if ($roots.Count -ne 1 -or (Get-OpusFullPath $roots[0].Groups[1].Value) -ne $OutDirectory) {
        throw 'Opus build transcript does not belong to the exact Cargo out_dir.'
    }
    $installed = Join-Path $OutDirectory 'lib/opus.lib'
    $installs = [regex]::Matches($transcript, '(?m)^\s*-- (?:Installing|Up-to-date): ([^\r\n]*[\\/]opus\.lib)\s*$')
    if ($installs.Count -lt 1) { throw 'Opus transcript has no installed archive record.' }
    foreach ($install in $installs) {
        if ((Get-OpusFullPath $install.Groups[1].Value.Trim()) -ne $installed) { throw 'Opus install transcript archive path mismatch.' }
    }
    $expected = Join-Path $OutDirectory "build/$($configurations[0])/opus.lib"
    foreach ($archive in [regex]::Matches($transcript, '(?m)^\s*opus\.vcxproj -> ([^\r\n]+)')) {
        if ((Get-OpusFullPath $archive.Groups[1].Value.Trim()) -ne $expected) { throw 'Opus project output conflicts with its install configuration.' }
    }
    return $configurations[0]
}

function Assert-OpusBuildPolicy {
    param([string] $OutDirectory)
    $out = Get-OpusFullPath $OutDirectory
    $build = Join-Path $out 'build'
    $cacheText = Read-OpusPolicyFile (Join-Path $build 'CMakeCache.txt')
    $cache = @{}
    foreach ($line in ($cacheText -split '\r?\n')) {
        if ($line -match '^([^/#][^:=]*):[^=]+=(.*)$') {
            if ($cache.ContainsKey($Matches[1])) { throw 'Duplicate CMake cache key.' }
            $cache[$Matches[1]] = $Matches[2]
        }
    }
    foreach ($setting in @('OPUS_HARDENING', 'OPUS_STACK_PROTECTOR', 'OPUS_USE_ALLOCA', 'OPUS_X86_MAY_HAVE_AVX2')) {
        if ($cache[$setting] -ne 'ON') { throw "Opus CMake policy is not enabled: $setting" }
    }
    foreach ($setting in @('OPUS_NONTHREADSAFE_PSEUDOSTACK', 'OPUS_X86_PRESUME_AVX2', 'OPUS_X86_PRESUME_SSE4_1')) {
        if ($cache.ContainsKey($setting) -and $cache[$setting] -ne 'OFF') { throw "Unsafe Opus CMake policy: $setting" }
    }
    if ($cache.CMAKE_BUILD_TYPE -notin @('Release', 'MinSizeRel')) { throw 'Unexpected upstream Opus CMAKE_BUILD_TYPE.' }
    $compilerFiles = @(Get-ChildItem -LiteralPath (Join-Path $build 'CMakeFiles') -Directory | ForEach-Object {
        $path = Join-Path $_.FullName 'CMakeCCompiler.cmake'
        if (Test-Path -LiteralPath $path -PathType Leaf) { $path }
    })
    if ($compilerFiles.Count -ne 1) { throw 'Expected one generated Opus compiler identity.' }
    $compiler = Read-OpusPolicyFile $compilerFiles[0]
    if ($compiler -notmatch '(?m)^set\(CMAKE_C_COMPILER_ID "MSVC"\)' -or
        $compiler -notmatch '(?m)^set\(CMAKE_C_COMPILER_ARCHITECTURE_ID "?x64"?\)' -or
        $compiler -notmatch '(?m)^set\(CMAKE_C_SIZEOF_DATA_PTR "8"\)') { throw 'Opus policy supports only actual x64 MSVC compilation.' }
    switch -Regex ($cache.CMAKE_GENERATOR) {
        '^Visual Studio 17 2022$' {
            if ($cache.CMAKE_GENERATOR_PLATFORM -ne 'x64') { throw 'Unexpected Opus Visual Studio platform.' }
            $configuration = Get-OpusVisualStudioConfiguration $out
            $compiled = Assert-OpusVisualStudio $build $configuration
        }
        '^Ninja$' { $configuration = $cache.CMAKE_BUILD_TYPE; $compiled = Assert-OpusNinja $build }
        default { throw "Unsupported Opus CMake generator: $($cache.CMAKE_GENERATOR)" }
    }
    $installed = Join-Path $out 'lib/opus.lib'
    foreach ($library in @($compiled, $installed)) {
        $file = Get-Item -LiteralPath $library -ErrorAction Stop
        if ($file.Length -lt 8 -or $file.Length -gt 64MB) { throw "Missing or invalid Opus archive: $library" }
        $stream = [IO.File]::OpenRead($library)
        try { $magic = [byte[]]::new(8); if ($stream.Read($magic, 0, 8) -ne 8 -or [Text.Encoding]::ASCII.GetString($magic) -ne "!<arch>`n") { throw "Not a COFF archive: $library" } } finally { $stream.Dispose() }
    }
    if ((Get-FileHash -LiteralPath $compiled -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash) {
        throw 'Installed Opus archive does not match the verified native build artifact.'
    }
    return [pscustomobject] @{ OutDirectory = $out; Generator = $cache.CMAKE_GENERATOR; NativeConfiguration = $configuration; Archive = $installed }
}
