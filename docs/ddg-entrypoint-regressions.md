# DDG entrypoint regressions — September 19, 2026

The earlier child-context evidence started on a modern results URL. It did **not**
establish that the homepage or the default HTML search results worked. User captures
exposed both failures, and hidden release runs reproduced them.

## Engine changes

- DOM `Node.isEqualNode()` and `isSameNode()` use genuine native node identities,
  including same-origin foreign realms. Equality compares node-specific data,
  unordered attributes and ordered light-tree children without invoking author
  getters. `Attr` comparison uses private state, including attached value changes.
  Template contents and shadow trees are not ordinary element children.
- A native select remains a single control when CSS blockifies it. Inline and block
  projection share option labels, values, selected index and intrinsic sizing.
  Option descendants no longer participate in page layout or paint.
- Native editing bypasses author-installed `value` setters before dispatching the
  trusted input event. Calling those setters incorrectly made framework change
  trackers treat user typing as an already-observed programmatic write.
- Empty input-button values stay empty. Accessible names and tooltips no longer
  supply visual labels; the old Search-to-Go substitution is removed.
- Inline SVG `currentColor` is rasterized per element using computed colour, instead
  of tinting the entire bitmap. Explicit fills remain intact. Raster cache identity
  includes computed colours, so inherited/class changes update the image while
  unchanged refreshes reuse it.

These are general engine paths, with no DDG hostname, selector or response overrides.
Primary contracts: [DOM node equality](https://dom.spec.whatwg.org/#concept-node-equals),
[HTML select rendering](https://html.spec.whatwg.org/multipage/rendering.html#the-select-element-2),
[HTML input button labels](https://html.spec.whatwg.org/multipage/input.html#submit-button-state-(type=submit)),
and [SVG paint/currentColor](https://svgwg.org/svg2-draft/painting.html#SpecifyingPaint).

## Before/after evidence

Same-machine hidden release captures, fresh profiles; geometry is CSS pixels.
These are functional/visual checks, not a performance comparison.

| Check | Before | After |
|---|---|---|
| Homepage after 15 seconds | Application error; 583 logged console errors | Homepage; zero console errors |
| Homepage typing + Enter | Stayed on homepage; change tracker missed input | Requested query; ten result links; zero JS/console errors |
| HTML results region select height | 760.32 px | 41.68 px |
| HTML results first result top | 878.36 px, below viewport | 167.72 px, visible |
| SVG explicit fill mixed with currentColor | Entire image tinted | Explicit and computed colours preserved |
| Empty search submit value | Invented Go label over icon | No invented label |

`scripts/test-ddg-entrypoints.ps1` checks homepage load, homepage-to-search, HTML
results and a second HTML query. It requires visible results, correct submitted
query, compact native selects, no option layout boxes, and no JS/console errors.
An HTTP 200, a challenge page or an empty result page is not a pass. Inspect its
screenshots as well: the script is not a pixel-perfect compatibility oracle.
All four cases passed with fresh profiles on the follow-up release build. HTML
search uses POST, so its query is verified in the returned field and page title,
not incorrectly required in the URL.

`tests/fixtures/native-select-layout.html` provides an owned cross-browser check
for node/Attr equality, native dropdown geometry, tracked textarea editing and
mixed-colour SVG. Unit tests also cover block/flex/grid/positioned/float selects,
invalid equality operands, namespaces, node kinds, templates, shadow trees,
author-overridden accessors, colour invalidation and unchanged raster reuse.

## Validation and limits

- Library: 1,234 passed, one existing ignore; browser: 118 passed; WPT runner: 17 passed.
- Isolated renderer: 130 passed; live runtime: 82 passed, three existing ignores.
- Hostile input, parser conformance, media process and fullscreen: 22 passed.
- Pinned curated WPT: 397 files / 3,393 assertions passed, no regressions.
- Existing alpha visual/readiness/scroll gates: all 16 Breeze/Chrome pairs passed.
- Clippy with warnings denied, formatting and source-size checks passed.

The owned Chrome capture verifies equality, compact select layout and mixed SVG
colours. Native-editing acceptance is covered by Breeze runtime tests and the live
homepage submission; Chrome's navigation-only submission harness cannot finish a
non-navigating textarea fixture, and that timed-out attempt is not counted as a pass.

The live headless Chrome HTML-results attempt received a CAPTCHA. It was not
solved, and it is not counted as an equal-content reference or timing sample.
The owned fixture is the deterministic reference. Live pages can vary with service
responses, and broader visual compatibility remains unfinished. The HTML search
fallback remains enabled; these fixes do not certify all DDG routes or features.
Modern results still show visual differences, including missing result favicons
and oversized rich-result text. Native listbox/multiple-select rendering and full
SVG/CSS paint conformance are not established by this dropdown/currentColor slice.
