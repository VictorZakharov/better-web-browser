# ADR 0003: Lean renderer text pipeline

- Status: Accepted
- Date: 2026-08-18
- Issue: [#60](https://github.com/VictorZakharov/better-web-browser/issues/60)
- Supersedes: [ADR 0002](0002-renderer-owned-text-stack.md)'s COSMIC Text implementation

## Context

ADR 0002 moved all document-font discovery, fallback, shaping, and rasterization into the
capability-free renderer. That security boundary worked, fixed the earlier measurement/painting
mismatch, and preserved complex scripts, but the selected COSMIC Text adapter regressed the frozen
Wikipedia page-ready median by 23.8%, non-network time by 44.0%, and layout/paint by 103.6% against
the same-day GDI control.

A temporary probe attributed about 277 ms to text calls: roughly 102 ms across hundreds of
thousands of outer-cache hits and 175 ms across about 2,664 cold misses. COSMIC's own cache did not
remove the cold cost. The goal is to recover that time without moving hostile font bytes or page
text into the privileged browser and without replacing advanced shaping with an ASCII-only path.

## Decision

Keep Breeze's bounded renderer-owned shape and glyph caches, but replace the general-purpose
COSMIC Text coordinator with three focused adapters:

- [Fontique 0.11.1](https://github.com/linebender/parley/tree/main/fontique) supplies lazy system
  font discovery, ordered CSS-family queries, character coverage, fallback, synthesis metadata,
  and in-memory webfont registration.
- [HarfRust 0.5.2](https://github.com/harfbuzz/harfrust) performs OpenType shaping with cached
  per-face `ShaperData`, reusable input buffers, script and direction properties, and variable-font
  instances.
- [Swash 0.2.10](https://docs.rs/swash/0.2.10/swash/scale/) performs hinted outline, color-outline,
  and color-bitmap rasterization through one persistent `ScaleContext`.

Breeze owns the browser-specific orchestration in small modules: Unicode bidi runs, grapheme-safe
fallback boundaries, common-script resolution, CSS spacing, stable cache keys, quarter-pixel
quantization, resource epochs, and all hostile-input budgets. It does not reimplement Unicode
bidirectional processing, grapheme segmentation, OpenType shaping, or font rasterization. Those
algorithms are difficult to reproduce correctly and already have maintained Rust implementations.

The renderer continues to emit only bounded glyph placements and premultiplied BGRA or alpha-mask
rasters. The browser validates and composites them; it neither receives remote font bytes nor
measures document text.

## Compatibility contract

The implementation keeps advanced shaping for every document string. There is no Latin fast path
and no fallback to GDI. Font selection occurs on extended grapheme clusters, adjacent clusters with
the same selected face and script are shaped together, and bidirectional visual runs preserve their
direction. This is required for contextual Arabic forms, Indic shaping, combining sequences,
ligatures, emoji ZWJ sequences, and mixed-direction content.

The implementation follows:

- [CSS Fonts Module Level 4](https://www.w3.org/TR/css-fonts-4/) for ordered family selection,
  matching, synthesis, and cluster fallback;
- [CSS Text Module Level 3](https://www.w3.org/TR/css-text-3/) for letter and word spacing;
- [Unicode Standard Annex #9](https://www.unicode.org/reports/tr9/) for bidirectional layout; and
- [Unicode Standard Annex #29](https://www.unicode.org/reports/tr29/) for grapheme boundaries.

Known upstream limits remain visible. HarfRust 0.5.2 does not implement Graphite, deprecated Apple
`mort`, or HarfBuzz's Arabic fallback shaper for fonts that lack normal OpenType Arabic tables.
Malformed fonts return an error rather than using HarfBuzz's dummy shaper. Breeze therefore treats
shape/raster failure as a missing glyph, under the same renderer isolation and resource budgets;
broader font-format and CSS Fonts conformance remains future work.

## Performance evidence

All page measurements used a release build, a hidden 1,920 by 1,080 window, 100 ms settle, and
`https://en.wikipedia.org/wiki/2026_East_Nusa_Tenggara_earthquake`. Breeze was launched only through
`scripts/run-hidden-benchmark.ps1`, which verifies `--benchmark` in the child command line and uses
`CreateNoWindow`.

### Before and after

The GDI and COSMIC rows are the frozen three-run medians from ADR 0002. The lean row is the final
three consecutive instrumented runs.

| Backend (3-run median) | Page ready | Non-network | Layout/paint | Scroll frame median / p95 | Worst input | Working set |
|---|---:|---:|---:|---:|---:|---:|
| GDI control | 576.915 ms | 223.193 ms | 105.540 ms | 6.817 / 10.467 ms | 14.772 ms | 155.7 MiB |
| COSMIC Text | 714.472 ms | 321.404 ms | 214.886 ms | 1.797 / 2.703 ms | 9.640 ms | 179.668 MiB |
| Lean Fontique/HarfRust/Swash | 534.731 ms | 226.830 ms | 131.399 ms | 1.995 / 3.087 ms | 10.937 ms | 183.672 MiB |

Against COSMIC, the selected implementation is 25.2% faster to page-ready, 29.4% faster outside
the network, and 38.9% faster in layout/paint. Against the GDI control it is 7.3% faster to
page-ready and 1.6% slower outside the network, satisfying issue #60's 10% allowance. Layout/paint
remains 24.5% slower than GDI and the working set remains 18.0% higher. The pre- and post-retained
scroll measurements are not identical paint paths, as ADR 0002 explains; all final lean runs use
the corrected retained-strip path.

### Final runs

| Metric | Run 1 | Run 2 | Run 3 | Median |
|---|---:|---:|---:|---:|
| Page ready | 534.731 ms | 518.116 ms | 686.145 ms | 534.731 ms |
| Network | 307.901 ms | 307.122 ms | 422.219 ms | 307.901 ms |
| Non-network | 226.830 ms | 210.994 ms | 263.926 ms | 226.830 ms |
| Layout/paint | 143.301 ms | 125.166 ms | 131.399 ms | 131.399 ms |
| Scroll frame median | 1.967 ms | 2.034 ms | 1.995 ms | 1.995 ms |
| Scroll frame p95 | 2.973 ms | 3.087 ms | 3.332 ms | 3.087 ms |
| Worst input latency | 9.361 ms | 10.937 ms | 11.568 ms | 10.937 ms |
| Working set | 182.977 MiB | 183.672 MiB | 184.688 MiB | 183.672 MiB |

All three reached the stable scroll range at 256 ms and passed the acceptance thresholds.

The benchmark now retains per-stage timing so future work does not require a temporary probe. The
following medians are totals across the benchmark's initial and post-load presentations, not an
additive decomposition of page-ready:

| Stage | Final median |
|---|---:|
| Font catalog work | 1.552 ms |
| Font selection | 10.233 ms |
| OpenType shaping | 8.897 ms |
| Glyph rasterization | 17.198 ms |
| Presentation encoding | 53.486 ms |
| Presentation decoding | 122.034 ms |
| Browser presentation installation | 255.768 ms |

Text selection, shaping, and rasterization now total about 38 ms. Presentation transport and
installation are the larger measured follow-up opportunity.

### Intermediate and outlier evidence

The first uninstrumented direct-backend run had cold network and scheduler interference: 831.033 ms
page-ready, 252.638 ms non-network, 82.733 ms worst input latency, and a failed early-scroll trace.
The next three runs were green with a 547.269 ms page-ready median, 228.762 ms non-network median,
and 1.916 / 2.842 ms scroll median/p95.

After permanent telemetry was added, six audit runs were captured. Five passed. One isolated run
reported 111.806 ms maximum input latency and 2,208 ms to smooth despite zero scrolling-only style
or layout rebuilds; the immediately following three-run batch above passed. The outlier is retained
here rather than omitted from the performance record.

The release executable changed as follows:

| Backend | Executable | Delta from GDI |
|---|---:|---:|
| GDI control | 15,661,568 B | baseline |
| COSMIC Text | 17,833,472 B | +13.87% |
| Lean pipeline plus telemetry | 17,717,248 B | +13.13% |

The lean executable is 116,224 B (0.65%) smaller than the COSMIC build. An incremental optimized
rebuild after the final source changes took 115.5 seconds on the development machine; this is a full
application link measurement, not a clean dependency-only comparison.

## Alternatives considered

- **Keep optimizing COSMIC Text.** Reusing its buffer, bypassing its line cache, disabling its
  inner shape cache, and allocation-focused cache-hit changes were measured in ADR 0002. None
  recovered the cold cost.
- **Use Parley as the top-level layout engine.** The earlier spike shaped quickly, but Breeze would
  still need a rasterizer and ownership integration. The selected adapter uses Fontique from the
  Parley project directly and keeps Breeze's existing line/layout ownership.
- **Lazy viewport-only glyph rasterization.** This complicates immutable presentation IPC and can
  expose missing glyphs during immediate scroll. Direct rasterization is only about 17 ms across
  the measured run, so the complexity is not justified by the current profile.
- **Parallel shaping/rasterization.** Font queries, shaping data, and Swash contexts are stateful;
  safe parallelism would require per-worker caches and deterministic merge/budget policy. With
  measured text work below 40 ms and presentation installation substantially larger, that is not
  the next bottleneck.
- **Implement OpenType and Unicode algorithms locally.** Rejected. It would turn a performance
  adapter into a new font engine, substantially increasing compatibility and hostile-input risk.

## Consequences

- The renderer security boundary and bounded glyph protocol remain unchanged.
- Breeze owns less general-purpose text-layout machinery and can profile each cold stage directly.
- Complex-script behavior continues through a maintained HarfBuzz-derived implementation.
- Cold page readiness meets the issue target; scroll responsiveness remains green.
- Memory and layout time are still above the original GDI control and remain visible tradeoffs.
- Presentation encoding, decoding, and browser installation are now separately measurable.

## Verification contract

Tests cover deterministic Latin, Arabic/RTL, Devanagari, combining-mark, ligature, emoji, and
mixed-direction geometry; Arabic joining changes; canonical composed/decomposed geometry; CSS
spacing; renderer-owned webfont aliases; glyph epochs; checked protocol round trips; and the real
AppContainer renderer. The renderer-process suite also retains crash, hang, malformed-IPC, and
native-fault containment coverage.

## Dependency provenance

The direct crates are resolved through Cargo and are not vendored. Fontique and Swash are
Apache-2.0 OR MIT, HarfRust is MIT, and the Unicode bidi, script, and segmentation crates are MIT OR
Apache-2.0. Exact versions and the transitive graph are locked in `Cargo.lock`; direct upstreams and
licenses are also recorded in `THIRD_PARTY_NOTICES.md`.
