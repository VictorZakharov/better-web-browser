# DOMParser and hidden-removal reassessment — September 16, 2026

This follows the [bootstrap assessment](browser-performance-bootstrap-2026-09-16.md).
The [detached parsing contract](detached-document-parsing.md) records platform
coverage, dependency licenses, and remaining XML/SVG limitations.

## What changed and why

The previous release lacked DOMParser. This slice implements engine-owned HTML
and XML parsing rather than substituting behavior for a particular site. The
missing-parser error disappears in all three modern DuckDuckGo runs; its existing
`Invalid scheme` error and incomplete shell remain. Neither browser's capture
establishes working modern search: Chrome receives a bot challenge. No challenge
bypass, provider change, watchdog increase, or site-specific patch was used.

The Wikipedia profile also showed full-page layout after completed scripts were
removed from a hidden head. Style refresh reported no computed-style changes,
but the blanket `removed_styles != 0` guard forced layout anyway. Removed subtrees
are now inspected before their rendering dependencies become unreachable. Geometry
reuse requires known-local removals, refreshed unchanged styles, surviving roots
under `display:none`, and no full style rebuild. Unknown removals, visible roots,
fullscreen content, base elements, slots, and foreign-namespace resource definitions
retain the conservative layout path. Independent image/font/resource changes
still rebuild. Title/accessibility/presentation updates are not suppressed.

This follows [CSS Display's box-generation rule](https://drafts.csswg.org/css-display/#valdef-display-none),
not a heuristic based on zero opacity or offscreen position. Owned tests cover
dependency tracking and coalescing, geometry-publication checkpoints, hidden
removals, revealing content, visible roots, slot redistribution into visible content,
and metadata publication without new
text measurements. Diagnostics now include bounded dirty-root tags and removal
provenance so that the reason can be inspected in real-page traces.

## Measurement conditions

- Before: merged #165 implementation, main `57502c2`; preserved release SHA-256
  `7C03077D130612059C1D3E2FF940147954D97B4C67BA0E755415C8B66E14434D`.
  After: implementation through `076aea9`, canonical release SHA-256
  `1066A43CEDFF09C59F3861D3BB4FCF979BA79E0C55F3F8F68FB5B3F6DEC39E7E`.
  Later changes only adjust test fixtures, tests, and documentation, not the
  measured browser implementation.
- Windows 10.0.26200, Ryzen 9 5900X, 24 logical processors, Chrome 153.0.8010.47.
  Hidden/silent Breeze and unified-headless muted Chrome, fresh temporary profiles,
  no concurrent builds or test suites, OS caches not flushed.
- Interleaved before/after/Chrome, reversing order on alternating trials: five per
  browser on Coron and three on Main Page and modern DuckDuckGo. Requested Breeze
  window 1280 x 720, content approximately 1248.8 x 548 CSS px; Chrome requested
  1249 x 548 (reported 1250 x 548 after device-scale rounding),
  125% scale, en-US, five-second settle, eight scroll samples. Coron also runs the
  existing six-second early-scroll trace.
- First presentation is process-relative and not full-page readiness. Chrome load
  includes debugger/harness setup; FCP is navigation-relative. Hidden-window and
  debugger-ready times are not equivalent interactive-startup milestones.
- Memory sums working/private bytes over process trees (median 2 Breeze / 10
  Chrome processes; individual Chrome samples vary). Shared pages can count twice.
  CPU is cumulative through capture/scroll completion; feature coverage and
  observation windows differ. This is not an equal-work comparison or leak test.

## Before / after: medians of individual runs

| Measurement | Before | After | Interpretation |
|---|---:|---:|---|
| Coron first presentation | 568.1 ms | 615.0 ms | Increased, not a complete-load improvement |
| Coron style work | 599.8 ms | 598.2 ms | Essentially unchanged |
| Coron layout/paint work | 1,272.5 ms | 1,163.9 ms | 8.5% lower observed median |
| Coron cumulative CPU | 8,375.0 ms | 8,328.1 ms | Essentially unchanged |
| Coron working / private memory | 207.9 / 174.1 MiB | 210.9 / 177.4 MiB | Small increase |
| Coron early-scroll input-to-paint p95 | 5.963 ms | 5.159 ms | All five before and five after traces pass |
| Main Page first presentation | 602.7 ms | 588.4 ms | Small difference |
| Main Page style work | 271.1 ms | 304.6 ms | Increased, not a style-wide win |
| Main Page layout/paint work | 362.7 ms | 361.2 ms | Essentially unchanged |
| Main Page cumulative CPU | 1,312.5 ms | 1,531.3 ms | Increased; no consistent overall CPU win |
| Main Page working / private memory | 161.5 / 125.2 MiB | 160.5 / 125.4 MiB | Essentially unchanged |
| DDG missing-DOMParser error | 3/3 runs | 0/3 runs | Confirmed API blocker removed |
| DDG Invalid-scheme error | 3/3 runs | 3/3 runs | Existing remaining blocker, not fixed here |

Live content, script batches, and response ordering vary. The direct causal proof
is narrower than the aggregate medians: in matched Coron trial 1, the head cleanup
after `ext.cite.referencePreviews`/`ext.math.popup`/`ext.popups.main` reports 0/65
styles changed and drops from **56.556 ms layout to 0.006 ms**. The after trace
identifies `head`, two removed styles, and verified-local removals. Style work
continues (5.514 to 4.618 ms). Other head checkpoints retain layout when concurrent
resources require it. Do not attribute the entire aggregate saving to one skipped
checkpoint, or extrapolate these samples into a universal speedup. The earlier
series under `paired` measured 16.8% lower Coron layout/paint and 15.5% lower CPU;
the final series above did not reproduce that magnitude. Main Page style/CPU
increased in both series. No broad page-load or CPU improvement is claimed.

After-change Coron scroll-only style/layout rebuilds stay at zero; time-to-smooth
is 256 ms in every sample, and the maximum sampled input-to-paint latency is
15.019 ms (before: 12.605 ms). All eight after-change Wikipedia runs report HTTP 200 and no JavaScript
errors. Inspected captures preserve readable article columns, infobox, sidebar
scrollbar, and appearance controls. Typography/spacing remain different from
Chrome; this is not a new pixel-parity claim.

## Fresh Chrome reference

| Page | Breeze first presentation | Chrome load / FCP | Working set B/C | Private B/C | CPU B/C |
|---|---:|---:|---:|---:|---:|
| Wikipedia Main Page | 588.4 ms | 648.1 / 228.5 ms | 160.5 / 645.4 MiB | 125.4 / 400.5 MiB | 1,531.3 / 4,375.0 ms |
| Coron, Palawan | 615.0 ms | 718.2 / 259.6 ms | 210.9 / 678.7 MiB | 177.4 / 431.7 MiB | 8,328.1 / 6,562.5 ms |

Hidden-window creation is 9.1/8.9 ms for Breeze (Main/Coron), versus 226.8/230.4 ms
for Chrome debugger readiness. Chrome's Coron style/layout medians are 40.5/80.9 ms;
Breeze still spends 598.2/1,163.9 ms in style and layout/paint. Repeated full-page
work remains a clear profiling target; the new reuse path does not solve it all.

## Evidence and reproduction

Final library tests: 1,152 passed, one ignored. Renderer-process integration:
128 passed. Live-runtime integration: 82 passed, three ignored. The curated WPT
run passes all 369 files / 2,895 assertions, including eight new parser files /
69 assertions. Clippy with warnings denied, formatting, and source-size checks
pass; the 20 existing source-size target warnings remain below their frozen
ceilings. No WPT source or expected result was weakened.

The final 14-fixture alpha comparison completes **42 accepted Breeze/Chrome
pairs** (three per fixture), with all image, readiness, and scrolling gates passing.
The first nine accepted pairs are under `alpha-verified`. The next Chrome run
rendered successfully but failed temporary-profile cleanup because Windows held
`journal.baj` open; that rejected run is preserved, not counted as a pass. The
remaining eleven fixtures were restarted under `alpha-resumed`, producing 33
accepted pairs and a complete partial-matrix report. The parser fixture and Coron
captures were also visually inspected. HTML parser conformance: four tests pass.

Raw final JSON and original PNGs remain under ignored `target/domparser-proof/paired-final`:
`coron-{1..5}-{before,after,chrome}` and `{main,ddg}-{1..3}-{before,after,chrome}`.
`target/domparser-proof/compare.ps1` records the exact local run orchestration.
No captured third-party pages, screenshots, or temporary probes are committed.

The original alpha attempt is retained under `target/domparser-proof/alpha`. Its
numeric image threshold passed even though the DOMParser fixture's SVG text was
missing. Visual inspection rejected that as text-rendering proof. The existing SVG
text limitation is now explicit, and the final fixture separately exercises SVG
geometry and actual CDATA text painting in HTML, backed by a renderer-process
display-list assertion. The parser implementation is unchanged by that adjustment.

Reproduce the checked-in matrix with:

```powershell
./benchmarks/run-alpha.ps1 -SkipBuild -Iterations 3
./scripts/run-wpt.ps1 -WptRoot G:/Git/wpt-breeze -SkipBuild -Jobs 4
```

Use current release Breeze/Chromium harness builds and a WPT checkout prepared
from `tests/wpt/manifest.json`; see the [benchmark instructions](../benchmarks/README.md).
