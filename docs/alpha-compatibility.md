# Public technical-alpha compatibility evidence

This page records the reproducible acceptance evidence for the real-world alpha gate. The source of truth is `benchmarks/alpha/matrix.json`; methodology and rerun instructions are in [`benchmarks/README.md`](../benchmarks/README.md).

## Deterministic gate result

On 2026-08-24, three hidden release samples per browser and fixture completed successfully: **27/27 Breeze–Chromium pairs**, with no expected failures. The machine ran Windows 10.0.26200, 24 logical processors, 125% display scaling, `en-US`, and Chrome 151.0.7922.170. Every run used a new profile; the loopback fixture server disabled caching.

| Fixture | Breeze ready | Chromium load | Chromium FCP | Breeze layout/paint | Breeze/Chromium working set | Visual diff / ceiling |
|---|---:|---:|---:|---:|---:|---:|
| Encyclopedia article | 185.3 ms | 385.7 ms | 14.2 ms | 15.8 ms | 49.0 / 578.7 MiB | 0.217 / 0.28 |
| Encyclopedia main | 164.8 ms | 398.5 ms | 14.7 ms | 10.3 ms | 26.0 / 562.8 MiB | 0.063 / 0.12 |
| Responsive blog | 176.3 ms | 380.3 ms | 14.9 ms | 10.0 ms | 50.9 / 578.7 MiB | 0.140 / 0.20 |
| Search results | 170.3 ms | 383.0 ms | 14.5 ms | 8.4 ms | 25.7 / 564.1 MiB | 0.045 / 0.12 |
| Compatibility dashboard | 168.8 ms | 373.9 ms | 13.5 ms | 9.2 ms | 25.4 / 558.4 MiB | 0.043 / 0.12 |
| Forms/storage | 155.5 ms | 384.4 ms | 14.4 ms | 5.5 ms | 25.1 / 565.5 MiB | 0.049 / 0.12 |
| Layout matrix | 155.4 ms | 374.4 ms | 14.3 ms | 7.7 ms | 25.3 / 559.3 MiB | 0.045 / 0.12 |
| Media/fonts | 168.5 ms | 377.1 ms | 15.0 ms | 6.3 ms | 30.2 / 561.0 MiB | 0.094 / 0.15 |
| Async mutation | 184.0 ms | 369.8 ms | 14.1 ms | 5.4 ms | 24.8 / 557.4 MiB | 0.033 / 0.12 |

“Ready” is process start through Breeze's first owned layout/paint. Chromium “load” is process start through `Page.loadEventFired`; FCP is navigation-relative and therefore not directly comparable to Breeze ready. No fixture exceeded the 2× Chromium page-ready ceiling.

The same median set records JavaScript, style, layout, capture, scroll, private memory, CPU, and process-tree metrics in generated `summary.json`. Across these samples Breeze used 2 processes versus Chromium's 10. Median Breeze steady-scroll paint averages were 2.3–3.3 ms; Chromium's two-animation-frame scroll measurements were 29.3–31.0 ms. These fields use different instrumentation and are retained for trend/regression analysis, not presented as like-for-like browser-speed claims.

Both six-second long-document traces passed the established early-scroll thresholds in all three samples:

| Fixture | Median p95 input-to-paint | Median maximum | Median time to smooth | Result |
|---|---:|---:|---:|---|
| Encyclopedia article | 2.25 ms | 3.11 ms | 256 ms | 3/3 passed |
| Responsive blog | 2.24 ms | 2.97 ms | 256 ms | 3/3 passed |

The fixtures verify HTTP success, populated/visible primary content, retained paint output, scripts without errors, asynchronous readiness, nonblank captures, and visual ceilings. Link activation, control editing, and form submission remain covered by the hidden renderer-process input regression suite; the alpha forms fixture additionally covers rendered controls, validation attributes, `FormData`-compatible control state, and local-storage script execution.

## Live side-by-side evidence

Live evidence was collected separately on 2026-08-24 with one hidden fresh-profile run, a 45-second per-browser bound, paired PNG captures, HTTP/final-URL diagnostics, DOM/paint counts, and JavaScript error collection. Captures are intentionally not committed because the pages are third-party content; the command regenerates them under ignored `benchmark-results/` or a selected output directory.

| Target | HTTP/final URL | Breeze retained items | Breeze JS errors | Nonblank pair | Visual diagnostic | Result |
|---|---|---:|---:|---|---:|---|
| Responsive blog | 200, `https://www.neolisk.blog/` | 939 | 0 | yes | 0.735 | Completed; substantial live visual divergence remains |
| DuckDuckGo HTML | 200, requested results URL | 445 | 0 | yes | 0.170 | Completed |
| HTML5test | 200, `https://html5test.com/` | 243 | 0 | yes | 0.054 | Completed |
| Wikipedia article | no report | — | — | no | — | Bounded Breeze timeout at 45 s |
| Wikipedia main page | no report | — | — | no | — | Bounded Breeze timeout at 45 s |

The Wikipedia timeouts are failures, not accepted error surfaces or performance exceptions. They do not weaken the deterministic Wikipedia-like gates; they document the current live-network result. Live perceptual values are diagnostics only because content, experiments, fonts, anti-automation behavior, and response ordering can differ between sequential browser requests.

## Claim boundary

The results establish an explicit gate for the owned feature-equivalent paths. They do not establish whole-web compatibility or a universal performance advantage. Chromium implements a much larger platform; Breeze has one end-to-end non-DRM H.264/AAC media path but still lacks or only partially implements canvas, broader media formats and DRM, accessibility, cross-site isolation, and broad standards coverage. The separate curated WPT gate and discovery sample quantify standards support without conflating a passing selection with global conformance.
