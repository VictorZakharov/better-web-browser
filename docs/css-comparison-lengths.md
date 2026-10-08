# Deferred CSS comparison lengths

The October 7 slice parses `calc()`, `min()`, `max()` and `clamp()` through the
existing `cssparser` tokenizer. Expressions are immutable shared trees, not
JavaScript evaluation or stylesheet-wide interning. General CSS lengths now
retain the expression until the relevant font, viewport and containing-block
dimensions are available.

## Compatibility contract

- Addition and subtraction require the CSS whitespace around their operators.
  Comments are tokens, not a substitute for that whitespace. Escaped function
  names and comments containing punctuation use the tokenizer's normal rules.
- Addition and comparisons require compatible number/length types. A length
  can be multiplied by a number or divided by a number. Matching lengths or
  angles can be divided to produce a number. A final
  number-only expression does not become a length merely because it is zero.
- Percentage type is retained even when arithmetic cancels its coefficient.
  An indefinite percentage basis remains unresolved; it is not silently zero.
- Comparisons involving percentages cannot choose a branch at parse time.
  `min(50%, 200px)` depends on its containing block, not just the viewport.
- `clamp()` accepts three arguments, with optional `none` endpoints. When the
  minimum exceeds the maximum, the minimum wins, as required by CSS Values 4.
- Absolute-unit-only expressions can simplify after normalization. Font/root
  and viewport units are resolved by the existing computed-style pipeline.
  Unchanged expression branches are shared rather than cloned into new trees.
- Length parsing is atomic. An invalid trailing token does not turn a malformed
  declaration into its first otherwise-valid length.
- Background-size and scroll-spacing shorthands respect balanced function
  components. Border widths use a length-only grammar; percentages and `auto`
  are rejected, while negative computed calculations clamp to zero.

These decisions follow [CSS Values 4 comparison functions](https://www.w3.org/TR/css-values-4/#comp-func)
and [calculation syntax](https://www.w3.org/TR/css-values-4/#calc-syntax).
Parsing is bounded to 16,384 source bytes, 256 expression nodes and 32 nested
levels. These are implementation safety limits, not complete CSS conformance.

## Aspect-ratio flow

An otherwise empty block with automatic height and a preferred aspect ratio
occupies its ratio-dependent height. Its margins must not collapse through the
box, nor may the final child's bottom margin collapse out of that height.
Top-child margin collapsing remains governed by the ordinary block rules.
Explicit zero-height boxes keep their separate existing behavior.

This follows [CSS Sizing 4, margin collapsing](https://www.w3.org/TR/css-sizing-4/#margin-collapsing).
The fix is in the block margin policy, not a special case for SVG or a game
stylesheet. It makes a responsive inline SVG and the following sibling agree
with Chrome in the controlled sizing fixture.

## Local verification

The checked-in `tests/canvas/css-math-sizing.html` fixture covers nine used
rectangles, including percentage comparisons, responsive SVG size and following
flow. Hidden release captures at CSS viewport widths 1,262, 500 and 320 match
Chrome's x/y/width/height within 0.02 CSS pixels for all nine rectangles.
A further matched capture at 125% device scaling and a 1,264 CSS-pixel viewport
passes the same nine rectangle comparisons. Both browser viewports are matched
from their actual reported sizes, not assumed from the outer window dimensions.
Subpixel implementation rounding is retained; this is geometry acceptance,
not a claim of universal raster or text pixel parity.

The curated WPT manifest includes the pinned upstream
`css/css-values/minmax-length-invalid.html` and
`css/css-values/clamp-length-invalid.html` files without modifying assertions.
The full local replay passed 578 files and 6,249 subtests. Full suites stay local;
CI remains a small smoke gate.

## Remaining scope

This is not the complete CSS Values 4 math algebra. Higher-dimensional products,
additional relative units and math in every number-only property remain separate
work. The [extended function slice](css-math-functions.md) adds matching-unit
division and explicit infinities/NaN without discarding their contextual types.
Small/large/dynamic viewport units
currently share the desktop viewport; there is no mobile browser-toolbar
viewport model. Full background shorthand grammar remains separate work.
HTML5test points are not used as a substitute for these behavior contracts.
