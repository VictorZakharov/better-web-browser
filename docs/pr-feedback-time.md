# PR feedback time

Track end-to-end time until required CI is green and an executable is available for manual testing. Do not sum parallel worker durations or call total PR lifetime active engineering time. [Issue #245](https://github.com/VictorZakharov/better-web-browser/issues/245) tracks this work.

## Local iteration

Run `./scripts/build-browser.ps1` for the canonical release profile; `-Profile performance` selects the existing alternative iteration profile. Pass `-CompilerDirectory` when LLVM 19+ is not already selected by `LIBCLANG_PATH` or the supported installation paths. The script validates LLVM before Cargo, prepares V8, defaults to four Cargo jobs and restores its process environment afterward. Outputs and temporary files stay on G:.

Each invocation writes stage durations and the build revision to `target/build-reports/<UTC timestamp>/build.json`, including on failure. Completed Cargo timing reports are copied beside it. A failed preflight does not attach an earlier build's timing report. The performance profile is for iteration; final release handoffs and publishable runtime measurements retain the canonical release contract.

An edit inside a split-out workspace crate can be tested without the browser library: `cargo test -p breeze-webgl --lib` rebuilds in about 3 s after a WebGL edit in the incremental performance profile, against about 25 s for the browser lib-test build. See [Crate layout](crate-layout.md) for the crate graph and planned splits.

Use `./scripts/run-impacted-tests.ps1 -Base origin/main` to inspect development coverage, then add `-Run` to execute it. Prepare the compiler with `prepare-angle.ps1`, CMake with `prepare-build-tools.ps1`, and V8 with `prepare-v8.ps1 -Profile debug` in the same shell first. The final `-Final -Run` command retains the complete tests and doc tests and now includes full-target Clippy. It defaults local verification to the same zero-debug-info settings as CI, while retaining debug assertions, overflow checks and explicit user debug-info overrides. This avoids generating large PDBs for routine verification; ordinary debugger builds keep their existing settings.

## Hosted CI

The automatic gate retains its three hidden browser smoke tests, source/format contracts, dependency policy, harness self-tests and required `windows` and `Linear PR history` check names. Cargo's compiler timing HTML is retained as the `browser-smoke-cargo-timings` artifact for seven days, including when compilation fails after producing a report.

The registry source volume does not contain Git dependencies. The Windows worker now caches Cargo's Git database and checkouts separately, keyed by the lockfile; Cargo still resolves and builds the locked revision. When the exact registry image is absent, a compatible older image is mounted and Cargo adds missing locked packages before publishing the new image. Compiler-level sccache, registry caches and source-volume safety checks remain in place. Workspace `target` outputs are not cached.

New pushes cancel superseded runs of the same PR. Main runs have unique concurrency groups and are not cancelled by this policy.

## Baseline: 15 October 2026

| Measurement | PR #242 | Main after merge |
| --- | ---: | ---: |
| Required CI, workflow creation to aggregate completion | 4m 54s | 8m 44s |
| Browser-smoke job | 4m 27s | 8m 16s |
| Cargo compile/link | 3m 32s | 6m 44s |
| Execute three smoke tests | 1.47s | 1.59s |
| C/C++ compiler-cache hits | 397/397 | 250/397 |
| Rust compiler-cache hits | 269/299 | 270/299 |

Sources: [PR run 38755427759](https://github.com/VictorZakharov/better-web-browser/actions/runs/38755427759), [main run 38758246446](https://github.com/VictorZakharov/better-web-browser/actions/runs/38758246446). Main's V8 dependency-metadata step took 32.44s and fetched the ANGLE Git repository during preparation. A PR cache is not a guarantee that the subsequent main run has the same cache state.

The last local release build for #242 took 5m 58s with four jobs. It overlapped remote CI; combined feedback time was 6m 20s. Earlier full local gates took 16m 2s (one fixture race) and 14m 28s (successful repeat), consuming 30m 30s sequentially; full-target Clippy took another 4m 36s. These are separate historical attempts, not an additive controlled comparison with the final run.

PR #242 was created at 2026-10-14 18:58:00 UTC and merged at 2026-10-15 15:12:15 UTC: 20h 14m 15s elapsed, including the overnight pause, implementation, manual testing and rework.

## Release compiler comparison

The release package override changes only `better-web-browser` to 16 code-generation units. Dependencies retain their existing one-unit setting; optimization level 3, ThinLTO, panic behavior and stripping remain unchanged. The following sequential samples used the same browser source, Rust 1.95.0, native compiler setup and four Cargo jobs, without this task's heavy workloads overlapping.

| Sample | Entire build, including preparation | Our library + browser compile/link | Qualification |
| --- | ---: | ---: | --- |
| Sixteen units, first run | 6m 3s | 3m 5s | Also rebuilt ANGLE: 2m 52s |
| One unit, reference | 6m 15s | 6m 11s | Native dependencies warm; both library and browser rebuilt |
| Sixteen units, second run | 2m 12s | 2m 9s | Reused the library; not an edited-library rebuild comparison |
| Sixteen units, forced final rebuild | 3m 18s | 3m 16s | Both library and browser rebuilt; includes the small gate repairs below |

The same-source project stages were about 50% shorter. The forced final rebuild independently confirmed both compilation units ran (63.43s library, 132.07s browser), with warm native dependencies and no source contents changed by the timestamp-only rebuild trigger. Its 3m 18s total was about 47% below the 6m 15s reference, although this final source also includes the behavior-preserving Clippy and test-fixture repairs below. The first candidate's cold dependency cost and the second candidate's library reuse are retained explicitly; neither is presented as a matched full edited-source build. Final-head freshness and hosted timing are recorded in the PR description.

Eight hidden captures in reference/candidate/candidate/reference order completed the original Canvas label-atlas and 65-frame native publication fixtures. All completed their pixel checks without JavaScript errors or renderer exits. Shadowed 2048-wide label samples overlapped at about 0.54–0.65 ms per operation; these small workloads are a functionality check and do not establish broad runtime performance parity.

## First hosted timing sample

[Draft run 38768099326](https://github.com/VictorZakharov/better-web-browser/actions/runs/38768099326) passed in 3m 44s, with 2m 22s Cargo time and 2.09s executing the same three smoke tests. It had 397/397 C/C++ and 271/299 Rust compiler-cache hits. Its unchanged browser library was served from cache in 2.77s; this is not a representative edited-Rust cache miss or a controlled claim that the source-cache change caused the whole improvement.

The retained timing artifact identifies the next critical path: ANGLE's build script took 102.0s despite all native object-cache hits; V8's build script took 27.27s. Cached compilation still pays setup, preprocessing, binding generation and object restoration costs. The existing report does not isolate those sub-stages. The pinned sccache 0.17.0 [direct/preprocessor cache mode](https://github.com/mozilla/sccache/blob/v0.17.0/docs/Local.md) supports neither MSVC nor the remote backend used here, so simply enabling that option is not a fix.

## Full local verification

The final `-Final -Run` sample took 15m 37s, including full-target Clippy, all targets, doc tests and selector contracts: 6,723 tests passed, 25 remained ignored, none failed. This is a warm build observation, not a controlled before/after improvement claim.

| Stage | Wall time |
| --- | ---: |
| Full-target Clippy | 49.80s |
| Test compilation | 46.87s |
| Library test execution | 6m 14s |
| Live-runtime test execution | 5m 52s |
| Renderer-process test execution | 1m 36s |

That baseline used one sequential test worker. Library and live-runtime execution dominate the sample; reducing build time alone does not put the full gate under five minutes. [Complete local test runtime](test-runtime.md) documents the subsequent scheduler, resource ownership, finite-fixture completion and measurements.

Two unsuccessful setup/gate attempts remain part of the investigation evidence: missing CMake failed after 53.93s, and inherited Clippy/test-constructor failures after 3m 23s. The build helper now discovers the installed Visual Studio CMake component; the Clippy repair preserves behavior and the two audio transport fixtures construct the existing shared control object without changing assertions. An earlier missing-LLVM experiment also failed and is excluded from warm build comparisons. The PR description records all final verification and build results.

## Measurement rules

- Separate cold dependency preparation, warm edited-source builds, no-change builds, test execution, hosted queues and artifact publication.
- Compare the same source, toolchain, native compiler flags, features and job count. Retain individual samples, failed attempts, ranges and busy-machine outliers.
- Run local heavy workloads sequentially; remote CI can overlap the local build. Preserve the original executable when comparing compiler settings, and verify functionality before adopting a change.
- Target at most five minutes for a normal incremental change to reach completed required CI and a manual-test executable. Report cold/native rebuilds separately. The baseline above misses the combined budget.

Cargo exposes [profile and code-generation settings](https://doc.rust-lang.org/cargo/reference/profiles.html), and rustc documents their [compilation/runtime tradeoffs](https://doc.rust-lang.org/rustc/codegen-options/index.html#codegen-units). Measure these tradeoffs before changing the canonical release profile.
