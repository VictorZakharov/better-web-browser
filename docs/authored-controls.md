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

Text painting retains the shaped font height separately from the line-box height and splits
extra leading above and below the font, following CSS 2.2 section 10.8.1. Previously, taller
line boxes reserved the right space but painted labels at the top. This fixes that leading
error; it is not a claim of complete mixed-font baseline or vertical-align support.

Button icon discovery also traverses the composed tree, including shadow/slotted content,
instead of searching only light-DOM descendants. The renderer control wire format advances
to minor 12 to carry the authored-content projection flag explicitly.

These changes concern controls and sizing. They do not resolve the separately tracked blank
video player, startup delay, or long-playback reset, and do not complete YouTube acceptance.
