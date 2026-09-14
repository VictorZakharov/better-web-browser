# CSSOM View inline and text fragments

This slice replaces the missing `Element.getClientRects()` method and the zero-valued
Range geometry stubs with renderer-owned fragment snapshots. It follows
[CSSOM View element rectangles](https://drafts.csswg.org/cssom-view/#dom-element-getclientrects),
[Range rectangles](https://drafts.csswg.org/cssom-view/#dom-range-getclientrects), and
[Geometry Interfaces](https://drafts.fxtf.org/geometry/#DOMRect).

## Implemented contract

- Plain inline elements retain one font-band rectangle per laid-out line, in content
  order. A tall atomic child does not replace its parent's font ascent/descent band.
  Existing atomic/decorated boxes continue to report their border geometry.
- Range queries include selected element boxes and selected text descendants. Partial
  text selections use shaped advances, not glyph ink or the enclosing element's union.
  DOM UTF-16 offsets survive CSS whitespace collapsing, CRLF normalization, wrapping,
  surrogate pairs, and the shaper's typographic clusters. Collapsed boundaries produce
  zero-width caret rectangles when a text fragment exists at that boundary.
- Nested scrolling, sticky offsets, and supported layout translations use the same
  coordinate conversion as element bounds. No page-specific correction is applied.
- Hidden inline content still generates measurable geometry without painting. Detached
  and `display:none` content has no associated boxes. Selected `display:contents`
  wrappers contribute their selected child boxes rather than a synthetic wrapper.
- Returned `DOMRectList`/`DOMRect` values are branded snapshots. Mutating an old result
  cannot change layout; bounding queries use internal fragment collection, not an
  author's override of `getClientRects()`.
- Renderer-backed rectangle components use a uniform 1/64 CSS-pixel publication grid.
  This prevents binary32 accumulation noise from making adjacent fragment unions
  inconsistent. It is a compatibility precision choice, not a CSSOM rounding mandate;
  [Blink's LayoutUnit](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/geometry/layout_unit.h)
  uses the same layout precision. Script-created DOMRects retain double precision.
- DOM/style mutation checkpoints replace both bounds and fragments together. Intrinsic
  sizing probes do not retain fragments. Repeated reads reuse published geometry.

## Text pipeline and cost

The existing HarfRust pass now retains UTF-16 cluster offsets and advance rectangles.
Both measurement and paint caches retain that geometry, with its bytes included in
their existing bounded budgets. UTF-16 conversion is linear across fallback-font runs;
geometry reads do not rasterize glyphs or introduce a second layout algorithm. The
renderer shares immutable snapshots with its script runtime; fragment data is not
serialized over the browser presentation protocol.

The simple `TextMeasurer` default supports test/embedding measurers using grapheme-prefix
measurements. The production renderer and geometry-only wrapper override it with cached
OpenType geometry. Tests verify identical painted/measured advances and no new raster
assets or shaping on a cached geometry read.

## Validation

Eight unchanged files from the pinned WPT checkout cover detached elements, source
whitespace, rectangle-list lengths, atomic/inline descendants, nested selected text,
`display:contents`, and partial-surrogate offsets. These are added to the existing
all-pass gate, not rewritten or given expected-failure allowances.

Owned tests additionally exercise wrapped lines, partial and collapsed ranges,
translation, nested scrolling, mutation refresh, hidden boxes, non-breaking spaces,
snapshot independence, and author overrides. The original self-checking
[`client-rect-fragments.html`](../benchmarks/alpha/fixtures/client-rect-fragments.html)
can be served with `scripts/serve-alpha-fixtures.ps1` and captured by the existing
hidden Breeze/Chromium harnesses. It records rectangles in `#result` data attributes.

The final matching-viewport comparison at 125% scale passed in both engines: two wrapped
fragments, 28 CSS px scroll displacement, and a partial selection width of 26.69 CSS px in
Breeze versus 26.70 in Chrome. The first font band's top is 172 versus 170.4 CSS px, and
its height is 22.34 versus 22.4 CSS px: existing font/line placement is not pixel-identical.
These are geometry measurements, **not a loading-speed claim**.

The full curated gate passes 153 files / 765 subtests. The WPT runner now fixes its test
environment to 100% scale and rejects mismatching reports; border and scrollbar assertions
also differ in Chrome at the desktop's native 125% scale. Normal browsing retains native
DPI. See [the runner contract](../tests/wpt/README.md#run-the-suite).

The live Coron, Palawan check, Climate anchor activation, and ordinary Palawan link
navigation had no renderer failure or missing-rectangle exception; the climate-table
rows remained readable without overlap.
Separate PerformanceObserver and storage warnings remained at this checkpoint.
The subsequent [Performance Timeline slice](performance-timeline.md) removes the missing-observer
exception; the ResourceLoader storage warning remains separate.

## Remaining boundaries

This is not a whole-CSSOM-View conformance claim and does not close issue #85. The
formatter still needs separate work for inline continuations around block children,
decorated inline fragmentation, independent table/caption box lists, explicit `br`
rectangles, vertical writing, and more complete bidi/transform geometry. The API does
not manufacture those missing layout fragments by duplicating a bounding rectangle.
DOM Range's other operations and live boundary-point adjustment are also separate
contracts; this change does not claim to complete them.
