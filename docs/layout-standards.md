# Layout compatibility: measured standards slices

Updated 2026-09-13. Wikipedia is a regression example, not a source of special-case
selectors or layout rules. These changes do **not** establish pixel-perfect Wikipedia
rendering or complete implementation of CSS Grid, tables, buttons, or legacy collections.

## Implemented contracts

| Contract | Implementation and regression evidence |
| --- | --- |
| [CSSOM stylesheet owner order](https://drafts.csswg.org/cssom/#document-css-style-sheets) | Linked and inline sheets are cascaded in owner tree order, independently of response arrival. Moving/removing owners and changing media, type, rel, or disabled attributes invalidates the rule set. Repeated link owners share loaded bytes but retain separate cascade positions. Adopted sheets follow tree-owned sheets. Library-injected sheets remain explicitly unowned. |
| [CSS Overflow scroll containers](https://drafts.csswg.org/css-overflow-3/#scrollable) | Element-owned offsets, clipped descendants, classic scrollbar gutters, thumb/track input, cancelable wheel defaults and scroll chaining. Hidden overflow remains programmatically scrollable; clip overflow does not. Both axes can induce gutters, without changing the outer border box. |
| [CSSOM View element scrolling](https://drafts.csswg.org/cssom-view/#dom-element-scrolltop) | Synchronous clamped `scrollTop`/`scrollLeft`, `scroll`/`scrollTo`/`scrollBy`, client/scroll extents and coalesced non-bubbling scroll events. Offset geometry stays unscrolled; client rectangles and hit testing use visual offsets. |
| [Sticky positioning](https://drafts.csswg.org/css-position-3/#sticky-pos) | Block-path sticky boxes follow the nearest scrollport within their containing block. Native viewport scrolling updates retained paint offsets without relaying out the document; normal-flow and offset geometry remain unchanged. Tests cover direct and wrapped scroller children, hidden versus clip overflow, inset limits, repeated reversal, and renderer presentation without JavaScript. |
| [Adjoining block margins](https://www.w3.org/TR/CSS22/box.html#collapsing-margins) | Preserve positive/negative extrema through empty blocks and nested parent/child boundaries, with formatting-context and border/padding exclusions. Comments do not generate boxes or interrupt empty-block margin collapse. |
| [HTML button layout](https://html.spec.whatwg.org/multipage/rendering.html#button-layout) | Inline, block, and flex-item buttons share authored descendant layout, clipping and form metadata. Automatic inline size is fit-content, without a 70px native minimum; flex stretch reaches the same block layout. SVG/mask descendants paint as authored content. The default border-box sizing, text alignment and block content centering can be overridden by author CSS. |
| [Block content alignment](https://www.w3.org/TR/css-align-3/#distribution-block) | Positional `align-content`, explicit safe/unsafe overflow, and single-subject distribution fallbacks move the block's in-flow contents as a unit. Non-normal alignment establishes a formatting context. Cascade, CSS-wide keywords, computed-value serialization and layout invalidation retain the value. Baseline alignment, multiline flex/grid distribution and vertical writing modes are not covered by this slice. |
| [CSS Fonts family lists](https://drafts.csswg.org/css-fonts-4/#font-family-prop) | Preserve ordered families through cascade, inheritance, shorthand, resource discovery, and renderer font selection. Parse quoted names, commas, escapes, and generic-family identity with the existing cssparser dependency. A missing first family no longer discards the specified fallback fonts. |
| [Grid spanning contributions](https://www.w3.org/TR/css-grid-1/#algo-spanning-items) | Resolve non-spanning intrinsic row contributions before spans; spans crossing flexible rows contribute to the flex fraction instead of inflating title rows. Lay out stretched items using the resolved area height, including percentage descendants and observer/paint bounds. |
| [Table cell sizing](https://www.w3.org/TR/CSS22/tables.html#auto-table-layout) | Percentage width preferences cannot starve another cell's intrinsic content, padding, and borders. Cell boxes use the shared row height, preserving backgrounds, clips, and bounds through normal block layout. |
| [Web IDL indexed properties](https://webidl.spec.whatwg.org/#legacy-platform-object-getownproperty) | Unsupported HTMLCollection indices fall through to ordinary property lookup (`undefined` absent an inherited property), while `item()` still returns `null`. Property indices do not undergo `item()`'s unsigned-integer conversion. Tests cover sentinel iteration, prototype lookup, and live removal. |

Grid/table intrinsic block-size probes are isolated from published paint, hit-test, and
ResizeObserver state. Per-layout caches reuse nested intrinsic measurements; the final
layout publishes each node once. This is a correctness change, not a loading-speed claim.

The repository-owned `scroll-containers.html` fixture additionally checks direct and
wrapped sticky boxes after a 150px element scroll. Headless Chrome reports top positions
10px and 130px and a 185px content width inside 200px scrollers; layout regression tests
assert these values. Renderer-process tests exercise wheel cancellation, clipping-aware
clicks, scrollbar dragging and sticky viewport updates through the actual IPC path.
Scrollbar artwork is Breeze-owned, not a reproduction of the platform theme.
Its classic gutter rounds 15 CSS pixels up to a whole device pixel, matching the observed
Chrome gutter at 125% scaling. ResizeObserver preserves the resulting fractional content
size; integer CSSOM client sizes do not necessarily reconstruct that fraction. The owned
`scrollbar-resize.html` probe records both APIs without rounding the observer result.
At 125% scaling, upstream `resize-observer/scrollbars-2.html` fails in **both** browsers:
Breeze reports 84.800003px versus the test's integer-derived 85px; Chrome reports
84.796875px versus its integer-derived 86px. The pinned WPT run has 144 passing cases and
this one failure (752 assertions pass, one fails). The test is retained unchanged; fractional
observer geometry is not rounded to make the suite green.

Normal block backgrounds and borders now paint before floating descendants, followed by
inline content and nonnegative positioned groups, following
[CSS 2.2 Appendix E](https://www.w3.org/TR/CSS22/zindex.html#painting-order).
Paint ownership survives geometry reordering; clip groups stay balanced when split across
phases, and opacity groups remain atomic. The owned `paint-phases.html` fixture compares
the border/float overlap with Chrome. It also exposes a separate, still-unfixed absolute
shrink-to-fit width difference; matching paint order does not establish matching geometry.
Positioned descendants escaping float/auto-z-index pseudo-contexts remain a separate gap.

Viewport wheel defaults retain relative distance through IPC and output coalescing, then
use the browser's smooth-scroll target. They do not reuse a stale input-time absolute
offset. Tests cover queued events, fractional device-pixel accumulation, cancellation,
and an intervening absolute script scroll superseding earlier relative movements.

Stylesheet ownership is not a claim of complete CSSOM stylesheet support: imports,
alternate/preferred stylesheet sets, and CSSStyleSheet mutation/disabled reflection still
need separate standards slices.

These slices are not a complete CSSOM View/Overflow/Position implementation. RTL scroll
origins, viewport horizontal scrolling, inline sticky fragmentation, independent-axis
clipping and fixed-position escape from nested clips still require further coverage.

## Controlled Chrome comparison

The repository-owned [layout-contracts fixture](../benchmarks/alpha/fixtures/layout-contracts.html)
contains no third-party page content or dependencies. Hidden release Breeze and headless
Chrome 152.0.7977.83 produced these identical CSS-pixel bounds on 2026-09-13:

| Box | x, y | Width × height (both browsers) |
| --- | --- | --- |
| Heading | 20, 20 | 400 × 40 |
| Tabs | 20, 70 | 400 × 30 |
| Stretched article | 20, 110 | 400 × 510 |
| Spanning sidebar | 420, 20 | 200 × 600 |
| Block button | 20, 110 | 22 × 22 |
| Image cell | 20, 140 | 60 × 60 |
| Percentage text cell | 80, 140 | 340 × 60 |

Serve with `scripts/serve-alpha-fixtures.ps1` and use `scripts/run-hidden-benchmark.ps1`
for Breeze. The Chromium harness must retain `--headless`, `--mute-audio`, and
`CreateNoWindow`. Query `h1`, `nav`, `article`, `aside`, `button`, `.image`, and `.message`
with the harnesses' diagnostic-selector options. Captures and JSON reports stay under
ignored `target/wiki-regression/`, not in commits. The fixture's collection sentinel loop
also completes without a script error in both browsers.

## Live-page evidence and remaining gaps

On the reported Wikipedia article, the sidebar text overlap and notice/icon overlap are
removed, the serif heading uses its fallback font, and the artificial title-to-tabs gap
is removed. The previous `nodeType` exception also disappears. The left pane now has its
own scrollbar and sticky placement; the heading rule paints behind the floating infobox.
Unchecked appearance controls and typography/spacing differences remain visible.
Console diagnostics now expose missing PerformanceObserver
support; this is not proof that it explains every remaining initialization difference.

Match **media viewport width**, not content width, when comparing responsive pages.
Breeze's 1694 × 960 outer window on this 125%-DPI machine has a 1680px media viewport and
1663.2px content width after its scrollbar. Chrome's viewport option includes its scrollbar.
Setting Chrome to 1663px incorrectly selected the opposite side of Wikipedia's 1680px
breakpoint. At 1680px both use a 260px left sidebar and 948px article column; their native
scrollbar widths still differ. Live banners/content can also vary between requests.

Further slices must cover remaining positioned paint-context ordering, broader grid track
constraints and self-alignment, shared multi-row table columns/spans and vertical alignment,
checkable form controls, and the remaining page-initialization APIs. Native control artwork
remains approximate. The owned `button-alignment.html` fixture checks intrinsic inline
buttons, retained SVGs and positional block alignment independently of Wikipedia.
At 125% scaling its inline text button is 30.6875px wide and its icon button is 40px wide
in both engines, and all four block-alignment child rectangles match. Inline baselines do
not yet match: Breeze centers neighboring atomic inline boxes and expands short authored
line heights to font metrics. This is a separate inline-formatting gap, not passing evidence.
Do not replace these gaps with site-specific CSS or claim full conformance from this fixture.
