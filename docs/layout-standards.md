# Layout compatibility: measured standards slices

Updated 2026-09-14. Wikipedia is a regression example, not a source of special-case
selectors or layout rules. These changes do **not** establish pixel-perfect Wikipedia
rendering or complete implementation of CSS Grid, tables, buttons, or legacy collections.

## Implemented contracts

Computed `line-height` retains its inheritance kind: unitless multipliers scale with a
child's font size; lengths and percentages inherit absolute values. Font sizing resolves
before the used line height, without reordering shorthand/longhand declarations. Short
and zero line heights allow negative leading instead of expanding to glyph height.
`normal` remains a 1.2em approximation, not font-specific typographic metrics.
In the owned `line-heights.html` Chrome comparison, all six CSS-pixel rectangles match:
inherited-number height 60px, inherited percentage and length heights 30px, two short
lines totaling 16px, and shorthand/longhand cases of 30px and 40px. On the live article,
the notice height changes from 67.6px to 88px versus Chrome's 90.8px, with matching top
positions. The remaining difference is not represented as a pass.

| Contract | Implementation and regression evidence |
| --- | --- |
| [CSSOM stylesheet owner order](https://drafts.csswg.org/cssom/#document-css-style-sheets) | Linked and inline sheets are cascaded in owner tree order, independently of response arrival. Moving/removing owners and changing media, type, rel, or disabled attributes invalidates the rule set. Repeated link owners share loaded bytes but retain separate cascade positions. Adopted sheets follow tree-owned sheets. Library-injected sheets remain explicitly unowned. |
| [CSS Overflow scroll containers](https://drafts.csswg.org/css-overflow-3/#scrollable) | Element-owned offsets, clipped descendants, classic scrollbar gutters, thumb/track input, cancelable wheel defaults and scroll chaining. Hidden overflow remains programmatically scrollable; clip overflow does not. Both axes can induce gutters, without changing the outer border box. |
| [CSSOM View element scrolling](https://drafts.csswg.org/cssom-view/#dom-element-scrolltop) | Synchronous clamped `scrollTop`/`scrollLeft`, `scroll`/`scrollTo`/`scrollBy`, client/scroll extents and coalesced non-bubbling scroll events. Offset geometry stays unscrolled; client rectangles and hit testing use visual offsets. |
| [Sticky positioning](https://drafts.csswg.org/css-position-3/#sticky-pos) | Block-path sticky boxes follow the nearest scrollport within their containing block. Native viewport scrolling updates retained paint offsets without relaying out the document; normal-flow and offset geometry remain unchanged. Tests cover direct and wrapped scroller children, hidden versus clip overflow, inset limits, repeated reversal, and renderer presentation without JavaScript. |
| [Adjoining block margins](https://www.w3.org/TR/CSS22/box.html#collapsing-margins) | Preserve positive/negative extrema through empty blocks and nested parent/child boundaries, with formatting-context and border/padding exclusions. Comments do not generate boxes or interrupt empty-block margin collapse. |
| [HTML button layout](https://html.spec.whatwg.org/multipage/rendering.html#button-layout) | Inline, block, and flex-item buttons share authored descendant layout, clipping and form metadata. Automatic inline size is fit-content, without a 70px native minimum; flex stretch reaches the same block layout. SVG/mask descendants paint as authored content. The default border-box sizing, text alignment and block content centering can be overridden by author CSS. |
| [SVG 2 auto sizing](https://svgwg.org/svg2-draft/geometry.html#Sizing) | An inline or block `<svg>` with automatic width/height uses its containing block's definite dimensions; a viewBox supplies aspect ratio when height is indefinite, not CSS pixel dimensions from its rasterized user-unit grid. A 24×24 flex item containing a 960-unit viewBox now paints a 24×24 SVG, matching the owned Chromium fixture rather than the former 960×960 box. |
| [Block content alignment](https://www.w3.org/TR/css-align-3/#distribution-block) | Positional `align-content`, explicit safe/unsafe overflow, and single-subject distribution fallbacks move the block's in-flow contents as a unit. Non-normal alignment establishes a formatting context. Cascade, CSS-wide keywords, computed-value serialization and layout invalidation retain the value. Baseline alignment, multiline flex/grid distribution and vertical writing modes are not covered by this slice. |
| [CSS Fonts family lists](https://drafts.csswg.org/css-fonts-4/#font-family-prop) | Preserve ordered families through cascade, inheritance, shorthand, resource discovery, and renderer font selection. Parse quoted names, commas, escapes, and generic-family identity with the existing cssparser dependency. A missing first family no longer discards the specified fallback fonts. |
| [CSS Fonts face weight ranges](https://drafts.csswg.org/css-fonts-4/#font-matching-algorithm) | Parse inclusive, reversible, fractional `@font-face font-weight` descriptor ranges. A face containing the requested weight wins before directional fallback matching; the loaded face is registered at the selected weight. Neutral fixture tests cover overlapping regular/medium/bold ranges and the 400–500 special search order. [WOFF2 decoding and Font Loading APIs](resource-loading-fonts-responsive-forms.md) were added later; variable-font axis interpolation remains open. |
| [Grid spanning contributions](https://www.w3.org/TR/css-grid-1/#algo-spanning-items) | Resolve non-spanning intrinsic row contributions before spans; spans crossing flexible rows contribute to the flex fraction instead of inflating title rows. Lay out stretched items using the resolved area height, including percentage descendants and observer/paint bounds. |
| [Table cell sizing](https://www.w3.org/TR/CSS22/tables.html#auto-table-layout) | Percentage width preferences cannot starve another cell's intrinsic content, padding, and borders. Cell boxes use the shared row height, preserving backgrounds, clips, and bounds through normal block layout. |
| [Web IDL indexed properties](https://webidl.spec.whatwg.org/#legacy-platform-object-getownproperty) | Unsupported HTMLCollection indices fall through to ordinary property lookup (`undefined` absent an inherited property), while `item()` still returns `null`. Property indices do not undergo `item()`'s unsigned-integer conversion. Tests cover sentinel iteration, prototype lookup, and live removal. |

Grid/table intrinsic block-size probes are isolated from published paint, hit-test, and
ResizeObserver state. Per-layout caches reuse nested intrinsic measurements; the final
layout publishes each node once. This is a correctness change, not a loading-speed claim.

### Hyperlink activation and section navigation (2026-09-14)

Browsing acceptance caught a contents link selecting its label without moving the article.
Hyperlink default actions now resolve the nearest DOM anchor after cancelable input
dispatch; they no longer depend on a link-bearing text paint command. Block anchors,
padding, images and nested children therefore retain their link action. Modified clicks
still request another tab, rather than scrolling the current document.

Same-document fragment links and `location.hash`/fragment `assign`/`replace` update the
retained realm, URL/history and scroll position without a network reload. Fragment lookup
tries literal IDs/named anchors before percent-decoded UTF-8, preserves literal `+`, and
handles empty fragments, `top`, missing targets and nested scrollports. `hashchange`
events carry old/new URLs and are queued after the synchronous update. Reader and page
URL snapshots remain consistent at the renderer protocol boundary. Generic retained-realm
and isolated-renderer tests cover these contracts against the
[HTML fragment algorithms](https://html.spec.whatwg.org/multipage/browsing-the-web.html#scroll-to-the-fragment).

The live check also exposed missing geometry for unadorned inline headings. Layout now
retains text-source bounds separately from hyperlink interaction IDs and aggregates their
inline ancestor bounds after final positioning. Painted and geometry-only layouts publish
the same multi-line union, including nested formatting and transformed ancestors. Existing
atomic/decorated boxes and containing blocks keep their own geometry rather than expanding
to include overflowing descendants. This supplies both DOM rectangle queries and section
scrolling. The subsequent [fragment-geometry slice](cssom-fragment-geometry.md) adds
per-line `getClientRects()` and partial text Range rectangles.

This is not complete navigation conformance: initial cross-document fragment scrolling,
same-document Back/Forward restoration, target-focus/`:target` styling, ancestor revealing,
scroll margins/padding and vertical-writing-mode alignment remain separate work.

### Intrinsic grid columns and browsing acceptance (2026-09-14)

The Main Page acceptance capture exposed an off-screen sidebar: the grid allocator
treated `min-content` as empty `auto`, then assigned all space to the adjacent `fr`
track. Grid parsing now retains `min-content` and `max-content` separately. Intrinsic
item contributions establish content-sized tracks before finite growth limits and
fractional expansion are resolved. `minmax(0, <length>)` retains a shrinkable base,
and flexible tracks whose minimum exceeds their share freeze before redistribution.
Spanning headings crossing a flexible track do not inflate the neighboring intrinsic
sidebar. Generic regressions cover each case, with painted/geometry-only parity.
This is the [CSS Grid intrinsic-track sizing](https://www.w3.org/TR/css-grid-1/#algo-content)
slice, not a claim of complete Grid track sizing, auto-placement, or writing-mode support.

### Table cell minimum heights and post-load scrolling (2026-09-14)

Under [CSS 2.2 table height layout](https://www.w3.org/TR/CSS22/tables.html#height-layout),
a cell's specified `height` is a minimum, not a fixed content cap. Intrinsic probes and
final layout now retain the natural content height even when the authored height is
smaller. Shared row backgrounds, border boxes, ResizeObserver boxes and subsequent row
positions use that expanded height. Ordinary block boxes keep fixed-height behavior.
Regression tests cover wrapped text, explicit line breaks, taller specified cells,
shared row heights and geometry-only/painted-layout parity.

The reported Coron, Palawan climate table reproduced overlapping labels with its authored
`height:16px`. Hidden captures before and after this correction show the overlap removed.
The subsequent shared-grid correction replaces per-row width allocation. All rows use
one set of min/max-content column constraints; column-spanning headers and footers
contribute to those tracks, and row spans reserve slots within their row group. Zero
row spans extend through the group. Span parsing follows HTML's integer-prefix rules
and limits; occupancy storage is linear in columns rather than rows times columns.
Automatic table widths shrink to max-content when it fits and never undercut min-content.
Percentage preferences cannot starve other tracks' intrinsic minimums.

Cell-content intrinsic measurements now retain an indefinite percentage basis. A
percentage-width block or nested table cannot inflate the track that will supply its
own width; cyclic preferred/max sizes act as their initial values and cyclic minima
as zero during that measurement, following
[CSS Sizing 3 section 5.2.1](https://www.w3.org/TR/css-sizing-3/#cyclic-percentage-contribution).
The normal layout pass still resolves those percentages against the allocated cell.
The cache distinguishes an indefinite basis from a definite zero. Regression tests
cover percentage blocks, mixed `calc()` widths and nested percentage tables with
cell padding; the live symptom was infobox bars extending beyond their table border.
Automatic margins on non-replaced inline content resolve to zero rather than creating
an unbreakable decorated box. This preserves wrapping in centered table footnotes.
Explicit CSS `table-row` and `table-cell` boxes on ordinary elements now join the
same grid as HTML rows/cells; HTML spanning attributes are not applied to arbitrary
elements. This restores nested flag/seal rows without changing the page markup.

Table-cell `vertical-align: top/middle/bottom` now moves in-flow paint and descendant
geometry together without shifting cell backgrounds or resize boxes. HTML row-group
defaults and row/cell UA inheritance, author declarations, CSS-wide values, CSSOM
serialization and layout invalidation retain that alignment. Baseline matching, inline
vertical-align shifts, column/column-group constraints and
complete collapsed-border conflict resolution remain outside this slice.

### Atomic inline boxes and anonymous CSS tables (2026-09-14)

The broader browsing sample found two additional blockers after the climate-table
repair: Rust's infobox expanded across the article, and CSS-table figures on the
periodic-table page disappeared. These are independent engine defects, not page rules.

Inline-block/inline-flex contents now use the shared block formatter for intrinsic
contributions, shrink-to-fit sizing, wrapping, block children and painted geometry.
Adjacent atomic boxes have soft wrapping opportunities without requiring literal
spaces; boundary wrapping uses the surrounding common ancestor's `white-space`,
not an individual inline-block's internal wrapping policy. Ordinary decorated inline
boxes do not acquire atomic wrapping opportunities. Explicit word/joining controls
and common nonbreaking characters suppress adjacent breaks; this is not a complete
Unicode line-breaking implementation or inline baseline-alignment claim.

CSS table captions, row/header/footer groups, columns and inline-table display roles
are retained by the cascade. A per-layout box tree generates missing tables, rows
and cells around consecutive content, suppresses irrelevant table whitespace, and
flattens `display:contents` without mutating the DOM or its node allocation budget.
Synthetic box identities are removed from published geometry; real descendant
geometry follows the normalized subtree during translations. Columns still lack
complete sizing/background propagation and header/footer reordering is not yet covered.
Owned regressions compare painted and geometry-only output, including DOM identity,
mixed cells, captions, translated descendants and float exclusion widths.

Responsive images preserve distinct min/max-content contributions for cyclic
percentage width constraints: a percentage maximum can compress the minimum, but
does not erase the preferred image width. HTML presentational hints enter before
author declarations, allowing explicit CSS `height:auto` to override an HTML height
attribute and preserve the decoded image ratio. Both inline and block image paths
are tested. These contracts follow [CSS table fixup](https://www.w3.org/TR/CSS22/tables.html#anonymous-boxes),
[atomic inline wrapping](https://www.w3.org/TR/css-text-3/#line-break-details), and
[cyclic percentage contributions](https://www.w3.org/TR/css-sizing-3/#cyclic-percentage-contribution).

Fresh release captures of the expanded browsing sample have now been reviewed;
the named coverage and its limits are recorded below. Unit-test success alone is
not visual acceptance.

### Responsive menus: insertion, positioned sizing and paint order (2026-09-14)

Opening the contents menu in a narrow viewport exposed additional generic failures.
`insertAdjacentElement` now implements all four positions through the normal DOM
pre-insertion path, preserving identity, adoption, mutation records, live collections
and custom-element reactions. Invalid arguments/positions and hierarchy violations
fail before tree mutation. `insertAdjacentText` creates literal text in the receiver's
document; it no longer serializes/reparses existing children. Tests cover disconnected
receivers, fragment parents and case-insensitive positions under the
[DOM insert-adjacent algorithm](https://dom.spec.whatwg.org/#insert-adjacent).

Automatic non-replaced absolute widths shrink to fit unless both horizontal insets
are definite. The shared min/max constraint pass also applies to the stretched case;
fixed boxes use the viewport percentage basis. Intrinsic content can exceed a narrow
trigger's containing block without being clipped to the trigger width. This implements
the [CSS 2.2 automatic positioned-width rules](https://www.w3.org/TR/CSS22/visudet.html#abs-non-replaced-width),
not intrinsic `width:min-content`/`max-content` keyword parsing or complete static-position
and bidi alignment. The observed menu uses the supported min/max constraints after its
unsupported width keyword falls back to auto.

Paint assembly distinguishes real stacking contexts from inline-blocks, floats and
positioned `z-index:auto` ancestors. Positioned descendants escape those non-context
groups and sort by numeric stack level, retaining tree order for ties. Integer z-index,
opacity, transforms and fixed/sticky contexts retain isolation. Ancestor clips remain
balanced around escaped chunks. Final node hit-test order is assembled with the paint
order, including popup backgrounds/padding, before renderer-local markers are stripped.
Regressions contrast auto versus zero, negative levels, nested non-context wrappers,
opacity/transform isolation and clipping, following
[CSS 2.2 Appendix E](https://www.w3.org/TR/CSS22/zindex.html#painting-order).

The live table also exposed a generic wrapping defect: whitespace outside a nowrap span
lost its break opportunity, and line fitting considered only the next atom rather than
the whole unbreakable run. Line fitting now measures runs across inline style boundaries;
the parent-owned whitespace before a nowrap span remains a legal break. This prevents
the rainy-days label from crossing the next cell without any page-specific widths or CSS.

These implementation tests do not establish the PR's visual acceptance. The acceptance
criterion is Wikipedia browsing without obvious broken rendering: compare loaded pages,
article navigation, tables/infoboxes, sidebars, and repeated scrolling with Chromium.
Crashes, overlapping/clipped content, broken shared columns, stale geometry and visibly
corrupted scroll frames block acceptance, even when isolated tests pass. Hidden live
captures and interaction checks must be reviewed before calling the PR ready.

### Reviewed browsing sample (2026-09-14)

The final candidate includes anonymous-table/atomic-inline sizing, adjacent DOM
insertion, positioned shrink-to-fit widths and non-context paint-order corrections.
All runs use the guarded hidden release harness with fresh profiles; Chromium
references use unified headless mode and muted audio. Generated evidence remains
in ignored `target/wiki-regression/`, not in the source tree or commits.

| Page / interaction | Reviewed result | Local evidence prefix |
| --- | --- | --- |
| Main Page | Article columns and Appearance panel remain inside the viewport; no overlapping text | `final-stack-main` |
| Coron, Palawan / Climate | All monthly columns align; wrapped row labels remain within their cells, including rainy days and humidity; source footer stays below the grid | `final-stack-coron` |
| Rust (programming language) | Infobox no longer expands across article text; nested atomic links wrap and image proportions are retained | `final-stack-rust`, `rust-sidebar-final` |
| Periodic table | Both CSS-table figures are visible and proportionate beside readable article text | `final-stack-periodic`, `periodic-sidebar-final` |
| Palawan / narrow contents menu | Opaque popup paints above the article, with 232px width matching the Chrome reference; activating Government updates the URL fragment and scrolls to that section | `final-stack-menu`, `final-stack-menu-navigation`, `chrome-menu-open` |
| Yemen article / down-down-up-up wheel input | Inspected 500ms-sampled frames show stable article/infobox columns and sidebars during reversal, without stale displaced content | `final-stack-yemen`, `final-stack-yemen-film` |

The reported overlap/collapsed-column/missing-figure/menu-paint failures are absent
in these reviewed views. This is a representative browsing sample, not a guarantee
for every Wikipedia page or pixel-perfect Chromium parity. Article-to-article link
activation (Coron to Palawan) was also checked in `palawan-navigation-check` before
the final stacking change; the final menu-to-section check exercises the updated
paint/hit-order path.

Final runs have no renderer exits, harness failures or reported uncaught script
errors. Console diagnostics still contain caught exceptions for missing
`PerformanceObserver` and ResourceLoader storage-cache writes (`invalid storage
value`); these are not described as zero JavaScript errors. Appearance controls
were absent in some 15-second-settle captures and present in the extended fresh
runs. Delayed asynchronous initialization, loading speed and post-load scroll
responsiveness remain open; the long validation settle interval is not a measured
page-load time or a performance pass. The subsequent
[Performance Timeline slice](performance-timeline.md) removes the missing-observer exception;
it does not claim to fix the storage warning or improve loading speed. The later
[Web Storage slice](web-storage.md) fixes the cache-value warning independently.

### Earlier checkpoint and remaining responsiveness work

The earlier `e4e4c71` release was inspected on the Main Page, Coron/Palawan climate table,
Palawan infobox and the Yemen article, including a narrower Palawan viewport and
real wheel input with reversals. These captures have no renderer exits or JavaScript
errors, and the inspected views no longer show the reported label overlap, off-screen
sidebar, overflowing infobox bars or missing flag/seal row. Palawan's table width is
323.84 CSS pixels in both Breeze and Chrome; the flag/seal row is 111.78 versus 111.76.
Evidence remains untracked in `target/wiki-regression/*-e4*`, with Chromium references
in the same directory. This is a named browsing sample, not proof that every Wikipedia
page matches Chrome; interactive maps, typography and full CSS conformance remain
separate compatibility work. The following responsiveness limitation is still open.

The same page's post-load scroll actions expose a separate responsiveness problem.
Opt-in host profiling now retains bounded details for callbacks of at least 16ms,
instead of discarding details below 100ms. The captured sidebar callbacks changed list
item classes and then read element scrolling geometry: synchronous layouts took roughly
25-40ms, followed by paintable layouts of roughly 29-48ms. Text measurement contributed
roughly 2.7-3.4ms to the synchronous layouts. These timings identify redundant full-page
work, not a demonstrated scrolling fix. Reuse must preserve mutations between reads,
resource/font changes, viewport changes, scroll offsets and observer semantics.
The page-ready scroll trace did not exercise this post-load callback sequence and is not
substituted for the user's reported interaction. Evidence is in ignored
`target/wiki-regression/coron-scroll-profile.json`.

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
Breeze reports 84.800003px and Chrome 84.796875px versus the test's integer-derived 86px.
After device-pixel border snapping, the pinned local WPT run at 125% has 141 passing cases
and four failing cases (746 assertions pass, seven fail), with no timeouts or crashes.
The other failures are `resize-observer/observe-018.html`,
`css/cssom-view/table-client-props.html` and `css/cssom-view/table-offset-props.html`.
Chrome reproduces those fractional-border assertion failures at the same scale: the
observer width is 40.390625px (Breeze 40.400002px) instead of the asserted 40px; separated
table/caption heights are 33/53px instead of 34/54px. These are observed comparison
results, not blanket conformance claims. Upstream tests and observer geometry remain
unchanged; the normal-DPI CI WPT gate also remains enabled.

Normal block backgrounds and borders now paint before floating descendants, followed by
inline content and nonnegative positioned groups, following
[CSS 2.2 Appendix E](https://www.w3.org/TR/CSS22/zindex.html#painting-order).
Paint ownership survives geometry reordering; clip groups stay balanced when split across
phases, and opacity groups remain atomic. The owned `paint-phases.html` fixture compares
the border/float overlap with Chrome. The subsequent positioned-width and paint-order
slices above address automatic shrink-to-fit sizing and descendants escaping non-context
ancestors. These corrections do not establish complete positioning/stacking conformance.

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

Physical border colors are independently cascaded, serialized and painted in top/right/
bottom/left order, following [CSS Backgrounds 3](https://www.w3.org/TR/css-backgrounds-3/#border-color).
One-to-four value expansion, side longhands, CSS-wide keywords, shorthand resets and
`currentColor` used values are covered; transparent sides retain ownership of their corner
sectors. Block, inline and control decoration carry all four colors across IPC (major 7
rejects incompatible older peers). The oversized paint and wire coordinators were split
by responsibility. `border-colors.html` is an owned comparison fixture; offscreen native
raster tests check edge colors and transparent joins. This does not implement dashed/
dotted/double patterns, independent border-style state, elliptical per-corner radii or
collapsed-table border conflict resolution. Existing radius painting remains approximate.
Border widths use CSS Values 4 device-pixel snapping before layout and CSSOM serialization,
including generated pseudo-elements. A DPI change recomputes the lengths; paint does not
independently round them up again. At 125% scale, authored 1px/6px borders become 0.8px/5.6px,
while positive sub-device-pixel widths retain a one-device-pixel stroke.

On the reported Wikipedia article, the sidebar text overlap and notice/icon overlap are
removed, the serif heading uses its fallback font, and the artificial title-to-tabs gap
is removed. The previous `nodeType` exception also disappears. The left pane now has its
own scrollbar and sticky placement. `display:flow-root` now establishes an independent
block formatting context: its border box avoids outside floats, contains its own floats,
and prevents parent/child margin collapse without imposing an overflow clip. Wikipedia's
Background rule is 572.88px wide versus Chrome's 572.8875px, ending before the infobox.
Appearance radio selections and the search field/button now render in the comparison;
typography/spacing and circle rasterization differences remain visible.
Console diagnostics now expose missing PerformanceObserver
support; this is not proof that it explains every remaining initialization difference.

Match **media viewport width**, not content width, when comparing responsive pages.
Breeze's 1694 × 960 outer window on this 125%-DPI machine has a 1680px media viewport and
1663.2px content width after its scrollbar. Chrome's viewport option includes its scrollbar.
Setting Chrome to 1663px incorrectly selected the opposite side of Wikipedia's 1680px
breakpoint. At 1680px both use a 260px left sidebar and 948px article column; their native
scrollbar widths still differ. Live banners/content can also vary between requests.

Further slices must cover remaining positioned paint-context ordering, broader grid track
constraints and self-alignment, remaining table border/baseline/column-group behavior,
broader form-control behavior, and the remaining page-initialization APIs. Native control artwork
remains approximate. The owned `button-alignment.html` fixture checks intrinsic inline
buttons, retained SVGs and positional block alignment independently of Wikipedia.
At 125% scaling its inline text button is 30.6875px wide and its icon button is 40px wide
in both engines, and all four block-alignment child rectangles match. Inline baselines do
not yet match: Breeze centers neighboring atomic inline boxes. This is a separate inline-
formatting gap, not passing evidence. `line-heights.html` covers computed inheritance,
shorthand/longhand order and negative leading independently of Wikipedia.
Do not replace these gaps with site-specific CSS or claim full conformance from this fixture.

## Authored form-control state and search-field follow-up

The owned `form-controls-flow-root.html` fixture covers the additional requested controls
without copying site markup. [HTML checkedness](https://html.spec.whatwg.org/multipage/input.html#dom-input-checked)
is live state separate from the `checked` content attribute. Dirty checkedness, default
reflection, cloning, reset, name/form/tree radio grouping, `:checked`/`:indeterminate`,
and canceled legacy click activation are tested. Labels forward activation, and native
clicks update authored radio artwork on both scripted and scriptless pages. These state
changes invalidate styles without fabricating attribute mutation records. A hidden live
Wikipedia click selected Small and changed the page preference, with no JavaScript errors.

[CSS Pseudo-Elements](https://www.w3.org/TR/css-pseudo-4/#placeholder-pseudo) placeholder
color/opacity cascade separately from entered text, including variables, inheritance and
`currentColor`. The retained control carries its resolved hint color across IPC major 7;
native EDIT painting uses the same color without putting placeholder text into the value.
No generated DOM child is created. Broader placeholder typography and native-control
artwork remain separate work.

[Flexbox cross sizing](https://www.w3.org/TR/css-flexbox-1/#cross-sizing) applies a single
line's min/max height before center/end alignment, including overflow and min-over-max
precedence, without turning a minimum into a definite percentage-height basis. This
centers the Search label without a button-specific offset. The rest of the flex algorithm,
including auto-height stretch relayout and multiline distribution, is not declared complete.

This is not full form conformance: native checkbox/radio artwork, keyboard group traversal,
full form reset/submission, dynamic external form-ID reassociation, and selected-option
state need further slices. The current work specifically verifies authored checkable
artwork and its pointer-driven live state.

The final local follow-up suite passed 1,196 tests (4 intentionally ignored); Clippy,
format and source-size checks passed without raising ceilings. The curated 125%-DPI WPT
rerun remains 141 passing/4 failing cases, 746 passing/7 failing assertions, zero timeouts
or crashes. These are the same documented fractional-DPI failures also observed in Chrome,
not newly passing conformance claims. Captures and reports remain ignored local artifacts.

## Scroll and loading regression follow-up (2026-09-13)

Static final screenshots did not cover rapid direction reversals. Sticky constraints now
travel with the retained display list through IPC major 8. The native browser evaluates
them at its latest viewport offset, including when installing an older renderer response.
It also invalidates the viewport instead of bit-scrolling already displaced sticky pixels.
The renderer reuses the same constraints for root scrolling, without walking the DOM or
measuring text again. Nested sticky ancestry and containing-block limits are retained;
wire ranges, dependency order, nesting and finite geometry are bounded and validated.
Malformed page-derived constraints are discarded before publication, not allowed to
terminate the renderer. This is retained viewport-sticky composition, not a claim of a
complete asynchronous scrolling/compositing tree for nested scrollports.

Pointer designation invalidates selectors first. If computed styles and generated boxes
are unchanged, the renderer retains layout and returns runtime effects without sending
an unchanged full display list. Author mutations and actual hover-style changes retain
their normal rendering work. Text measurement reports now consume their work counters;
an update without layout no longer reports the previous update's measurements.

Window named-property lookup previously traversed the whole DOM for every queried name.
A mutation-generation index now preserves the existing tree order, duplicate suppression,
and live attribute/removal behavior while sharing that traversal. On the reported article,
three roughly 2,450-name enumeration passes consumed **3,254 ms** in the old implementation;
three roughly 2,470-name passes consumed **3.857 ms** after indexing. These are measured host
call costs, not whole-page completion times. The implementation preserves the existing
[Window named access](https://html.spec.whatwg.org/multipage/nav-history-apis.html#named-access-on-the-window-object)
contract; it does not add unsupported child browsing contexts.

Three alternating fresh-profile release/Chrome loads used a 1680px media viewport, 788px
content height, 125% scaling, no visible windows, and 500ms captures anchored at navigation
start. Breeze's outer window was 1694x960; its scrollbar-excluded width was 1663.2px.

| Observed milestone | Headless Chrome | Hidden release Breeze |
| --- | --- | --- |
| Article, map and readable text visible | Already present in the first 0.5s sample, all three runs | Around 2.0s |
| Appearance controls visible | Already present in the first 0.5s sample, all three runs | First complete samples at 5.0s, 6.0s and 5.5s |
| Harness page-ready median (process start; **not** visual completion) | 871 ms (775-934) | 1,697 ms (1,657-1,736) |

The controls milestone is still roughly ten times the first Chrome sample. Sampling only
gives interval bounds, not an exact ratio, and these are fresh-profile live-page runs, not
a guarantee for every network/cache state. The page-ready number substantially understates
the remaining initialization gap. **Loading parity is not achieved.** The lookup bottleneck
is fixed; the remaining delay needs further phase profiling rather than a larger timeout,
relaxed benchmark threshold, or a site-specific workaround.

The hidden harness now accepts `-WheelTarget 'x,y,delta'` in CSS viewport/pixel coordinates.
It uses ordinary renderer wheel dispatch, including cancellation, scroll chaining and
native animation. A process test checks the four delivered events, return to zero, and
the final sticky pixels. Two live runs exercised ten alternating viewport/nested-pane
wheels, one after twelve seconds of pointer-only initialization. Both survived without
renderer exits or uncaught script errors; sampled scrolled and return-to-top frames kept
the panels positioned correctly. This does not establish a universal scrolling FPS bound:
script-driven nested scroll and geometry flushes can still do expensive work.

The user's incident also contained missing `Element.getClientRects()` errors. The subsequent
[fragment-geometry slice](cssom-fragment-geometry.md) implements retained inline/text
fragments rather than substituting a bounding rectangle. PerformanceObserver warnings remain.
The current head passes 1,196 local tests (4 intentional ignores), Clippy, formatting and
source-size checks. The WPT figures above are from the earlier form/layout head and have
not been rerun for this scrolling/performance follow-up.

## Further loading work (2026-09-14)

The 5-6 second Appearance-panel result above is the previous published head, not the
acceptance target. The target remains a fully initialized first viewport in under two
seconds, judged from navigation-anchored screenshots with the controls present. A low
page-ready counter alone does not satisfy it.

The next optimizations remove repeated engine work without suppressing author scripts,
changing stylesheet blocking, relaxing timeouts, or special-casing Wikipedia:

- Selector candidate lists are partitioned by element/pseudo target. A bounded,
  mutation-versioned ancestor-key filter rejects impossible matches before expensive
  structural checks. Possible matches still use the complete selector matcher; tests
  cover collisions, adoption, reparenting and shadow boundaries.
- Unmatched generated pseudos no longer resolve full computed values. Explicit CSSOM
  pseudo queries still resolve initial/inherited values, and later matching rules create
  or remove generated boxes normally.
- Ready timer tasks share the existing bounded execution slice, retaining a microtask
  checkpoint after each callback and synchronous layout for geometry reads. Expired
  idle callbacks are queued tasks outside an idle period; genuine idle callbacks still
  yield to the embedder. This follows the
  [idle-callback timeout contract](https://w3c.github.io/requestidlecallback/#the-idledeadline-interface),
  not an extended idle deadline or a larger callback budget.
- Active idle deadlines no longer report zero merely because their own callback changed
  the DOM. Admission of another idle callback still waits for pending rendering. The
  active deadline retains its original wall-clock/next-task bounds, and actual delivered
  input/network/rendering tasks still interrupt it. The regression fails before the fix
  and passes afterward; the equivalent hidden Chrome fixture retained 15.3ms after a
  mutation from an initial 15.5ms budget (these are fixture observations, not constants).
- Publishing a layout snapshot does not eagerly build a second style cache for a
  hypothetical JavaScript geometry read. The read builds it when needed.
- Text cache hits borrow their lookup keys; intrinsic measurement does not copy raster
  glyph payloads. Only painted text requests those payloads, with unchanged shaping
  metrics. Width contributions are reused within one layout pass and its sizing probes;
  exact percentage bases, including indefinite versus zero, remain distinct. A new pass
  starts cold so changed DOM, styles, fonts and image dimensions cannot reuse old widths.

Runtime style/resource refreshes now contribute to the style-time counter. Previously
only initial loading contributed there, understating style work. Streaming Fetch script
timing also stops before style/layout processing instead of counting that work twice.
The opt-in checkpoint diagnostic separates style/resources, element/pseudo resolution,
and layout costs. Old and new style/script totals therefore have different accounting
coverage and must not be presented as a performance regression or speedup by themselves.

### Banked checkpoint

The banked fresh-profile hidden release capture has the article and map visible at 1.0s.
Appearance radio controls are absent in the 2.5s capture and present at 3.0s (actual capture
times 2500.540 and 3000.545ms). That banks a roughly three-second visual initialization
checkpoint, versus the earlier 5-6s samples. This is a live-page, 500ms-sampled observation,
not a precise completion timestamp or a guarantee across network/cache states. The earlier
Chrome reference already had its controls in the first capture, around 0.6s actual time.

| Latest Breeze measurement | Result |
| --- | --- |
| First complete Appearance-panel sample, navigation start | 3.0s |
| Harness page-ready, process start (not visual completion) | 898ms |
| Cumulative style/resource refresh | 556ms |
| Cumulative layout | 927ms |
| Cumulative JavaScript | 268ms |
| Renderer CPU | 2,344ms |

Measurements are retained locally in `target/wiki-regression/load-deadline-1.json` and its
navigation-anchored filmstrip. These generated artifacts are not committed. Repeated style
and layout checkpoints remain the largest measured engine costs; **the under-two-second
target and Chrome loading parity are still open**.

Checkpoint validation: `cargo test --all-targets --quiet`, Clippy with warnings denied,
formatting, and the source-size gate pass. The 12 unchanged upstream idle-callback files
also pass all 22 assertions at pinned WPT revision
`f9ecd8a4a9c6e9865ea4aee4741e4b02f75fd476`. This does not imply the broader WPT suite was
rerun or that all web-platform scheduling behavior is conformant.

### Literal declaration preparation

The next measured candidate reuses token serialization for immutable declarations without
`var()`, instead of reconstructing their strings on every matched element and refresh.
This does not cache computed lengths, colors, URLs or element-dependent values. Tokenization
detects escaped function names and nested references; strings containing the text `var(`
are not references. Variable-bearing values continue to resolve against current custom
properties, following [CSS Variables](https://www.w3.org/TR/css-variables-1/#using-variables).
Each retained literal is capped at 4KiB; oversized values keep the uncached path. Cache
ownership follows the existing bounded parsed stylesheet lifetime, not a global map.

Two fresh hidden baseline runs recorded 670/653ms of cumulative style refresh; three runs
with literal preparation recorded 603/777/606ms. The 777ms run was also slower in layout
(1,286ms versus roughly 1,010-1,044ms in the other runs) and JavaScript. These samples
suggest a modest style-phase reduction, not another demonstrated visual-loading milestone
or a controlled statistical speedup. Reports are `load-literal-before-*` and
`load-literal-after-*` under the same ignored evidence directory. The three-second banked
checkpoint and the outstanding under-two-second target are unchanged.
The final PNGs from both baseline runs and the first/third post-change runs are byte-identical;
the inspected three-second post-change frame retains the article, map and radio controls.

### Dirty-scope ownership and non-rendered geometry (2026-09-14)

Node moves now invalidate only connected old/new parents. Detached staging fragments no
longer leave missing roots in the rendering work list; moving a connected child into a
detached holder still requests removal rendering and evicts its old style entries. These
three regressions failed before the change. Script elements still participate in structural
selectors: insertion/removal can change a following element's `:first-child` geometry.
The former blanket script-cleanup skip was replaced with that explicit contract, following
the [DOM insertion/removal algorithms](https://dom.spec.whatwg.org/#concept-node-insert).

A possible stylesheet mutation keeps its DOM dirty roots until the style consumer checks
the effective inputs. Exact shared identity includes text, order, source URLs, shadow scope
and media environment; a document-base change also prevents reuse. Identical rules refresh
only those dirty subtrees, not cached selector results. Changed inputs still refresh the
whole document. Both paths prune detached/unassigned composed-tree styles and generated
pseudos. Tests compare inherited values, sibling matching, generated content, stylesheet
removal and synchronous geometry reads against a fresh cascade.

After refreshing styles, changes confined to unchanged `display:none` subtrees can reuse
layout. This does not suppress script, resource discovery, metadata or accessibility
presentations. Reveals, geometry/removal changes, global rule refreshes, unknown/mixed roots,
base-URL definitions, non-HTML content and fullscreen descendants retain the normal path.
`visibility:hidden` and `opacity:0` do not qualify: they still generate boxes under
[CSS Display](https://drafts.csswg.org/css-display/#valdef-display-none).

A clean release rebuild of banked commit `473097c` was compared with the candidate in three
alternating pairs, with no concurrent compiler/tests, fresh profiles, the same 125% DPI
viewport and navigation-anchored 500ms screenshots. Every final PNG is byte-identical to
the banked capture. The candidate's 2.5s frame was visually inspected: its Appearance controls
are absent at 2.0s and present at 2.5s. All slower samples are retained.

| Measurement | Banked build | Candidate |
| --- | --- | --- |
| First final-equivalent viewport samples | 3.0 / 3.5 / 3.0s | 3.0 / 2.5 / 3.5s |
| Median visual completion sample | 3.0s | 3.0s |
| Median renderer CPU | 2,547ms | 2,266ms |
| Median cumulative style/resource time | 611ms | 526ms |
| Median cumulative layout-phase time | 884ms | 923ms |

These small, variable live-page samples show lower median CPU/style work, not a proven
visual-loading speedup or Chrome parity. Three earlier candidate captures were 3.0/2.5/2.5s;
they are not substituted for the slower paired results. The under-two-second target remains
open. Evidence is in ignored `target/wiki-regression/load-paired-{before,after}-*.json` and
filmstrips. The legacy `full_layout_rebuilds` field counts render-requested presentations,
including retained layouts, and is **not** used to claim fewer layout executions here.

### Transparent native text controls over rounded backgrounds (2026-09-22)

[CSS Backgrounds and Borders §4.3](https://www.w3.org/TR/css-backgrounds-3/#corner-clipping)
requires an element's background to follow its rounded border edge. The renderer already
computed and emitted the correct used radius for rounded search wrappers, but an opaque
Win32 EDIT child filled the same rectangle afterward when its own CSS background was
transparent. Projected text, search, password, and textarea controls retain CSS
transparency in their alpha channel while their RGB channels carry the composited
ancestor color. The host uses that color in an owned opaque brush: Win32 EDIT's
hollow-brush path failed to repaint reliably while focused and exposed a white strip.
Where a matching rounded solid backdrop contains the control, the native child is
clipped with a window region to that CSS edge. The region is relative to the child
and is applied once on recreation, not on every scroll. Windows sends read-only or
disabled EDIT controls through `WM_CTLCOLORSTATIC`, so it and `WM_CTLCOLOREDIT`
use the same backdrop brush policy. A later CSS restyle recreates native controls
when their paint/font spec changes; geometry-only moves preserve the live EDIT.
This is a solid-color compositing slice, not full background-image or multi-corner
clipping support.

The hidden DDG comparison at 1454 CSS px and 125% scale measured a 24px computed radius
in both Chrome and Breeze, with a 20px used display-list radius after fitting the 40px
box. Before the change the top-left corner painted as an opaque rectangle; after it,
background pixels follow the rounded edge, and the focused release capture has no white
strip or displaced text. This validates the compositing fix, not full search-page
parity. CSS box shadows and multi-corner radii remain separate compatibility
slices.

### Font fallback selection during repeated layout (2026-09-22)

Each grapheme previously repeated font-family parsing and fallback queries across
successive presentations. Renderer-local selection now caches the chosen face by
family, grapheme, script, weight, and italic state, bounded to 4096 entries. Registering
a new web font resets the catalog and its cache so late fonts cannot reuse stale
fallback choices. On one pair of fresh-profile hidden DDG runs, measured font
selection fell from 746.944 ms to 46.908 ms. This is a direct subsystem metric;
live network/script timing varied, and the page can still show late CSS/script
presentations and visible layout shifts. No claim of Chrome-equivalent load time or
elimination of those shifts follows from this cache.
