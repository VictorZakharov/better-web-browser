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

## GitHub Actions feedback

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

The workflow keeps source/formatting, Clippy, two test shards, and curated WPT independent. The
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

Material changes from the baseline are:

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

### Remaining critical path

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
- [rustc linker options](https://doc.rust-lang.org/rustc/codegen-options/index.html#linker)
- [GitHub Actions dependency caching](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
- [GitHub-hosted runner software](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md)
- [sccache GitHub Actions backend](https://github.com/mozilla/sccache/blob/main/docs/GHA.md)
