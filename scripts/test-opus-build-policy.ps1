[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'assert-opus-build-policy.ps1')
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../target/opus-policy-tests-$([Guid]::NewGuid().ToString('N'))"))
if ($env:GITHUB_ACTIONS -ne 'true' -and [IO.Path]::GetPathRoot($root) -ne 'G:\') { throw 'Local policy fixtures must remain on G:.' }
[IO.Directory]::CreateDirectory($root) | Out-Null
$script:passed = 0
function Write-Fixture {
    param([string] $Path, [string] $Value)
    [IO.Directory]::CreateDirectory((Split-Path $Path)) | Out-Null
    [IO.File]::WriteAllText($Path, $Value)
}
function Expect-Rejection {
    param([string] $Name, [scriptblock] $Action)
    $rejected = $false
    try { & $Action | Out-Null } catch { $rejected = $true }
    if (-not $rejected) { throw "Unsafe Opus fixture accepted: $Name" }
    $script:passed++
}
function Expect-Acceptance {
    param([scriptblock] $Action)
    & $Action | Out-Null
    $script:passed++
}
$definitions = 'OPUS_BUILD;ENABLE_HARDENING;USE_ALLOCA;OPUS_HAVE_RTCD;OPUS_X86_MAY_HAVE_AVX2'
$cache = @'
CMAKE_GENERATOR:INTERNAL=Visual Studio 17 2022
CMAKE_GENERATOR_PLATFORM:INTERNAL=x64
CMAKE_BUILD_TYPE:UNINITIALIZED=Release
OPUS_HARDENING:BOOL=ON
OPUS_STACK_PROTECTOR:BOOL=ON
OPUS_USE_ALLOCA:BOOL=ON
OPUS_X86_MAY_HAVE_AVX2:BOOL=ON
OPUS_X86_PRESUME_AVX2:BOOL=OFF
OPUS_X86_PRESUME_SSE4_1:BOOL=OFF
'@
$compiler = @'
set(CMAKE_C_COMPILER_ID "MSVC")
set(CMAKE_C_COMPILER_ARCHITECTURE_ID x64)
set(CMAKE_C_SIZEOF_DATA_PTR "8")
'@
$project = @'
<Project xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <ItemDefinitionGroup Condition="'$(Configuration)|$(Platform)'=='Release|x64'">
    <ClCompile>
      <BufferSecurityCheck>true</BufferSecurityCheck>
      <PreprocessorDefinitions>DEFINITIONS</PreprocessorDefinitions>
      <AdditionalOptions>%(AdditionalOptions) -Brepro</AdditionalOptions>
    </ClCompile>
  </ItemDefinitionGroup>
  <ItemGroup>
    <ClCompile Include="source/opus_decoder.c" />
    <ClCompile Include="source/opus_encoder.c" />
    <ClCompile Include="celt/bands.c" />
    <ClCompile Include="celt/x86/pitch_avx.c">
      <EnableEnhancedInstructionSet Condition="'$(Configuration)|$(Platform)'=='Release|x64'">AdvancedVectorExtensions2</EnableEnhancedInstructionSet>
    </ClCompile>
  </ItemGroup>
</Project>
'@
$project = $project.Replace('DEFINITIONS', $definitions)
$out = Join-Path $root 'debug/build/opusic-sys-a123/out'
$build = Join-Path $out 'build'
$transcript = "  opus.vcxproj -> $(Join-Path $build 'Release/opus.lib')`n  -- Install configuration: `"Release`"`n  -- Installing: $(Join-Path $out 'lib/opus.lib')`ncargo:root=$out`n"
function Reset-Fixture {
    Write-Fixture (Join-Path $build 'CMakeCache.txt') $cache
    Write-Fixture (Join-Path $build 'CMakeFiles/fixture/CMakeCCompiler.cmake') $compiler
    Write-Fixture (Join-Path $build 'opus.vcxproj') $project
    Write-Fixture (Join-Path $build 'Release/opus.lib') "!<arch>`nowned policy fixture"
    Write-Fixture (Join-Path $build 'opus.lib') "!<arch>`nowned policy fixture"
    Write-Fixture (Join-Path $out 'lib/opus.lib') "!<arch>`nowned policy fixture"
    Write-Fixture (Join-Path (Split-Path $out) 'output') $transcript
}
Reset-Fixture
Expect-Acceptance { Assert-OpusBuildPolicy $out }
Write-Fixture (Join-Path (Split-Path $out) 'output') $transcript.Replace('-- Installing:', '-- Up-to-date:')
Expect-Acceptance { Assert-OpusBuildPolicy $out }
Reset-Fixture
foreach ($required in @('ENABLE_HARDENING', 'USE_ALLOCA', 'OPUS_HAVE_RTCD')) {
    Write-Fixture (Join-Path $build 'opus.vcxproj') $project.Replace(";$required", '')
    Expect-Rejection "VS omitted $required" { Assert-OpusBuildPolicy $out }
}
foreach ($unsafe in @('NONTHREADSAFE_PSEUDOSTACK', 'OPUS_X86_PRESUME_AVX2', 'OPUS_X86_PRESUME_SSE4_1')) {
    Write-Fixture (Join-Path $build 'opus.vcxproj') $project.Replace($definitions, "$definitions;$unsafe=0")
    Expect-Rejection "VS unsafe defined $unsafe" { Assert-OpusBuildPolicy $out }
}
foreach ($unsafeProject in @(
    $project.Replace('<BufferSecurityCheck>true', '<BufferSecurityCheck>false'),
    $project.Replace('-Brepro', '-Brepro /GS-'),
    $project.Replace('-Brepro', '-Brepro /UENABLE_HARDENING'),
    $project.Replace('-Brepro', '-Brepro /DENABLE_HARDENING=0'),
    $project.Replace('-Brepro', '-Brepro /DNONTHREADSAFE_PSEUDOSTACK'),
    $project.Replace('-Brepro', '-Brepro $(UnsafeFlags)'),
    $project.Replace('-Brepro', '-Brepro /arch:AVX2'),
    $project.Replace('Include="source/opus_decoder.c" />', 'Include="source/opus_decoder.c"><BufferSecurityCheck>false</BufferSecurityCheck></ClCompile>'),
    $project.Replace('Include="source/opus_decoder.c" />', 'Include="source/opus_decoder.c"><PreprocessorDefinitions>OPUS_BUILD</PreprocessorDefinitions></ClCompile>'),
    $project.Replace('Include="source/opus_decoder.c" />', 'Include="source/opus_decoder.c"><ExcludedFromBuild>true</ExcludedFromBuild></ClCompile>'),
    $project.Replace('Include="celt/x86/pitch_avx.c"', 'Include="celt/not_specialized.c"'),
    $project.Replace('Release|x64', 'Mystery|x64'),
    $project.Replace('<PreprocessorDefinitions>', '<!-- ENABLE_HARDENING USE_ALLOCA OPUS_HAVE_RTCD --><PreprocessorDefinitions>').Replace(';ENABLE_HARDENING', ''),
    '<!DOCTYPE Project [<!ENTITY unsafe "true">]>' + $project.Replace('>true</Buffer', '>&unsafe;</Buffer')
)) {
    Write-Fixture (Join-Path $build 'opus.vcxproj') $unsafeProject
    Expect-Rejection 'VS unsafe active compile settings or XML' { Assert-OpusBuildPolicy $out }
}
Reset-Fixture
Write-Fixture (Join-Path $build 'opus.vcxproj') $project.Replace('Release|x64', 'MinSizeRel|x64')
Write-Fixture (Join-Path $build 'CMakeCache.txt') $cache.Replace('CMAKE_BUILD_TYPE:UNINITIALIZED=Release', 'CMAKE_BUILD_TYPE:UNINITIALIZED=MinSizeRel')
Write-Fixture (Join-Path $build 'MinSizeRel/opus.lib') "!<arch>`nowned policy fixture"
Write-Fixture (Join-Path (Split-Path $out) 'output') $transcript.Replace('Release', 'MinSizeRel')
Expect-Acceptance { Assert-OpusBuildPolicy $out }
Reset-Fixture
Write-Fixture (Join-Path $build 'opus.vcxproj') $project.Replace('Release|x64', 'Debug|x64')
Write-Fixture (Join-Path $build 'Debug/opus.lib') "!<arch>`nowned debug archive"
Write-Fixture (Join-Path $out 'lib/opus.lib') "!<arch>`nowned debug archive"
Write-Fixture (Join-Path (Split-Path $out) 'output') $transcript.Replace('Release', 'Debug')
Expect-Acceptance {
    $selected = Assert-OpusBuildPolicy $out
    if ($selected.NativeConfiguration -ne 'Debug') { throw 'Cache Release incorrectly selected instead of actual Debug.' }
}
Reset-Fixture
foreach ($unsafeTranscript in @(
    $transcript.Replace('"Release"', '"Unknown"'),
    ($transcript + '  -- Install configuration: "Debug"'),
    $transcript.Replace('  -- Install configuration: "Release"', ''),
    $transcript.Replace('cargo:root=', 'cargo:unrelated='),
    $transcript.Replace('build\Release\opus.lib', 'build\Debug\opus.lib'),
    $transcript.Replace('lib\opus.lib', 'lib\other.lib'),
    ''
)) {
    Write-Fixture (Join-Path (Split-Path $out) 'output') $unsafeTranscript
    Expect-Rejection 'Unknown, conflicting, missing, or mismatched exact transcript' { Assert-OpusBuildPolicy $out }
}
Reset-Fixture
foreach ($unsafeCache in @(
    $cache.Replace('Visual Studio 17 2022', 'UnknownGenerator'),
    $cache.Replace('OPUS_HARDENING:BOOL=ON', 'OPUS_HARDENING:BOOL=OFF'),
    $cache.Replace('OPUS_STACK_PROTECTOR:BOOL=ON', 'OPUS_STACK_PROTECTOR:BOOL=OFF'),
    $cache.Replace('OPUS_X86_PRESUME_AVX2:BOOL=OFF', 'OPUS_X86_PRESUME_AVX2:BOOL=ON'),
    ($cache + "`nOPUS_NONTHREADSAFE_PSEUDOSTACK:BOOL=ON"),
    $cache.Replace('CMAKE_BUILD_TYPE:UNINITIALIZED=Release', 'CMAKE_BUILD_TYPE:UNINITIALIZED=Debug'),
    ''
)) {
    Write-Fixture (Join-Path $build 'CMakeCache.txt') $unsafeCache
    Expect-Rejection 'Unsafe or missing CMake policy' { Assert-OpusBuildPolicy $out }
}
Reset-Fixture
Write-Fixture (Join-Path $out 'lib/opus.lib') "!<arch>`nwrong installed artifact"
Expect-Rejection 'Archive mismatch' { Assert-OpusBuildPolicy $out }
Write-Fixture (Join-Path $out 'lib/opus.lib') 'not an archive'
Expect-Rejection 'Archive format' { Assert-OpusBuildPolicy $out }
Reset-Fixture
Write-Fixture (Join-Path $build 'CMakeFiles/fixture/CMakeCCompiler.cmake') $compiler.Replace('"MSVC"', '"GNU"')
Expect-Rejection 'Non-MSVC actual compiler' { Assert-OpusBuildPolicy $out }
Reset-Fixture
$ninjaDefs = '-D' + ($definitions -replace ';', ' -D')
$ninja = "include CMakeFiles/rules.ninja`n`n"
foreach ($source in @('src/opus_decoder.c', 'src/opus_encoder.c', 'celt/bands.c', 'celt/x86/pitch_avx.c')) {
    $flags = '/nologo /MD /O2 /GS'
    if ($source -match '_avx\.c$') { $flags += ' /arch:AVX2' }
    $ninja += "build CMakeFiles/opus.dir/$source.obj: C_COMPILER__opus_Release source/$source`n  DEFINES = $ninjaDefs`n  FLAGS = $flags`n`n"
}
$rules = @'
rule C_COMPILER__opus_Release
  command = "C:/Program Files/VC/cl.exe" /nologo $DEFINES $INCLUDES $FLAGS /Fo$out /FS -c $in
  description = Building C object $out

'@
function Reset-NinjaFixture {
    Reset-Fixture
    Write-Fixture (Join-Path $build 'CMakeCache.txt') $cache.Replace('Visual Studio 17 2022', 'Ninja')
    Write-Fixture (Join-Path $build 'build.ninja') $ninja
    Write-Fixture (Join-Path $build 'CMakeFiles/rules.ninja') $rules
}
Reset-NinjaFixture
Expect-Acceptance { Assert-OpusBuildPolicy $out }
Write-Fixture (Join-Path $build 'CMakeFiles/rules.ninja') $rules.Replace('$DEFINES', '${DEFINES}').Replace('$FLAGS', '${FLAGS}').Replace('"C:/Program Files', '${LAUNCHER}${CODE_CHECK}"C:/Program Files')
Expect-Acceptance { Assert-OpusBuildPolicy $out }
Write-Fixture (Join-Path $build 'build.ninja') ("LAUNCHER =`nCODE_CHECK = `n" + $ninja.Replace('  FLAGS =', "  LAUNCHER = `n  CODE_CHECK =`n  FLAGS ="))
Expect-Acceptance { Assert-OpusBuildPolicy $out }
Reset-NinjaFixture
foreach ($unsafeNinja in @(
    $ninja.Replace(' -DENABLE_HARDENING', ''),
    $ninja.Replace(' -DUSE_ALLOCA', ''),
    $ninja.Replace(' -DOPUS_HAVE_RTCD', ''),
    $ninja.Replace('-DOPUS_BUILD', '-DOPUS_BUILD -DNONTHREADSAFE_PSEUDOSTACK'),
    $ninja.Replace('-DOPUS_BUILD', '-DOPUS_BUILD -DOPUS_X86_PRESUME_AVX2'),
    $ninja.Replace(' /GS', ' /GS-'),
    $ninja.Replace(' /GS', ''),
    $ninja.Replace('/O2', '/O2 /arch:AVX2'),
    $ninja.Replace('FLAGS = /nologo', 'FLAGS = $unsafe /nologo'),
    $ninja.Replace(' -DENABLE_HARDENING', ' /DENABLE_HARDENING'),
    '',
    $ninja.Replace('C_COMPILER__opus_Release', 'UNKNOWN_COMPILER'),
    ("LAUNCHER = sccache`n" + $ninja),
    $ninja.Replace('  FLAGS =', "  CODE_CHECK = unsafe`n  FLAGS ="),
    $ninja.Replace('  FLAGS =', '  LAUNCHER = ${Unknown}' + "`n  FLAGS ="),
    ($ninja + "include unknown.ninja`n"),
    $ninja.Replace('include CMakeFiles/rules.ninja', 'subninja CMakeFiles/rules.ninja')
)) {
    Write-Fixture (Join-Path $build 'build.ninja') $unsafeNinja
    Expect-Rejection 'Ninja omitted or unsafe actual flags' { Assert-OpusBuildPolicy $out }
}
Reset-NinjaFixture
foreach ($unsafeRule in @(
    $rules.Replace('/nologo', '/nologo /GS-'),
    $rules.Replace('/nologo', '/nologo /DNONTHREADSAFE_PSEUDOSTACK'),
    $rules.Replace('$FLAGS', '$FLAGS $UnsafeFlags'),
    $rules.Replace('$FLAGS', '$FLAGS ${UnsafeFlags}'),
    $rules.Replace('$FLAGS', '${FLAGS'),
    $rules.Replace('$FLAGS', '@$RSP_FILE'),
    $rules.Replace('  description =', '  LAUNCHER = ${Unknown}' + "`n  description ="),
    ($rules + "include unknown.ninja`n")
)) {
    Write-Fixture (Join-Path $build 'CMakeFiles/rules.ninja') $unsafeRule
    Expect-Rejection 'Unsafe flags or unknown variables in Ninja rule, not edge' { Assert-OpusBuildPolicy $out }
}

# CMake's Windows/MSVC EncodePath emits backslash filenames, not backslash flags.
# Keep this separate realistic shape instead of replacing separators in whole commands.
$windowsNinja = "# Windows MSVC Ninja policy fixture`nninja_required_version = 1.5`nCONFIGURATION = Release`ninclude CMakeFiles\rules.ninja`n`nbuild cmake_object_order_depends_target_opus: phony`n`n"
foreach ($source in @('src/opus_decoder.c', 'src/opus_encoder.c', 'celt/bands.c', 'celt/x86/pitch_avx.c')) {
    $path = $source.Replace('/', '\')
    $flags = '-nologo -MD -Brepro /O2 /GS'
    if ($source -match '_avx\.c$') { $flags += ' /arch:AVX2' }
    $windowsNinja += "build CMakeFiles\opus.dir\$path.obj: C_COMPILER__opus_unscanned_Release " +
        'G$:\owned\opus\' + "$path || cmake_object_order_depends_target_opus`n  DEFINES = $ninjaDefs`n  FLAGS = $flags`n" +
        "  INCLUDES = -IG:\owned\opus\include`n  OBJECT_DIR = CMakeFiles\opus.dir`n  TARGET_COMPILE_PDB = CMakeFiles\opus.dir\opus.pdb`n`n"
}
$windowsRules = @'
rule C_COMPILER__opus_unscanned_Release
  deps = msvc
  command = ${LAUNCHER}${CODE_CHECK}"C:\Program Files\VC\cl.exe" /nologo $DEFINES $INCLUDES $FLAGS /showIncludes /Fo$out /Fd$TARGET_COMPILE_PDB /FS -c $in
  description = Building C object $out

'@
function Reset-WindowsNinjaFixture {
    Reset-NinjaFixture
    Write-Fixture (Join-Path $build 'build.ninja') $windowsNinja
    Write-Fixture (Join-Path $build 'CMakeFiles/rules.ninja') $windowsRules
}
Reset-WindowsNinjaFixture
Expect-Acceptance { Assert-OpusBuildPolicy $out }
Write-Fixture (Join-Path $build 'build.ninja') $windowsNinja.Replace('include CMakeFiles\rules.ninja', 'include CMakeFiles/rules.ninja')
Expect-Acceptance { Assert-OpusBuildPolicy $out }
Write-Fixture (Join-Path $build 'build.ninja') $windowsNinja.Replace("`r`n", "`n").Replace("`n", "`r`n")
Write-Fixture (Join-Path $build 'CMakeFiles/rules.ninja') $windowsRules.Replace("`r`n", "`n").Replace("`n", "`r`n")
Expect-Acceptance { Assert-OpusBuildPolicy $out }
foreach ($unsafeWindowsNinja in @(
    $windowsNinja.Replace(' -DENABLE_HARDENING', ''),
    $windowsNinja.Replace(' -DUSE_ALLOCA', ''),
    $windowsNinja.Replace(' -DOPUS_HAVE_RTCD', ''),
    $windowsNinja.Replace('-DOPUS_BUILD', '-DOPUS_BUILD -DNONTHREADSAFE_PSEUDOSTACK'),
    $windowsNinja.Replace('-DOPUS_BUILD', '-DOPUS_BUILD -DOPUS_X86_PRESUME_AVX2'),
    $windowsNinja.Replace('-DOPUS_BUILD', '-DOPUS_BUILD -DOPUS_X86_PRESUME_SSE4_1'),
    $windowsNinja.Replace(' /GS', ' /GS-'),
    $windowsNinja.Replace(' /GS', ''),
    $windowsNinja.Replace('/O2', '/O2 /UENABLE_HARDENING'),
    $windowsNinja.Replace('/O2', '/O2 /arch:AVX2'),
    $windowsNinja.Replace('FLAGS = -nologo', 'FLAGS = ${UnsafeFlags} -nologo'),
    $windowsNinja.Replace('src\opus_decoder.c', 'src\unrelated.c'),
    $windowsNinja.Replace('include CMakeFiles\rules.ninja', 'include CMakeFiles\unknown.ninja'),
    $windowsNinja.Replace('include CMakeFiles\rules.ninja', 'include CMakeFiles\..\CMakeFiles\rules.ninja'),
    $windowsNinja.Replace('include CMakeFiles\rules.ninja', 'include ${OwnedRules}'),
    ($windowsNinja + "include CMakeFiles\rules.ninja`n"),
    ($windowsNinja + "include CMakeFiles/rules.ninja`n"),
    $windowsNinja.Replace('include CMakeFiles\rules.ninja', 'subninja CMakeFiles\rules.ninja'),
    $windowsNinja.Replace('include CMakeFiles\rules.ninja', '')
)) {
    Reset-WindowsNinjaFixture
    Write-Fixture (Join-Path $build 'build.ninja') $unsafeWindowsNinja
    Expect-Rejection 'Unsafe flags, incomplete sources, or unknown include in Windows Ninja' { Assert-OpusBuildPolicy $out }
}
foreach ($nestedWindowsDirective in @('include CMakeFiles\rules.ninja', 'include CMakeFiles\unknown.ninja', 'subninja CMakeFiles\rules.ninja')) {
    Reset-WindowsNinjaFixture
    Write-Fixture (Join-Path $build 'CMakeFiles/rules.ninja') ($windowsRules + "`n$nestedWindowsDirective`n")
    Expect-Rejection 'Nested Windows Ninja include/default ownership' { Assert-OpusBuildPolicy $out }
}

$package = 'registry+https://github.com/rust-lang/crates.io-index#opusic-sys@0.7.5'
$scriptMessage = @{ reason = 'build-script-executed'; package_id = $package; out_dir = $out; linked_libs = @('static=opus'); linked_paths = @('native=' + (Join-Path $out 'lib')) }
$artifact = @{ reason = 'compiler-artifact'; package_id = $package; target = @{ name = 'opusic_sys'; kind = @('lib') }; features = @('bundled'); profile = @{ test = $false; debug_assertions = $true }; fresh = $true }
function Cargo-Lines {
    param($Script = $scriptMessage, $Library = $artifact)
    return @('an upstream log, not a command', ($Script | ConvertTo-Json -Depth 8 -Compress), ($Library | ConvertTo-Json -Depth 8 -Compress), '{"reason":"build-finished","success":true}')
}
Expect-Acceptance { Get-OpusCargoOutput (Cargo-Lines) $root debug }
$rebuiltArtifact = @{} + $artifact
$rebuiltArtifact.fresh = $false
Expect-Acceptance { Get-OpusCargoOutput (Cargo-Lines $scriptMessage $rebuiltArtifact) $root debug }
foreach ($unsafeLines in @(
    ,@(),
    ,@('{broken'),
    ,@(Cargo-Lines | Where-Object { $_ -notmatch 'build-finished' }),
    ,@(Cargo-Lines | ForEach-Object { $_.Replace('"success":true', '"success":false') }),
    ,@(Cargo-Lines | ForEach-Object { $_.Replace('opusic-sys@0.7.5', 'opusic-sys@0.7.6') }),
    ,@(Cargo-Lines | ForEach-Object { $_.Replace('static=opus', 'dylib=opus') }),
    ,@(Cargo-Lines | ForEach-Object { $_.Replace('"bundled"', '"no-hardening"') }),
    ,@(Cargo-Lines | ForEach-Object { $_.Replace('"debug_assertions":true', '"debug_assertions":false') }),
    ,@((Cargo-Lines) + ($scriptMessage | ConvertTo-Json -Depth 8 -Compress))
)) {
    Expect-Rejection 'Cargo exact build identity and completeness' { Get-OpusCargoOutput $unsafeLines $root debug }
}
$wrongOut = @{} + $scriptMessage
$wrongOut.out_dir = Join-Path $root 'release/build/opusic-sys-a123/out'
Expect-Rejection 'Cargo out_dir profile mismatch' { Get-OpusCargoOutput (Cargo-Lines $wrongOut) $root debug }
$wrongOut.out_dir = Join-Path (Split-Path $root) 'outside/debug/build/opusic-sys-a123/out'
Expect-Rejection 'Cargo out_dir escapes selected target' { Get-OpusCargoOutput (Cargo-Lines $wrongOut) $root debug }
$wrongOut = @{} + $scriptMessage
$wrongOut.linked_paths = @('native=' + (Join-Path $root 'unrelated/lib'))
Expect-Rejection 'Cargo native link path mismatch' { Get-OpusCargoOutput (Cargo-Lines $wrongOut) $root debug }
$releaseMessage = @{} + $scriptMessage
$releaseMessage.out_dir = Join-Path $root 'release/build/opusic-sys-b123/out'
$releaseMessage.linked_paths = @('native=' + (Join-Path $releaseMessage.out_dir 'lib'))
$releaseArtifact = @{} + $artifact
$releaseArtifact.profile = @{ test = $false; debug_assertions = $false }
Expect-Acceptance { Get-OpusCargoOutput (Cargo-Lines $releaseMessage $releaseArtifact) $root release }
Expect-Rejection 'Cargo oversized line' { Get-OpusCargoOutput @('x' * (1MB + 1)) $root debug }
Write-Host "Opus policy self-tests passed: $script:passed. Owned fixtures: $root"
