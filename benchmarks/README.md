# Public alpha compatibility and performance gate

`run-alpha.ps1` is the reproducible Breeze-versus-Chromium gate for the public technical alpha. By default it builds the canonical release browser and release Chromium harness, serves repository-owned fixtures on loopback, launches fresh hidden browser profiles, captures comparable page surfaces, enforces compatibility thresholds, and writes raw JSON plus median Markdown and JSON summaries.

The Chromium baseline is owned by this repository under `benchmarks/chromium` and targets .NET 8 without third-party packages. It uses Chrome DevTools Protocol directly. Chrome documents `--headless` as its current unified headless mode, which creates no displayed platform windows: [Chrome Headless mode](https://developer.chrome.com/docs/automation-and-testing/headless). Metric and environment controls use the official [Performance](https://chromedevtools.github.io/devtools-protocol/tot/Performance/), [Page](https://chromedevtools.github.io/devtools-protocol/tot/Page/), and [Emulation](https://chromedevtools.github.io/devtools-protocol/tot/Emulation/) domains.

## Deterministic matrix

Every checked-in page, stylesheet, script, and SVG is original project material. The fixtures imitate useful page roles without copying third-party page bodies or captures.

| Fixture | Compatibility role | Extra gate |
|---|---|---|
| `encyclopedia-article` | Long-form article, infobox, contents, references | Six-second early-scroll trace |
| `encyclopedia-main` | Dense portal/main-page structure | Structural and visual checks |
| `responsive-blog` | Responsive article and cards | Six-second early-scroll trace |
| `search-results` | Search form and result list | Form/layout/visual checks |
| `compatibility-dashboard` | Script-populated capability dashboard | DOM mutation and visual checks |
| `forms-storage` | Forms, validation, and Web Storage | Script and structural checks |
| `layout-matrix` | Flex, grid, table, float, overflow | Layout and visual checks |
| `media-fonts` | Raster data URL, SVG, and system webfont | Resource and visual checks |
| `async-mutation` | Delayed DOM/class/text mutation | Settle and visual checks |

Run the same local matrix used by CI:

```powershell
.\benchmarks\run-alpha.ps1 -Iterations 3
```

Use `-Fixture layout-matrix,media-fonts` for a focused run, `-OutputDirectory <path>` to select the artifact location, or `-SkipBuild` only when both selected outputs are already current. Pull-request CI passes `-BuildProfile debug`: it preserves every compatibility assertion while reusing the same compiler-cache inputs as the core, integration, and WPT workers instead of adding a release-LTO build to the critical path. Measurements from that profile are regression signals, not publishable performance claims. `compare.ps1` is a compatibility alias for the same runner.

The default output under `benchmark-results/alpha-<timestamp>/` contains:

- one JSON report and PNG capture per browser, fixture, and iteration;
- `raw-results.json`, including load/first usable paint, JavaScript, style, layout/paint, scroll, memory, CPU, and process-tree metrics;
- `summary.json` and `REPORT.md`, using medians across iterations;
- the exact commit, OS, processor count, locale, and Chromium version.

## Controlled comparison contract

- Canonical evidence runs both browsers on the same machine from release builds. Pull-request CI uses Breeze's `debug` profile and the release Chromium harness; its timing fields are regression signals only. Every browser remains hidden and gets a new temporary profile for every sample. Cache is disabled in Chromium; the loopback server sends `Cache-Control: no-store`.
- The matrix fixes the outer Breeze window, `en-US` locale, settle period, scroll sample count, and fixture bytes. Breeze's observed content viewport and Windows scale factor are then applied to Chromium. Fractional Windows scaling requires at most a two-CSS-pixel viewport tolerance because CDP accepts integer dimensions and Chromium quantizes device pixels.
- Breeze launches only through `scripts/run-hidden-benchmark.ps1`, which fail-closes unless the actual child command line contains `--benchmark`. Chromium launches with the exact `--headless` flag, `CreateNoWindow`, a fresh profile, and a visible-window check.
- Local captures must be nonblank and structurally populated. Breeze must report HTTP 200, one visible `#main`, retained draw items, no JavaScript errors, page-ready no slower than two times Chromium load, and successful early-scroll acceptance where configured.
- The 64×64 mean-RGB perceptual comparison crops Breeze's browser chrome and rejects blank surfaces. Per-fixture ceilings in `alpha/matrix.json` were calibrated from the complete matrix and preserve margin for renderer/scale variation; they range from 0.12 to 0.28 on a normalized 0–1 scale.

Breeze page-ready is its first owned layout and paint. Chromium page-ready is `Page.loadEventFired`; its first-contentful-paint is recorded separately. Breeze scroll metrics measure owned repaint work, while Chromium scroll metrics measure two-animation-frame latency, so compare trends rather than treating unlike fields as identical instrumentation.

## Opt-in live evidence

The matrix also names representative public URLs for Wikipedia, a responsive blog, DuckDuckGo HTML results, and HTML5test. They are never used as deterministic CI gates. Each browser gets a 45-second live-process bound; failed samples are retained as diagnostics while the remaining targets continue. Successful pairs must still produce nonblank captures. To collect fresh hidden side-by-side evidence:

```powershell
.\benchmarks\run-alpha.ps1 -Live -Iterations 1
```

Live content, server behavior, anti-automation responses, fonts, and networks change independently of this repository. Live runs therefore retain captures and measurements for manual review but do not apply local visual or performance thresholds.

## Interpretation

Breeze owns HTML, CSS, layout, images/SVG/fonts, forms, a subset JavaScript runtime, and painting, but Chromium implements substantially more of the web platform. Canvas, media playback, accessibility, cross-site isolation, and broad standards coverage remain incomplete. Results are development evidence for the controlled paths above, not a universal claim that Breeze is faster than or compatible with Chromium.
