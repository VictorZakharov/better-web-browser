# Browser performance reassessment — 2026-09-16

For the later interleaved before/after/reference trials and the diagnosed bootstrap
blocker, see [the follow-up reassessment](browser-performance-bootstrap-2026-09-16.md).
The numbers below preserve the earlier #164 snapshot, not current-HEAD timings.

This replaced historical numbers as a **measured snapshot**, not as a
claim of Chrome parity. Observer implementation revision: `0bdcd9e39912ff377e06959114113a9ea889f6e6`.
Documentation-only follow-ups do not change that measured implementation.

## Conditions and interpretation

- Windows 10.0.26200, Ryzen 9 5900X, 24 logical processors, 127.9 GiB reported RAM;
  canonical Breeze release build and Chrome **153.0.8010.47** on the same machine.
- Three samples per browser/case, sequential fresh processes and temporary profiles;
  no competing agent builds or test suites during measurement. All runs hidden/headless
  and silent. This is **not cold-boot testing**: OS caches were not flushed.
- Owned matrix: all 12 fixtures, unchanged thresholds, 800 ms settle, eight steady
  scroll samples, 125% scale, `en-US`; matched content viewport about 1089 x 608 CSS px
  (the existing tolerance is two pixels). No-store fixture responses; Chrome cache disabled.
- Live Wikipedia: three pairs each, 1280 x 720 requested Breeze window, matched content
  viewport about 1249 x 548 CSS px, five-second settle, eight scroll samples; Coron also
  uses the six-second early-scroll trace. Sites/network/content can differ between requests.
- **Breeze ready** is process start to first owned layout/paint, not a full-page or
  usable-content milestone. **Chrome load** is harness/process start to load event,
  including DevTools setup; **Chrome FCP** starts at navigation, excluding process startup.
  These are deliberately labeled separately. Do not divide them into a speed ratio.
- Memory sums the browser's process tree: working set includes resident shared pages
  and can double-count them; private bytes report committed private memory, not just
  physical RAM. Neither is a leak test. CPU is cumulative process-tree CPU time through
  capture/scroll completion, not wall time or utilization; the measurement windows and
  executed feature sets differ. Average CPU percentages have unlike windows and are
  intentionally not compared.

## Current controlled results

**36/36 pairs pass**: populated/nonblank content, no Breeze script errors, existing
visual ceilings and readiness limits. These small original fixtures exercise supported
paths; they are not stand-ins for arbitrary real pages or YouTube playback.
All values below are three-run medians; memory pairs and CPU pairs are Breeze / Chrome.

| Fixture | Breeze ready, ms | Chrome load, ms | Chrome FCP, ms | Working set, MiB | Private, MiB | CPU, ms |
|---|---:|---:|---:|---:|---:|---:|
| Encyclopedia article | 221.4 | 494.6 | 18.4 | 63.5 / 558.6 | 36.9 / 295.7 | 1046.9 / 4734.4 |
| Encyclopedia main | 198.8 | 537.4 | 17.5 | 34.5 / 548.8 | 14.6 / 304.6 | 125.0 / 2828.1 |
| Responsive blog | 205.4 | 584.6 | 21.4 | 64.5 / 560.0 | 36.3 / 290.9 | 1203.1 / 5812.5 |
| Search results | 267.5 | 522.7 | 18.1 | 34.0 / 549.7 | 14.1 / 305.6 | 171.9 / 2875.0 |
| Compatibility dashboard | 202.2 | 514.3 | 19.1 | 33.7 / 546.0 | 13.9 / 303.2 | 140.6 / 3109.4 |
| Forms/storage | 219.8 | 574.8 | 22.1 | 33.6 / 548.9 | 13.4 / 302.5 | 125.0 / 3281.2 |
| Layout matrix | 200.8 | 490.7 | 16.9 | 33.6 / 540.9 | 13.8 / 297.9 | 125.0 / 2921.9 |
| Media/fonts | 194.3 | 480.0 | 18.4 | 38.6 / 542.0 | 18.9 / 307.3 | 156.2 / 2718.8 |
| Async mutation | 187.3 | 494.4 | 18.2 | 33.1 / 541.8 | 13.1 / 301.5 | 109.4 / 2781.2 |
| Custom elements | 199.7 | 483.9 | 18.2 | 33.5 / 541.3 | 13.8 / 295.6 | 171.9 / 2937.5 |
| Shadow components | 215.2 | 511.9 | 21.4 | 33.5 / 541.2 | 13.7 / 293.2 | 125.0 / 2953.1 |
| Constructed stylesheets | 199.0 | 557.6 | 19.7 | 33.1 / 542.0 | 13.3 / 299.5 | 125.0 / 3156.2 |

Both suites use **2 Breeze processes / 10 Chrome processes**. Chrome implements a
much larger platform; these memory/CPU totals are observations, not an efficiency
ratio for identical functionality.

### Startup probes

The existing `window_ready_ms` fields mean different things. Breeze records native
hidden-window creation; Chrome records debugger-port readiness. Neither proves a
visible interactive window or a usable page. Application launch parity needs a shared
user-visible milestone, so no "20x faster startup" claim is made from this table.

| Fixture | Breeze hidden window, ms | Chrome debugger ready, ms |
|---|---:|---:|
| Encyclopedia article | 12.1 | 226.9 |
| Encyclopedia main | 12.1 | 234.3 |
| Async mutation | 12.0 | 229.7 |

### Same-day before / after this PR

Baseline is the final #163 release executable `bf8dfa2`, whose code is merged in
`4289da5`; both batches use this same harness, Chrome version, profile policy and fixture
selection. This is three sequential samples per case, **not interleaved A/B trials**.

| Fixture | Breeze ready before → after, ms | Chrome load before → after, ms | Breeze working set before → after, MiB |
|---|---:|---:|---:|
| Encyclopedia article | 201.3 → 221.4 | 431.8 → 494.6 | 63.2 → 63.5 |
| Encyclopedia main | 171.1 → 198.8 | 413.4 → 537.4 | 34.6 → 34.5 |
| Async mutation | 155.6 → 187.3 | 397.7 → 494.4 | 33.4 → 33.1 |

Observed Breeze readiness is 10–20% higher in the second batch; Chrome is also
15–30% higher. This is **no evidence of a loading speedup**. It does not isolate a
code regression from host/run variation; interleaved trials and profiles are required
to attribute the difference. Memory is approximately unchanged on these cases.

### Scrolling

All six owned long-form traces pass the unchanged acceptance; all three Coron
traces also meet it. Every trace has zero scroll-only style/layout rebuilds and
time-to-smooth 256 ms. Below are medians of each run's p95 and maximum, not pooled
percentiles or the worst single sample.

| Breeze six-second trace | Input-to-paint p95, ms | Per-run maximum, ms | Samples passing |
|---|---:|---:|---:|
| Owned encyclopedia article | 3.06 | 9.44 | 3/3 |
| Owned responsive blog | 3.67 | 4.57 | 3/3 |
| Live Coron | 6.51 | 10.97 | 3/3 |

The worst Coron sample maximum is **24.64 ms**. Owned article steady-scroll repaint
work averages 3.0 ms (median run maximum 4.4 ms), versus Chrome's 12.8/14.0 ms
**two-animation-frame latency**. Those unlike instruments must not be used to claim
Breeze scrolls four times faster. Chrome's trace runs after load/settle; Breeze's
early trace begins at first presentation. Complete compositor/input parity remains unmeasured.

## Real-site reassessment

All six Wikipedia pairs completed, HTTP 200, zero recorded Breeze JavaScript errors
and no renderer exits. Inspected Main Page and Coron screenshots have readable
article/portal columns, infobox and appearance controls. Main Page whitespace,
typography and some geometry still differ: this is not pixel parity or every-interaction acceptance.

| Live page | Breeze first presentation, ms | Chrome load, ms | Chrome navigation FCP, ms | Working set B/C, MiB | Private B/C, MiB | CPU B/C, ms |
|---|---:|---:|---:|---:|---:|---:|
| Wikipedia Main Page | 742.0 | 868.2 | 293.3 | 163.6 / 647.7 | 127.7 / 396.0 | 1796.9 / 5968.8 |
| Coron, Palawan | 595.6 | 832.3 | 269.9 | 202.0 / 682.3 | 169.9 / 436.8 | 8734.4 / 7828.1 |

The fast first-presentation field does **not** prove the full page finishes in under
a second. Coron still accumulates 1370.8 ms of Breeze layout/paint work and 650.8 ms
of style work over the run (Chrome layout/style: 105.8/50.4 ms, different coverage).
Its total sampled CPU is higher than Chrome's despite its lower memory footprint.
Remaining post-load layout/style work is a concrete profiling target; we have not
proven what fraction can be removed or attributed that cost to IntersectionObserver.

Modern DuckDuckGo was separately retested at `https://duckduckgo.com/?q=game&ia=web`
with one fresh hidden release/Chrome pair, matched viewport/scale and five-second
settle. **It remains unsupported**: Breeze's main script and DOMContentLoaded each
hit the existing two-second JS watchdog, `#react-layout` stays empty, and the capture
is blank. Its reported 540 ms first layout is **not a successful search load**. Chrome
renders the shell but receives a bot challenge, so normal search-result parity cannot
be assessed from that reference. No challenge bypass, watchdog increase, provider
switch, or site-specific workaround was made. Track the unresolved site in [#89](https://github.com/VictorZakharov/better-web-browser/issues/89).

## Previous evidence reviewed

- [PR #145](https://github.com/VictorZakharov/better-web-browser/pull/145) explicitly
  called its banked ~3 s visual checkpoint historical and did not establish <2 s or
  Chrome parity. A visual checkpoint is not today's first-layout timestamp.
- [PR #153](https://github.com/VictorZakharov/better-web-browser/pull/153) removed the
  whole-response gate on a deliberately delayed streaming fixture (first content
  before EOF). It was not a general live-site CPU/memory benchmark.
- [PR #163](https://github.com/VictorZakharov/better-web-browser/pull/163) isolated
  early-scroll acceptance from competing tests without changing its thresholds.
  Its one-sample alpha run was a regression check; the current assessment uses medians.
- The [2026-08-24 alpha snapshot](alpha-compatibility.md) recorded article/main ready
  185.3/164.8 ms and working sets 49.0/26.0 MiB versus today's 221.4/198.8 ms and
  63.5/34.5 MiB. Feature coverage, renderer work and Chrome version changed; retain
  the historical increase rather than claiming all recent work made startup/memory
  better. That snapshot's live Wikipedia timeouts do not describe today's completed runs.

## Reproduction and artifacts

```powershell
./scripts/prepare-v8.ps1 -Profile release
cargo build --release --locked --bin better-web-browser
./benchmarks/run-alpha.ps1 -SkipBuild -Iterations 3 -OutputDirectory target/performance/current
```

For the live table, use the same alpha matrix schema with two cases (`Main_Page`,
`Coron,_Palawan`), viewport 1280 x 720, settle 5000 ms, eight scroll samples, and
`early_scroll:true` only for Coron, then run `run-alpha.ps1 -Live -Iterations 3
-Matrix <that-file> -SkipBuild`. The harness measures Breeze's actual content
viewport and applies it to Chrome. The standard matrix's live article URL is
`Web_browser`, **not** Coron, so do not substitute it when reproducing this table.

The exact local reports, matrices, environment metadata and inspected captures are
under ignored `target/intersection-proof/{perf-before,perf-after,live-after}` plus
`ddg-release.*` and `ddg-chrome-release.*`. Third-party captures and raw profiles are
not committed. Preserve failure reports, use release builds, and inspect captures
before turning any readiness field into a usability claim.
