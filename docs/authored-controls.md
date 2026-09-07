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
