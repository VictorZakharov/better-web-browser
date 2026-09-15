# Build performance

Build-time work is measured separately from browser runtime performance. Published binaries and
runtime benchmark claims use Cargo's canonical `release` profile; the `performance` profile exists
only to shorten the local optimized edit/build loop.

## Local optimized rebuild

Measurements were collected on the Windows development host on 2026-08-16. The edited-source case
touches a frequently changed top-level Windows coordinator and rebuilds without changing the
dependency graph.

| Path | Wall time | Cargo-reported time |
| --- | ---: | ---: |
| Baseline edited `release` rebuild | 67.86 s | not recorded |
| Edited `performance` rebuild | 2.07 s | 2.00 s |
| Canonical `release` verification build | 99.72 s | included a broader rebuild |

The optimized edit path is **32.8x faster** than the recorded baseline. It keeps optimization but
disables release LTO, uses parallel code generation, and enables incremental compilation. The
canonical release profile retains thin LTO, one code-generation unit, symbol stripping, and its
existing panic behavior.

Rust's bundled MSVC-compatible `rust-lld` path was also measured. It reduced a warm edited
`performance` build from roughly 3.5 seconds to 3.01 seconds in that experiment, which was too small
and variable to justify changing the linker. The default MSVC linker therefore remains in use.

## Keep comparison builds isolated

Do not point an archived baseline's `--manifest-path` at the working tree's `target`
directory. During a September 14 comparison investigation, a reused integration-test
fingerprint tracked `perf-baseline-source-5e0aa38/tests/...` instead of the current
test sources. Cargo reported Fresh and ran an older executable with zero matches for
a newly added test. A package-only debug cleanup rebuilt the expected test, which
then failed as intended before its fix. Use a distinct target directory for each
source tree and verify that a targeted test actually runs, not just that Cargo exits
successfully. If contamination is confirmed, preview and clean only this package's
affected profile, preserving downloaded dependencies and other comparison evidence.

This local artifact issue is not evidence for the reported browser crash. Runtime
claims still require a canonical release build from the exact source head.

## GitHub Actions feedback

### September 15 PR policy (#155)

The required `windows` and `Linear PR history` names remain unchanged. Source-changing PRs retain
source/format checks, Clippy, all core tests, renderer smoke tests, focused Windows integration tests,
dependency advisories/licenses/bans/sources and notice verification, and Chromium harness self-tests.
The fixed `windows-2022` hosted image uses the default Cargo build concurrency. Rust executables
run in three independent PR workers: core, renderer, and the remaining
Windows integration targets. WPT-runner unit tests run with Windows integration, leaving the large
library test executable alone on the core worker to balance compilation/linking time.
PRs select fourteen renderer contracts covering containment, recovery,
native input/navigation, first paint, and video ownership/watchdogs. The selector verifies every
exact test name before execution, so renamed tests cannot silently become zero-test passes.
Main runs all 123 renderer tests. The renderer suite runs with one test thread because it shares a
single AppContainer profile and already serializes its sessions. Its media cadence test starts the
deliberately blocking JavaScript callback only after receiving the first decoded frame, so startup
timer advancement cannot trigger the hang prematurely. Watchdog assertions are unchanged.
`media_process` and `fullscreen_layout` are explicitly included; the previous CI target list omitted
those two executables. Valid media-decoding fixtures allow three seconds for cold OS codec startup;
failure-containment commands retain their 750 ms deadline, and startup-fault tests retain 200 ms.
The hidden-benchmark timeout self-test runs once, not on both visual shards.

The full twelve-fixture Breeze-versus-Chromium visual/performance matrix, curated WPT, and
`live_runtime` end-to-end tests now run only on pushes to `main`. No fixture or assertion was
deleted. This is an intentional coverage tradeoff: PR feedback is shorter, but broader integration
regressions can first be detected after merge. Run relevant local integration tests and the release
matrix before reviewing those changes. The checked-in aggregate policy rejects failed, cancelled,
missing, and incorrectly skipped workers; 142 policy cases cover the accepted and rejected result
combinations. Only a PR may skip these three broad suites, and only a confirmed Markdown-only PR
may skip the remaining workers. Main always requires the full suite. The final PR path runs 1,246
Rust tests; moving broader integration coverage to main is a deliberate speed/coverage tradeoff.

In PR #155, compiler-level sccache and Cargo package/index caches remained enabled; `target` was not cached.
Rust stays pinned by `rust-toolchain.toml`, including Clippy and rustfmt. V8 metadata is restricted to
the requested Windows target instead of unpacking other platforms' dependencies. Direct registry
directory caching through the cache action's MSYS tar was measured and rejected: a 124 MiB cache
took 27–43 seconds to extract on ordinary samples and 133 seconds on one worker. A native ZIP
experiment also regressed on its warm run (78 seconds extracting), so extracted sources are not
cached in that revision. Rust installation stays sequential: an attempted overlap caused a
component-install conflict and was removed. Those serial-extraction and background-install
experiments are not part of the current workflow.

The September 15 baseline was [PR #154's final run](https://github.com/VictorZakharov/better-web-browser/actions/runs/35017147276):
**4m47s** from workflow start to completion of `windows`. The slowest visual worker took 4m28s;
Windows integration took 3m57s. A four-fixture PR-smoke experiment still took 4m23s on its first run
and 4m36s on its repeat, so fixture reduction alone was rejected. The first run failed two media
startup timeouts; the repeat passed. A subsequent source-cache experiment also exposed a cold
font/watchdog timeout. No failed run counts as successful performance evidence.

The rejected native-archive configuration's first run passed in 3m23s, but its warm repeat took
4m02s and failed a media startup assertion. The three-worker policy's
[first run](https://github.com/VictorZakharov/better-web-browser/actions/runs/35023279509/attempts/1)
passed in **3m26s**, running 1,355 Rust tests (one pre-existing ignored test). This is 1m21s faster
than the baseline but does **not** meet the requested sub-three-minute target. Its critical renderer
worker spent 62 seconds resolving/unpacking dependencies and installing Rust, 57 seconds compiling,
and 32 seconds testing. A repeat passed in 3m14s. These samples preceded the final fourteen-test
renderer smoke selection. The full local release visual matrix passed all twelve fixtures; an earlier
full hosted run passed all 1,396 Rust tests and 220 curated WPT cases / 2,169 subtests. Runner
queueing, dependency changes, and cold builds remain variable.

The renderer-smoke configuration passed in
[3m59s](https://github.com/VictorZakharov/better-web-browser/actions/runs/35024549149/attempts/1)
and [3m22s](https://github.com/VictorZakharov/better-web-browser/actions/runs/35024549149/attempts/2).
Core became the critical path: the first run spent 95 seconds in dependency metadata/unpacking,
and the repeat's core worker finished 25 seconds after Windows integration. The subsequent target
rebalance moves the sixteen WPT-runner unit tests to that integration worker without reducing
coverage. These results do not establish reliable sub-three-minute feedback.

### Cargo-source filesystem cache (#156)

The essential PR checks and complete main suite from #155 are unchanged. This follow-up targets
dependency preparation, not test coverage, compiler options, or runner size. V8 contributes 15,317
of the 24,433 files in the hosted Windows registry-source snapshot. Serial extraction of these
small files dominated the earlier setup measurements.

The shared Windows action caches only `registry/src` inside one dynamically expanding NTFS
filesystem image (4 GiB capacity), mounted at Cargo's normal source directory. A warm worker
restores one image instead of recreating thousands of individual files. Cargo package
archives/indexes and compiler-level sccache remain separate and unchanged. The cache never
contains `target`, Cargo credentials, or global Cargo configuration. Its exact key includes
`Cargo.lock`, the pinned Rust toolchain configuration, and both volume helpers; it has no
partial-key fallback. Cargo's source-cache layout is not a stable API, so a toolchain/helper
change intentionally repopulates this cache.

On a cache miss, each worker resolves dependencies normally into a new temporary image. This
also puts cold extraction on the runner's temporary-data disk. Only the core worker publishes the
cache: it detaches the image to flush and unlock it before saving, then remounts it for compilation.
Other cold workers keep their images attached without packing/uploading. A first cache fill or
dependency change can therefore take longer than a warm run. Cache hits are writable
for Cargo compatibility but never saved back; each disposable VM owns its restored copy.

Disk operations refuse developer machines and self-hosted runners. The image and mount paths are
fixed children of the hosted runner's temporary/profile directories. Creation refuses an existing
image, mounting requires an empty directory, and unexpected reparse points are rejected. DiskPart
only creates/formats a new virtual image: it never selects or clears a physical disk. Later
operations resolve that exact image, verify its non-system disk, single partition, NTFS filesystem,
and label, and use the Storage cmdlets. The hosted VM discards the attached image when the job ends.
Local tests exercise path/command validation and the execution guard without invoking disk tools;
hosted compilation/tests exercise the actual create, detach, restore, and mount lifecycle.

The preceding four-ZIP parallel-extraction experiment was rejected. Local extraction of 15,317
V8 files improved from 7.30s to 3.20s, but hosted results did not follow: the
[cache-fill run](https://github.com/VictorZakharov/better-web-browser/actions/runs/35029585793)
passed in **3m12s**, then the
[cache-hit run](https://github.com/VictorZakharov/better-web-browser/actions/runs/35029800702)
passed in **3m43s**. The latter spent 29.47s (lint), 37.76s (core), and 79.40s (renderer) extracting
the same sources. Parallel ZIP extraction is not retained. The preceding merged PR #155's
[final source-change run](https://github.com/VictorZakharov/better-web-browser/actions/runs/35025572639)
took **3m55s**. These are workflow-start-to-required-`windows` measurements, not sums of job times.

The final volume configuration's
[cold-source-cache run](https://github.com/VictorZakharov/better-web-browser/actions/runs/35031297688/attempts/1)
passed in **2m56s**, 59 seconds (25.1%) below that baseline. Every source-changing PR check ran;
this was not the Markdown-only skip path. Windows integration setup took 34s, versus 109s in the
earlier ordinary-directory sample. Core setup took 54s including creating and publishing the
cache, then compilation took 75s and its 1,080 passing tests took 12.12s. The image cache occupied
about 65 MiB compressed; its 4 GiB capacity is not a preallocated download.

The unchanged configuration's
[warm repeat](https://github.com/VictorZakharov/better-web-browser/actions/runs/35031297688/attempts/2)
also passed, in **2m36s** (33.6% below the 3m55s baseline). The cold and warm attempts ran the same
1,246 passing Rust tests, Clippy, dependency policy, source/format/CI-policy checks, and Chromium
harness tests. These measurements include the classifier, runner allocation, setup, and final
required gate; they do not subtract those costs to claim a shorter wall clock.

Do not confuse this with a guaranteed service deadline. An earlier image-cache
[warm run](https://github.com/VictorZakharov/better-web-browser/actions/runs/35031002318) passed in
**6m18s** because the harness job waited from 22:27:00Z until 22:32:02Z for a hosted runner.
Its Rust workers all passed; source-image mounting took 4.09–4.44s on the measured integration/core
workers. Another [experimental run](https://github.com/VictorZakharov/better-web-browser/actions/runs/35030831622)
failed the existing three-second media frame-sequence decode deadline on a worker that still used
ordinary source extraction. That failure remains recorded, not counted as a timing success; the
deadline/assertions are unchanged and the later final configuration passed that test.

### Historical measurements

End-to-end time is measured from each workflow attempt's `run_started_at` timestamp through
completion of the required `windows` aggregate gate. Each sample is a fresh GitHub-hosted Windows
VM running the full source-change path; GitHub's rounded duration labels are not used for the
calculation.

| Revision and attempt | End to end | Slowest workers |
| --- | ---: | --- |
| [Baseline](https://github.com/VictorZakharov/better-web-browser/actions/runs/31856014762) | 2m51s | Test 2m37s; curated WPT 2m30s |
| [Final sample 1](https://github.com/VictorZakharov/better-web-browser/actions/runs/31970249397/attempts/1) | 2m21s | Curated WPT 2m00s; Windows integration 1m59s; core 1m58s |
| [Final sample 2](https://github.com/VictorZakharov/better-web-browser/actions/runs/31970249397/attempts/2) | 2m15s | Core 1m56s; Windows integration 1m48s; curated WPT 1m41s |
| [Final sample 3](https://github.com/VictorZakharov/better-web-browser/actions/runs/31970249397/attempts/3) | 2m20s | Lint 2m02s; Windows integration 1m58s; curated WPT 1m49s |
| **Final median** | **2m20s** | 31s (18.1%) faster than the 2m51s baseline |

At that revision, the workflow kept source/formatting, Clippy, two test shards, and curated WPT independent. The
required `windows` and `Linear PR history` names are unchanged. Markdown-only pull requests still
take the checked-in fail-safe classifier path and skip every Windows worker; pushes to `main` run the
full suite.

### Public-alpha compatibility critical path

The public technical-alpha milestone added a required nine-fixture Breeze-versus-Chromium gate.
Building the canonical release profile inside every pull request put thin LTO and single-unit code
generation on the critical path even though publishable benchmark evidence and release packaging
already retain their own release-profile contract.

Exact GitHub timestamps measure the correction:

| Revision | End to end | Public-alpha worker |
| --- | ---: | ---: |
| [Release-profile baseline](https://github.com/VictorZakharov/better-web-browser/actions/runs/32745221188) | 8m54s | 8m29s |
| [Debug-profile compatibility gate](https://github.com/VictorZakharov/better-web-browser/actions/runs/32748153419) | 4m53s | 4m30s |

The pull-request path is **4m01s (45.1%) faster** while retaining all nine fixtures and every
structural, visual, JavaScript, page-readiness, and early-scroll assertion. The measured
public-alpha worker spent 57 seconds in checkout/cache setup, 69 seconds building Breeze, 30 seconds
building the Chromium harness, and 1m50s in the hidden browser matrix. Generated reports record the
Breeze build profile so debug-profile regression signals cannot be mistaken for canonical release
claims. The job has a six-minute hard limit; distributable binaries and published benchmark claims
continue to use `release`.

Material changes in that historical experiment were:

- compiler outputs remain in the compiler-level sccache backend;
- Cargo package archives, index data, and extracted registry sources share one content-addressed
  cache, while `target` is never cached directly;
- the pinned WPT checkout is restored by manifest hash and verified without a redundant fetch;
- source, formatting, lint, tests, and WPT run in parallel, with test executables divided between a
  core shard and a Windows/AppContainer integration shard;
- the current stable Rust, Clippy, and rustfmt already installed on `windows-latest` are used instead
  of performing a network update in every worker; and
- curated WPT runs eight hidden browser cases concurrently, while every Breeze launch retains its
  `CREATE_NO_WINDOW` path.

### Historical remaining critical path

The original compile/test path's initial sub-two-minute target is not yet met; its measured median
misses it by 20 seconds. The later public-alpha gate now determines pull-request wall clock at
roughly five minutes. On its measured run, an exact Cargo registry-source cache hit took 41 seconds
to extract, and the serial nine-fixture matrix took 1m50s after both browser builds. The uncached
workspace outputs, V8 archive extraction and Cargo fingerprinting, and MSVC linking still leave the
compile-heavy workers clustered around two minutes.
Adding more compiler workers was measured and rejected: remote-cache read latency doubled and the
end-to-end result regressed.

The next public-alpha investigation should evaluate isolated fixture shards or a shared browser
artifact without contaminating same-runner performance comparisons or increasing compiler-cache
contention. The broader build investigation should make V8 archive reuse and the workspace
output graph cheaper to fingerprint and link, then remove the serial classification delay if branch
protection can remain fail-safe. A larger or persistent runner is an operational fallback, not a
source-level fix. Direct caching of `target` remains excluded because it has not produced reliable
hits for the native-engine and workspace build graph.

## References

- [Cargo build profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [Cargo test target selection](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [Cargo home and registry caching](https://doc.rust-lang.org/cargo/guide/cargo-home.html)
- [Windows virtual disk images](https://learn.microsoft.com/en-us/windows/win32/vstor/about-vhd)
- [Mount-DiskImage](https://learn.microsoft.com/en-us/powershell/module/storage/mount-diskimage?view=windowsserver2025-ps)
- [DiskPart script error handling](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/diskpart-scripts-and-examples)
- [rustc linker options](https://doc.rust-lang.org/rustc/codegen-options/index.html#linker)
- [GitHub Actions dependency caching](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
- [GitHub-hosted runner software](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md)
- [sccache GitHub Actions backend](https://github.com/mozilla/sccache/blob/main/docs/GHA.md)
