# CSS Transitions baseline

Breeze computes `transition`, `transition-property`, `transition-duration`,
`transition-delay`, and `transition-timing-function` in the CSS cascade. A
connected element's own or ancestor's `class`, `id`, or `style` mutation compares
its old presentation with its new underlying computed style and samples the
transition into the native style/paint path on animation frames. Inline style
and stylesheet declarations remain unchanged. `getComputedStyle()` and layout
geometry therefore observe intermediate values.

The supported interpolation set is opacity; foreground, background, and side
border colors; compatible 2D translations; dimensions and insets; font and
text spacing; border radius and widths; margin and padding sides; and flex
grow/shrink. Timing lists repeat to the length of `transition-property`, the
last matching property entry wins, and the supported shorthand property groups
expand to their longhands. Durations, signed delays, easing keywords,
`cubic-bezier()`, `steps()`, and bounded `linear()` stops are parsed. A transition shorter than or equal
to a negative delay is not started. `transitionrun`, `transitionstart`,
`transitionend`, and `transitioncancel` bubble with a `TransitionEvent` carrying
the longhand name and elapsed active time.

Pages with no compiled transition rules or inline transition declarations skip
the style snapshot entirely. For stylesheet rules, a conservative rightmost
selector filter skips unrelated targets; active transitions still take the
snapshot needed to restart from their presented value. Active work is bounded
to 64 elements and 16 longhands per element, and all active transitions share
one animation-frame timer. If this budget is exceeded, additional changes take
effect immediately. The queued lifecycle-event burst is capped at 2,048
entries (enough for one full `run`/`start` phase); a script that exceeds it
without yielding to a frame loses subsequent events but not style changes.

Ancestor changes snapshot potentially affected descendants before the mutation.
Compiled selectors narrow stylesheet candidates; a weak, mutation-maintained
index tracks up to 4,096 inline-transition candidates without retaining detached
nodes or rescanning the document on each toggle. Descendant admission is stable
and shares the 64-target budget. Hidden ancestors cancel active transitions.

Transitions have their own native cascade origin above author `!important`.
Reversing-shortening follows the eased progress and the prior shortening factor;
negative delays shorten along with the reversing active duration. Comma-bearing
easing functions are parsed as component values, not split into timing entries.

This is an intentionally bounded implementation, not a claim of full CSS
Transitions conformance. Changes caused only by
stylesheet insertion/replacement, pseudo-class state, or a media-query update
do not start transitions yet. It does not transition on initial tree insertion,
pseudo-elements, custom properties, or unsupported/discrete/3D transform
values. Complex mixed-unit interpolation and compositor offloading remain
open. Interruption starts from the current visual value rather than jumping
to the old endpoint.

The parsing and event/timing behavior follows [CSS Transitions Level 1](https://drafts.csswg.org/css-transitions-1/);
interpolation and sampled declarations reuse the [Web Animations Level 1](https://www.w3.org/TR/web-animations-1/)
baseline already used by Breeze.
