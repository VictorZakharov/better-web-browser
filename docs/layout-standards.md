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
| [Block content alignment](https://www.w3.org/TR/css-align-3/#distribution-block) | Positional `align-content`, explicit safe/unsafe overflow, and single-subject distribution fallbacks move the block's in-flow contents as a unit. Non-normal alignment establishes a formatting context. Cascade, CSS-wide keywords, computed-value serialization and layout invalidation retain the value. Baseline alignment, multiline flex/grid distribution and vertical writing modes are not covered by this slice. |
| [CSS Fonts family lists](https://drafts.csswg.org/css-fonts-4/#font-family-prop) | Preserve ordered families through cascade, inheritance, shorthand, resource discovery, and renderer font selection. Parse quoted names, commas, escapes, and generic-family identity with the existing cssparser dependency. A missing first family no longer discards the specified fallback fonts. |
| [Grid spanning contributions](https://www.w3.org/TR/css-grid-1/#algo-spanning-items) | Resolve non-spanning intrinsic row contributions before spans; spans crossing flexible rows contribute to the flex fraction instead of inflating title rows. Lay out stretched items using the resolved area height, including percentage descendants and observer/paint bounds. |
| [Table cell sizing](https://www.w3.org/TR/CSS22/tables.html#auto-table-layout) | Percentage width preferences cannot starve another cell's intrinsic content, padding, and borders. Cell boxes use the shared row height, preserving backgrounds, clips, and bounds through normal block layout. |
| [Web IDL indexed properties](https://webidl.spec.whatwg.org/#legacy-platform-object-getownproperty) | Unsupported HTMLCollection indices fall through to ordinary property lookup (`undefined` absent an inherited property), while `item()` still returns `null`. Property indices do not undergo `item()`'s unsigned-integer conversion. Tests cover sentinel iteration, prototype lookup, and live removal. |

Grid/table intrinsic block-size probes are isolated from published paint, hit-test, and
ResizeObserver state. Per-layout caches reuse nested intrinsic measurements; the final
layout publishes each node once. This is a correctness change, not a loading-speed claim.

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
This does not finish table layout: columns are still allocated independently per row,
and automatic table width, span placement, and cell vertical alignment do not yet match
Chromium. Those require separate table-grid work; no page-specific widths or selectors
were added. Evidence is in ignored `target/wiki-regression/coron-table-{before,height}.png`
and `coron-chrome.png`.

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
constraints and self-alignment, shared multi-row table columns/spans and vertical alignment,
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

The user's incident also contains missing `Element.getClientRects()` errors. That separate
fragment-geometry API is not implemented by this change; returning one bounding rectangle
would not implement its inline-fragment contract. PerformanceObserver warnings also remain.
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
