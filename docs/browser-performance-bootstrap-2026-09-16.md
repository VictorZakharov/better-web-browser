# Bootstrap and post-load reassessment — September 16, 2026

This follows [the earlier same-day assessment](browser-performance-2026-09-16.md).
The implementation and compatibility decisions are described in
[native microtasks and resource invalidation](native-microtasks-and-resource-invalidation.md).
Modern DuckDuckGo remains unsupported, and this change does not establish a live
Wikipedia loading speedup.

## Measurement conditions

- Before: the final #164 release executable (code merged as `b3170d9`), SHA-256
  `F7261A1B92C4F49426C331AE025D842379CA74CDF0A07AE7F42B9D75AF86B6CD`.
  After: implementation `31b8580` (including native microtasks in `1f4b49c`),
  canonical release build with diagnostic stack sampling removed. Subsequent
  documentation/build-preparation changes do not alter the measured browser code.
- Windows 10.0.26200, Ryzen 9 5900X, 24 logical processors; Chrome 153.0.8010.47.
  Sequential hidden/silent Breeze runs and unified-headless, muted Chrome. Fresh
  temporary profiles, no competing builds/test suites, OS caches not flushed.
- Interleaved before/after/Chrome trials, reversing browser order on alternating
  trials: five per browser on Coron, three on Wikipedia Main Page and DuckDuckGo.
  Requested Breeze window 1280 x 720; content viewport 1248.8 x 548 CSS px, matched
  Chrome viewport 1249 x 548, 125% scale, `en-US`, five-second settle, eight scroll
  samples. Coron additionally runs the existing six-second early-scroll trace.
- Breeze ready means **first presentation from process start**, not app usability.
  Chrome load includes harness/debugger setup; Chrome FCP is navigation-relative.
  No browser speed ratio can be derived from these different milestones.
- Memory sums working set/private committed bytes over 2 Breeze versus 10 Chrome
  processes; shared resident pages can be counted twice. CPU is cumulative process
  CPU through capture/scroll completion. Feature coverage and measurement windows
  differ; these are neither leak tests nor equal-work efficiency comparisons.

## Before / after

Values are per-run medians, not pooled timings. The live pages can change between
requests. In particular, a shorter first-presentation time is not proof of faster
complete-page loading.

| Measurement | Before | After | Interpretation |
|---|---:|---:|---|
| DDG main module | 2,014.0 ms, timed out | 108.0 ms, completed | Confirmed recursive scheduling fixed |
| DDG total JavaScript | 4,161.3 ms | 824.3 ms | Advances beyond the watchdog stalls, still incomplete |
| DDG working set / private memory | 261.7 / 230.6 MiB | 184.6 / 147.8 MiB | Different failure states, not usable-search parity |
| Wikipedia Main first presentation | 534.6 ms | 579.5 ms | No measured improvement |
| Coron first presentation | 543.6 ms | 532.2 ms | Small change, not an established speedup |
| Coron style work | 533.7 ms | 555.7 ms | Not reduced in the live median |
| Coron layout/paint work | 1,133.8 ms | 1,157.0 ms | Not reduced in the live median |
| Coron full-layout rebuilds | 18 | 18 | Remaining work is not solved by this slice |
| Coron cumulative CPU | 7,875.0 ms | 8,140.6 ms | No overall CPU win established |
| Coron working set / private memory | 211.6 / 177.0 MiB | 212.6 / 179.7 MiB | Approximately unchanged |
| Coron early-scroll input-to-paint p95 | 5.392 ms | 5.033 ms | All five before and five after traces pass |

The resource fix has a **deterministic regression contract**, not an inflated
live-speed claim: script source and nonvisual load callbacks no longer publish an
unchanged presentation; mutations still paint and error handlers still start
follow-up work. It does not remove the dominant remaining Coron style/layout cost.
The sampled maximum scroll latency after the change is 11.848 ms; all five runs
retain zero scroll-only style/layout rebuilds and 256 ms time-to-smooth.

## Fresh Chrome reference

| Live page | Breeze first presentation | Chrome load / navigation FCP | Working set B/C | Private B/C | CPU B/C |
|---|---:|---:|---:|---:|---:|
| Wikipedia Main Page | 579.5 ms | 662.3 / 232.1 ms | 160.0 / 642.3 MiB | 124.4 / 396.9 MiB | 1,312.5 / 4,750.0 ms |
| Coron, Palawan | 532.2 ms | 711.3 / 259.8 ms | 212.6 / 693.4 MiB | 179.7 / 442.6 MiB | 8,140.6 / 6,109.4 ms |

Hidden-window creation is 9.0/9.1 ms for Breeze (Main/Coron); debugger readiness is
230.1/209.6 ms for Chrome. As before, these are **not interactive startup parity**.
Chrome's Coron layout/style medians are 80.0/38.7 ms, versus Breeze's
1,157.0/555.7 ms. Unlike total first-presentation timing, this remains a clear reason
to profile the repeated full-document work, while accounting for different coverage.

All eight after-change Wikipedia runs completed with HTTP 200 and no reported
JavaScript errors. Inspected Coron captures retain readable article columns,
infobox, contents scrollbar, and appearance controls. Typography and geometry are
not pixel-identical to Chrome; this PR is not a new visual-parity claim.

All three DDG after-change runs avoid the old module/lifecycle watchdog failures.
Their console logs instead show a caught missing-`DOMParser` error, and inspected
captures show an incomplete shell without useful results. Headless Chrome receives
a bot challenge, so it is not a normal search-result acceptance reference either.
No challenge bypass, provider switch, site patch, or watchdog increase was used.

## Reproduction and evidence

The final owned matrix passes **39/39 Breeze/Chrome pairs** (13 fixtures, three
runs each), including the explicit ready-marker check in both browsers. Selected
medians, retaining the different readiness definitions:

| Owned fixture | Breeze first presentation | Chrome load | Working set B/C | Visual difference / ceiling |
|---|---:|---:|---:|---:|
| Encyclopedia article | 156.8 ms | 419.8 ms | 63.4 / 557.6 MiB | 0.018 / 0.28 |
| Encyclopedia main | 163.6 ms | 412.9 ms | 34.6 / 548.0 MiB | 0.036 / 0.12 |
| Native microtask bootstrap | 147.3 ms | 392.8 ms | 32.4 / 539.1 MiB | 0.019 / 0.12 |

The earlier stricter-matrix attempt failed once on the blog's unchanged 2x timing
gate: first presentation 3,493.852 ms versus Chrome load 400.040 ms. Reported network,
JS, and layout totals remained about 21/24/26 ms; the first renderer timeline entry
arrived at 3,452 ms. The report does not identify the remaining delay, so it is not
attributed to the OS, network, or these changes. The complete repeat passes without
changing code or thresholds. Preserve this startup outlier for follow-up rather than
claiming all attempts passed. `target/bootstrap-proof/alpha-ready` retains the failed
attempt; `alpha-ready-final` contains the final full matrix and summary.

Validation includes all 1,140 library tests, the renderer-process and live-runtime
suites (82 live tests passed, three explicitly ignored), Clippy with warnings denied,
formatting, source-size policy, and all **361 curated WPT files / 2,826 assertions**.
The three upstream microtask tests were already in that manifest; they were rerun,
not counted as new coverage. New coverage consists of four owned runtime regressions,
two renderer-process regressions, and the original bootstrap fixture.

Raw reports and inspected captures are under ignored
`target/bootstrap-proof/paired`: `coron-{1..5}-{before,after,chrome}` and
`{main,ddg}-{1..3}-{before,after,chrome}`. Baseline identity is recorded above.
The original minimal Promise-scheduler fixture fails on the old executable with
`Maximum call stack size exceeded`; the test is not a copied vendor script.
Temporary diagnosis reports are separate from the final measured release runs.

Run the owned matrix with `./benchmarks/run-alpha.ps1 -SkipBuild -Iterations 3`
after building Breeze and the Chrome harness. Its new readiness guard was also
checked directly to reject an otherwise nonblank, error-free report missing the
ready marker.

```powershell
./scripts/prepare-v8.ps1 -Profile release
cargo build --release --locked --bin better-web-browser
./scripts/run-hidden-benchmark.ps1 -Url 'https://en.wikipedia.org/wiki/Coron,_Palawan' `
  -Output target/coron.json -Screenshot target/coron.png -FreshProfile `
  -SettleMs 5000 -ScrollSamples 8 -EarlyScrollTrace -DeviceScaleFactor 1.25 `
  -WindowWidth 1280 -WindowHeight 720 -DiagnosticSelector main,'#react-layout'
```

Repeat with `-Browser <preserved-baseline-exe>` and the new release, alternating
order. Run the repository Chrome harness with the matched viewport, scale, locale,
settle, and scroll settings above. Do not run builds or other benchmarks concurrently.

The remaining targets are the standards-based DOMParser vertical slice and a
separate profile of Coron's repeated style/layout checkpoints, including synchronous
geometry flushes. No percentage completion or ETA follows from removing one blocker.
