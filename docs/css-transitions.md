# CSS Transitions baseline

Breeze computes `transition`, `transition-property`, `transition-duration`,
`transition-delay`, and `transition-timing-function` in the CSS cascade. A
connected element's own `class`, `id`, or `style` attribute mutation compares
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
`cubic-bezier()`, and `steps()` are parsed. A transition shorter than or equal
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

This is an intentionally bounded implementation, not a claim of full CSS
Transitions conformance. Changes caused only by an ancestor selector,
stylesheet insertion/replacement, pseudo-class state, or a media-query update
do not start transitions yet. It does not transition on initial tree insertion,
pseudo-elements, custom properties, or unsupported/discrete/3D transform
values. Its current presentation overlay shares Web Animations' cascade origin,
so `!important` author declarations remain above transitions; a dedicated
transition origin is follow-up work. Reversing-shortening behavior is not yet
implemented, though interruption starts from the current visual value rather
than jumping to the old endpoint.

The parsing and event/timing behavior follows [CSS Transitions Level 1](https://drafts.csswg.org/css-transitions-1/);
interpolation and sampled declarations reuse the [Web Animations Level 1](https://www.w3.org/TR/web-animations-1/)
baseline already used by Breeze.
