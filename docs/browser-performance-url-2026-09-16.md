# URL/request correctness and performance guard — September 16, 2026

This follows the [DOMParser reassessment](browser-performance-domparser-2026-09-16.md).
The [URL/request contract](url-request-resolution.md) explains the standards slice
and the remaining parser gaps. This is a correctness change, not a loading-speed
optimization claim.

## Validation

| Measurement | Merged #166 | This slice |
|---|---:|---:|
| Failing assertions in the same 16-file URL cluster | 133 / 472 | 0 / 472 |
| Curated WPT gate | 369 files / 2,895 assertions | 380 files / 3,354 assertions |
| Four focused public-URL/native-request reproductions | 4 fail | 4 pass |
| Author-URL replacement fixture, Breeze/Chrome | Not previously gated | 3/3 pairs pass |
| Complete owned alpha matrix | 42 pairs / 14 fixtures | 45 pairs / 15 fixtures pass |

The final library suite passes 1,162 tests (one ignored), renderer-process
integration 129, live-runtime integration 82 (three live-service cases ignored),
and HTML parser conformance four. Clippy with warnings denied, formatting, and
source-size checks pass (20 existing target warnings; no ceiling changes).
An initial timing-sensitive live-runtime scroll test failed during a concurrent
release build. Its isolated rerun and the subsequent complete integration run
both pass; the rejected log is retained rather than counted as a passing run.
The diagnostic guard also exposed the adapter's assumption that all large WPT
callback reports could be emitted in one task. The owned adapter now schedules
bounded chunks across delayed tasks; upstream tests and the strict result reader
are unchanged. The initial truncated-report runs are retained as rejected evidence.

The independently run full constructor/origin/setter discovery corpus is still
**1,500 pass / 85 fail**. Its command exits nonzero, with no expected-failure
allowances, timeout, or crash. It is not included in the green curated count.
The owned request fixture is visually inspected in both browsers: all eight
behavioral checks are visible. List-marker placement differs; this is not a new
pixel-parity claim. Its median visual difference is 0.052 under the existing
0.12 ceiling; its readiness marker depends on actual loopback response content.

## Live comparison method

- Before: merged #166, main `f834ffc`; preserved executable SHA-256
  `1066A43CEDFF09C59F3861D3BB4FCF979BA79E0C55F3F8F68FB5B3F6DEC39E7E`.
- After: implementation `120cdaf`; release SHA-256
  `26CE3B40660A2317F09E01919C407115C1F1A53E12ADB8519FB64B1521C0ED44`.
- Same Windows/Ryzen machine and Chrome 153.0.8010.47 as the previous assessment.
  All browser runs hidden and muted, fresh temporary profiles, no concurrent
  builds or test suites, OS caches not flushed. Before/after/Chrome are interleaved
  with reversed order on alternate trials: five each on Coron and three each on
  Wikipedia Main Page and modern DuckDuckGo.
- Breeze window 1280 x 720, content approximately 1248.8 x 548 CSS pixels;
  Chrome requested 1249 x 548, reported 1250 x 548 after device-scale rounding.
  Scale 1.25, en-US, five-second settle, eight scroll samples. Coron also uses the
  existing six-second early-scroll trace.
- First presentation is process-relative, not complete-page readiness. Chrome
  load includes debugger/harness startup; FCP is navigation-relative. Hidden-window
  creation and debugger readiness are not comparable interactive-startup markers.
  Process-tree memory sums shared resident pages more than once; CPU covers
  different feature sets and observation windows. No equal-work speed ratio or
  leak conclusion follows from these samples.

## Final before/after measurements

Medians; all samples are retained, including the Main Page script error noted below.

| Measurement | Merged #166 | Final slice |
|---|---:|---:|
| Observed `Invalid scheme` console error, modern DDG | 3/3 runs | 0/3 runs |
| Modern DDG retained draw items, median | 275 | 2,549 |
| Coron first presentation | 685.1 ms | 658.9 ms |
| Coron style / layout-paint work | 639.2 / 1,186.9 ms | 655.7 / 1,195.5 ms |
| Coron cumulative CPU | 7,843.8 ms | 8,687.5 ms (+10.8%) |
| Coron working / private memory | 203.11 / 168.57 MiB | 200.97 / 167.66 MiB |
| Coron early-scroll p95, median per run | 6.942 ms | 6.283 ms |
| Main Page first presentation | 657.6 ms | 744.5 ms (+13.2%) |
| Main Page style / layout-paint work | 344.0 / 388.0 ms | 310.7 / 474.1 ms |
| Main Page cumulative CPU | 1,562.5 ms | 1,593.8 ms |
| Main Page working / private memory | 159.74 / 124.62 MiB | 164.16 / 127.78 MiB |

This is **not a general performance improvement**. Coron CPU increased and Main
Page first presentation/layout work increased in this final series. Earlier paired
samples had smaller or opposite differences; live script/network timing and
incomplete intermediate presentations limit causal attribution. Full-document
style/layout costs remain much higher than Chrome and need separate profiling.
All five final Coron early-scroll traces meet the existing acceptance, with zero
scroll-only style/layout rebuilds and 256 ms time to smooth.

| Page | Breeze first presentation | Chrome load / FCP | Working set B/C | Private B/C | CPU B/C |
|---|---:|---:|---:|---:|---:|
| Main Page | 744.5 ms | 784.4 / 268.5 ms | 164.16 / 649.01 MiB | 127.78 / 408.92 MiB | 1,593.8 / 5,437.5 ms |
| Coron | 658.9 ms | 781.1 / 302.1 ms | 200.97 / 678.96 MiB | 167.66 / 438.81 MiB | 8,687.5 / 8,421.9 ms |

Hidden-window creation is 12.8–16.9 ms for Breeze versus 236–240 ms for Chrome
debugger readiness. These are different milestones, not interactive startup parity.

## Live visual and functional limits

The final three modern DuckDuckGo captures show populated result content, report
no JavaScript errors or renderer stops, and no longer contain the observed
invalid-scheme failure. The retained-item counts are 2,469 / 2,549 / 2,556. A further
15-second run also completes. However, a side menu is incorrectly visible and
overlaps content; search input, result navigation and repeated-navigation acceptance
are not established. Chrome still receives a bot challenge, so these are not
equivalent-content performance samples. The HTML search fallback stays unchanged.

All eight final Wikipedia runs return HTTP 200 without renderer stops. One Main
Page run reports `element.attributes is not iterable`; the same error also occurred
in a preserved baseline run in the initial series (`paired/main-1-before.json`).
It is not counted as error-free or fixed by this slice.

Visual review also caught an incompletely populated Coron Appearance panel at the
five-second observation point in both baseline and new-build captures. Additional
15-second captures of both builds populate all eight controls; the new build's
selector diagnostics confirm eight inputs. This remains a readiness limitation,
not evidence that first presentation equals a fully usable page. Article spacing,
sidebar and list-marker differences remain; no Chrome pixel-parity claim is made.

## Evidence

The first URL-only release (`75b7c27`) removed the observed invalid-scheme error
but stopped the renderer in all three modern DuckDuckGo runs. A diagnostic build
confirmed that console output exceeded the IPC count limit. These rejected runs
remain under `paired` and are not performance wins or successful page loads. The
separate bounded-report fix (`120cdaf`) retains explicit truncation notices rather
than weakening the decoder or dropping operational state changes. Its standalone
15-second release run completes with populated content and no renderer stop.

Original JSON/PNG artifacts remain ignored under `target/url-resolution-proof`:
`alpha-final` contains all 45 final accepted pairs; `paired-final` contains the final
interleaved live captures; `wpt-accepted-final.json`, `wpt-url-before.json`, and `wpt-discovery-accepted-final.json`
retain all assertions. `compare.ps1` records the local live orchestration.
No captured third-party content, screenshots, or probes are committed.

Reproduce the owned gates with a prepared external pinned WPT checkout and current
release Breeze/Chromium harness builds:

```powershell
./scripts/run-wpt.ps1 -WptRoot G:/Git/wpt-breeze -SkipBuild -Jobs 4
./benchmarks/run-alpha.ps1 -SkipBuild -Iterations 3
```
