# Authored controls and replaced-image constraints

Buttons with authored flex/grid layout must retain their descendants, rather than being
flattened into a platform label. Breeze now lays out those descendants and keeps a semantic
control record for activation and form submission. Native child windows and offscreen control
labels do not paint over the authored contents. Input-button and legacy inline-button
projection remain separate paths; this is not a claim of complete button-layout support.

This implements the relevant authored-layout behavior from the
[HTML button rendering model](https://html.spec.whatwg.org/multipage/rendering.html#button-layout).
Visible button text is kept separate from its submitted `value` attribute. A hidden integration
test checks both child pixels and trusted submission, not just DOM presence.

Inline-block content establishes a new percentage-sizing basis when its dimensions are
definite. Replaced images in the inline collection path now apply minimum/maximum dimensions,
preserving the intrinsic ratio where both-axis constraints permit it. Incompatible bounds
take precedence over the ratio. See [CSS 2.2 sizing](https://www.w3.org/TR/CSS22/visudet.html).

The live channel-avatar diagnosis was an 88-pixel intrinsic image with `max-width:100%` inside
a 40-pixel inline-block. The old image path ignored that maximum. Owned regressions cover
this case and percentage dimensions within a padded border-box container.

Nested block wrappers now measure descendants' intrinsic content contributions rather than
using their flex base sizes. A zero flex basis controls later flex distribution; it must not
collapse an ancestor's content measurement. The owned regression reproduces the one-pixel
Subscribe control without any site selectors.

The live wrapper chain additionally required distinguishing an indefinite percentage basis
from a definite zero. During intrinsic measurement, cyclic percentage maximum widths
(including percentage-bearing `calc()`) behave as `none`; the normal layout pass resolves
them against the resulting containing block. See
[CSS Sizing 3 cyclic percentages](https://www.w3.org/TR/css-sizing-3/#cyclic-percentage-contribution).

Text painting retains the shaped font height separately from the line-box height and splits
extra leading above and below the font, following CSS 2.2 section 10.8.1. Previously, taller
line boxes reserved the right space but painted labels at the top. This fixes that leading
error; it is not a claim of complete mixed-font baseline or vertical-align support.

Button icon discovery also traverses the composed tree, including shadow/slotted content,
instead of searching only light-DOM descendants. The renderer control wire format advances
to minor 12 to carry the authored-content projection flag explicitly.

These changes concern controls and sizing. They do not resolve the separately tracked blank
video player, startup delay, or long-playback reset, and do not complete YouTube acceptance.

## Stylesheet completion and script measurements

Computed-style queries include downloaded stylesheets as well as inline rules. A stylesheet
resource completing without a DOM mutation invalidates the script realm's style and geometry
caches. Before its `load` handler runs, the synchronous layout snapshot receives the same
stylesheet sources as the presentation pipeline. This follows the
[CSSOM computed-style model](https://drafts.csswg.org/cssom/#dom-window-getcomputedstyle) and
[HTML stylesheet processing](https://html.spec.whatwg.org/multipage/links.html#link-type-stylesheet).

An owned hidden integration test installs an external stylesheet dynamically and reads both
computed color/position and bounding width inside its load handler. This is a compatibility
regression test, not evidence that the remaining live playback or layout problems are resolved.

Script style caches consume rule invalidation independently of presentation. The renderer's
accumulated dirty flag can remain set across many script queries; using it to invalidate
each query repeatedly reparsed unchanged external CSS after ordinary attribute mutations.
Actual stylesheet mutations still invalidate both script caches immediately.

## In-flow positioned painting

Relatively positioned boxes retain their normal-flow geometry but participate in positioned
painting, alongside absolute/fixed boxes. Paint ranges are collected after layout and alignment,
then ordered by stack level and source order; hit-test order follows the same ordering.
This follows [CSS 2.2 Appendix E](https://www.w3.org/TR/CSS22/zindex.html).

The live blank-player failure involved a relatively positioned player sibling at stack level 1,
followed by an opaque application background. Painting the former as ordinary flow content let
that later background hide valid decoded video. An owned hidden screenshot test checks the
equivalent relationship without any site-specific markup or overrides. This does not claim
complete CSS stacking support, including positioned descendants escaping nested auto-level groups.

## Flex cross-axis orientation

Automatic height stretching applies to row and row-reverse containers, not columns.
In a column, the cross axis is horizontal; the available container height remains a basis
for percentage heights without becoming every child's used height. This follows
[Flexbox cross-axis alignment](https://www.w3.org/TR/css-flexbox-1/#align-items-property).
Previously, an empty spacer in a fixed-height column expanded to the full container height
and pushed its following navigation content below the viewport.

Regressions cover both column directions, continued row stretching, and a hidden native
drawer capture with trusted coordinate activation. This correction is not a claim of full
column flex-grow/shrink, wrapping, or automatic minimum-size support.

## Pointer hover designation and boundary events

Native mouse target changes now update `:hover` designation through flat-tree ancestors,
including assigned slots and shadow hosts. Both script-free and scripted documents invalidate
computed styles and repaint on entry and exit; repeated moves inside the same target do not
invalidate styles again. Designation is user-agent state, not an HTML attribute mutation, and
is not copied by node cloning or changed by synthetic mouse events.

Scripted documents receive trusted pointer/mouse over, out, enter, and leave events. Enter/leave
do not bubble or cross shadow boundaries; common ancestors are not re-entered when moving
between their children. Event dispatch retargets `relatedTarget` alongside `target`, suppressing
internal shadow-tree transitions outside that tree. Window/content exit clears designation.
The contracts follow [Selectors hover](https://www.w3.org/TR/selectors-4/#the-hover-pseudo),
[Pointer Events boundary events](https://www.w3.org/TR/pointerevents3/#boundary-events), and
[DOM event dispatch](https://dom.spec.whatwg.org/#concept-event-dispatch).

Coverage includes a hidden renderer repaint test with and without JavaScript, boundary event
counts, computed style during entry, shadow retargeting, slots, cloning, and repeated moves.
This is not a claim of complete pointer support: pointer capture, boundary re-hit-testing after
stationary-pointer layout changes, and the separate active/focus pseudo-classes remain gaps.
Live YouTube controls, navigation metadata, comments, and playback cadence require separate
end-to-end verification; these regressions alone do not establish site completion.

## Bottom-anchored positioned boxes

An absolutely/fixed positioned box with automatic `top` and a definite `bottom` aligns
its bottom margin edge, not its top border edge, to that inset. Resolve its used height
first, including natural height, padding/border, and min/max constraints, then translate
its in-flow paint and hit-test geometry together before laying out positioned children.
This follows [CSS 2.2 positioned height constraints](https://www.w3.org/TR/CSS22/visudet.html#abs-non-replaced-height).
Previously the controls bar's top landed at the player's bottom, outside its clipping box.

Regressions exercise absolute and fixed boxes, percentage bottom insets, explicit and auto
heights, min/max constraints, margins, borders/padding, descendant bounds, and painted bounds.
This does not close the separate hit-testing/initial-container, metadata, comments, or
playback-cadence issues on live pages.

## Cached resource completion belongs to each element

Fetch deduplication must not suppress a newly attached stylesheet link's `load` event.
The renderer keeps completed resource results separate from the set of element owners
already notified. A later owner of cached bytes receives its own asynchronous completion,
without fetching the URL again or replaying events on existing owners. Stylesheet handlers
observe the installed cascade through computed style and synchronous geometry APIs.
This follows the per-element completion steps in
[HTML stylesheet processing](https://html.spec.whatwg.org/multipage/links.html#link-type-stylesheet).

An owned HTTP fixture checks two sequential links, one stylesheet request, asynchronous
delivery, computed color, and exactly one event per link. Ownership tests cover cloning,
unrelated mutations, and removal/reinsertion or URL changes observed across checkpoints.
Full per-request generation tracking for multiple retargets within one script task remains
outside this change. Image owners share the completion bookkeeping as before.

Live verification removed YouTube's initial player skeleton, which had intercepted clicks
over recommendations. A subsequent recommendation click now reaches the next URL, but
revealed a separate synchronous-layout timeout retaining the previous metadata. This is
progress on initialization, not a claim that navigation or playback is complete.

## Synchronous style/layout profiling

Opt-in host-call diagnostics split synchronous geometry flushes into
`layoutFlush::style` and `layoutFlush::layout`. These are sub-phases of `layoutRect`,
not additional time to add to that total. Zero-work phases are omitted; aggregation
is drained with the surrounding script task's diagnostics. Ordinary browsing does
not collect the host-call profile.

`layoutFlush::style-full` and `layoutFlush::style-incremental` partition that style
time by whether stylesheet rules were rebuilt. They overlap `layoutFlush::style`
and must not be added to it.

For incremental recalculation, `layoutFlush::elements` and `layoutFlush::pseudos`
measure ordinary computed-style construction and generated pseudo-element updates.
They are included in style time; the remainder includes invalidation traversal,
style comparisons, and cache maintenance.

`layoutFlush::text-measure` is the text-measurement sub-phase of synchronous box
layout; per-call timing is enabled only with host-call profiling. The
`layout-style-change` and `layout-intrinsic-or-initial` categories partition
`layoutFlush::layout` by its trigger. These nested values are not additional time
to add to the layout total. An intrinsic-only trigger is not necessarily wasted
work: visible text and child-list changes can require layout with equal styles.

Incremental refresh preserves the storage of an exactly equal custom-property map
from the preceding computed style. This avoids repeated deep comparisons in
descendants without reusing values when a custom property actually changes.

Generated `::before`/`::after` rules no longer force geometry invalidation merely
because they match. Refresh compares the materialized box's geometry-affecting
style and resolved text, including `attr()` values. Box creation/removal and real
size or content changes still invalidate layout; paint-only changes still update
the generated style. Regression tests cover each of those transitions.

These are bounded reductions in redundant work, not a claim that YouTube's
navigation timeout or playback cadence is resolved. Live recommendation switches
still need end-to-end validation of the new URL, metadata, and visible playback.

### Retained-realm navigation: reducing redundant style work

The navigation investigation found expensive synchronous geometry reads inside a
site callback, not just a stale text-paint cache. A changed URL or a playing media
backend alone is therefore not evidence that a navigation finished successfully.

- Selector candidate indexing includes necessary attribute names and chooses a
  selective positive key. Child/descendant chains can reject missing ancestor
  keys before the complete matcher runs. Alternatives and sibling chains remain
  conservative, and no ancestor-key cache survives a matching pass or mutation.
- Read-only selector attribute checks borrow DOM strings instead of allocating
  copies. This does not change namespace or selector matching semantics.
- Independent style consumers share immutable compiled rules only when the
  document, ordered CSS sources, source URLs, shadow scopes, and media environment
  are identical. Computed values remain separately owned. The bounded thread-local
  lookup holds weak references and cannot retain a closed document's CSS.
  Discovery is bounded to four live candidates per document and 64 document keys;
  a discarded temporary cascade cannot hide another consumer's still-live parse.
  When the source set changes, unchanged individual sheets also retain immutable
  parsed payloads. Each occurrence receives fresh cascade order numbers, so sheet
  insertion, removal, reordering, and repetition preserve tie-breaking. A cached
  truncated sheet is reparsed if its available rule budget grows; all existing
  source, nesting, declaration, per-sheet, and page-rule limits remain in force.
- Script computed style and `offsetParent` preserve the same ordered external
  stylesheet occurrences as layout. A URL-keyed map must not collapse repeated
  occurrences or reorder otherwise equal cascade ties. CSSOM resource lookup
  remains separate from cascade order.
- Full rule rebuilds recompute all matches but reconcile against previous computed
  values and generated boxes. Exactly equal custom-property maps retain their
  storage; actual rule, value, content, and geometry changes still take effect.
- Setting an attribute to its existing value still produces MutationObserver
  records and custom-element reactions, as required by the
  [DOM attribute-change algorithm](https://dom.spec.whatwg.org/#concept-element-attributes-change).
  It no longer changes the internal rendering version or expands a subsequent
  real mutation's dirty subtree. Resource side effects are not suppressed.
- Each renderer presentation refreshes the document title from the retained DOM,
  rather than repeatedly sending the title captured during initial script loading.

The two-second script execution guard is unchanged. Verification uses fresh,
hidden, silent sessions with native recommendation clicks and half-second
screenshots; repeated switches, updated metadata, and visible frames must be
checked together. These changes do not by themselves establish complete YouTube
fidelity, comments support, or smooth playback.

Hidden reports include a bounded `titles` object: the last accepted renderer
document title and the actual browser tab title, each limited to 512 UTF-8 bytes
with an independent truncation flag. A missing renderer presentation reports
`null`, not an inferred title. This separates stale document metadata from a
browser-title propagation failure; it is not a site-specific title replacement.

Synchronous layout snapshots now defer styles below `display:none` boundaries,
whose descendants generate no boxes under
[CSS Display's box-generation rules](https://www.w3.org/TR/css-display-3/#box-generation).
This is exclusive to the geometry snapshot: owning-page diagnostics and script
computed-style caches keep their existing full/on-demand contracts. A hidden
mutation evicts stale deferred entries; reveal recomputes the subtree, including
inherited properties and generated content. `display:contents` is not skipped.
Table row groups, rows, cells, captions, and native-button icon traversal respect
the same boundary. Hidden root elements and hidden fullscreen ancestors suppress
layout rather than causing a missing-style lookup or showing their descendants.
Fullscreen top-layer eligibility follows shadow-including ancestry, not slot
assignment. Eligible fullscreen subtrees under hidden slots or outside the normal
composed traversal are hydrated separately in both dense and deferred snapshots;
their mutations and fullscreen exit still invalidate geometry.

CSSOM View geometry uses the same sizing and placement algorithms as normal
rendering, but does not construct discarded paint items, clipping/opacity layers,
paint ordering, or form-action records. It retains the same scrollable overflow
extent for script-requested viewport scrolling. Inline
recursion, replaced/control boxes, flex/grid/table placement, and transforms still
populate the complete node-bounds map. Parity fixtures compare those maps with
normal retained rendering; this is not a separate simplified layout algorithm.

Publishing fresh renderer geometry acknowledges only the intrinsic-size work
already included in that snapshot. Pending style roots, removals, and rule rebuild
obligations are retained. A later ARIA-only change no longer inherits an old text
mutation's intrinsic trigger; a new text mutation restores that trigger normally.

IntersectionObserver registrations can now be reused after `disconnect()` or
after the final `unobserve()`. Rendering updates iterate a snapshot of active
observers so callback reconnection cannot cause repeated delivery in one update.
Callback exceptions dispatch the existing trusted global error event, with one
console report unless the event is canceled; later observers still receive their
entries, as required by the
[observer notification algorithm](https://www.w3.org/TR/intersection-observer/#notify-intersection-observers-algo).
This is a generic lifecycle fix, not a claim that YouTube comments are complete.

Native viewport scrolling now translates `getBoundingClientRect()` snapshots into
viewport coordinates while preserving document-coordinate `offset*` measurements.
Missing and disconnected boxes retain the all-zero rectangle; positioned boxes
with zero width and height still retain their translated position. Previously
returned rectangles remain snapshots, as required by
[CSSOM View](https://www.w3.org/TR/cssom-view/#dom-element-getboundingclientrect).
Zero-scroll reads skip fixed-position style classification; scrolled reads reuse
the existing computed-style cache. Viewport-fixed subtrees remain viewport-relative,
whereas fixed boxes contained by transforms or other fixed-position containing
blocks move with that ancestor, following
[CSS Position](https://www.w3.org/TR/css-position-3/#fixed-cb).
This corrects the geometry API, not the separate native fixed-content painting gap.

Native scroll and resize also schedule the existing dedicated renderer geometry-
observer task after the input task's microtask checkpoint, independent of public
event propagation. Author `stopPropagation()` or
`stopImmediatePropagation()` cannot suppress these updates, and synthetic scroll
events do not manufacture a UA observer task or published layout snapshot. Quiet
scroll inputs request an immediate clock update without generating a visual
revision, and repeated inputs coalesce with pending geometry work. Fixtures cover
threshold crossings, stable explicit-root intersections, no duplicate unchanged
delivery, and reuse of valid geometry without a forced layout. Nested scroll
containers and complete ancestor clipping remain separate work; these tests do
not establish that live YouTube comments load correctly.

Native pointer hit testing retains document coordinates, but authored mouse and
pointer events now expose viewport-relative `clientX`/`clientY` consistent with
client rectangles after scrolling. `pageX`/`pageY` expose document coordinates
during dispatch and current native scroll plus client coordinates outside dispatch;
`x`/`y` alias client coordinates. Synthetic coordinate initialization follows the
same CSSOM View contract and ignores nonstandard page-coordinate initializer keys.
This does not implement target-padding-relative `offsetX`/`offsetY`, drag movement
metrics, or complete overlay hit-testing semantics, nor prove that live player
hover controls are fixed.
