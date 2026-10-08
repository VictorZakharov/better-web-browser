# Typed CSS math beyond comparisons

This extends the shared length parser, not a site-specific stylesheet rewrite.
The existing `cssparser` dependency supplies CSS tokenization; Rust's numeric
library supplies the elementary functions. No copied implementation or new
dependency is introduced.

## Implemented behavior

- `round()` supports nearest, up, down and to-zero strategies. Nearest ties
  choose the higher multiple, including negative inputs. Negative step values
  use their magnitude. Only number arguments may omit the step (default one).
- `mod()` takes the step's sign; `rem()` takes the dividend's sign. Exact-zero
  results preserve that distinction inside a containing calculation.
- `abs()`, `sign()` and `hypot()` retain percentages, relative font sizes and
  viewport dependencies until the property's actual basis is available.
  A number produced by `sign(50% - 80px)` does not become a fixed scalar early.
- `pow()`, `sqrt()`, `log()` and `exp()` accept number calculations, not lengths.
  `calc(pow(30px / 1px, 2) * 1px)` performs an explicit unit conversion.
- `sin()`, `cos()` and `tan()` accept numbers interpreted as radians or typed
  angles. `asin()`, `acos()`, `atan()` and `atan2()` produce typed angles, which
  can be consumed by an outer trigonometric function. Angle units are canonical
  radians internally. Exact quadrants do not leave spurious tiny residues;
  nearby author angles are not epsilon-snapped to them.
- Matching lengths, angles or times can divide to form a number. Number multiplication
  can scale each type. Mixed-dimension sums and comparisons are rejected even if
  a term is zero. Higher-dimensional products are still unsupported.
- `e`, `pi`, `infinity`, `-infinity` and `NaN` are calculation constants, not bare
  property values. NaN is infectious inside every function, including `pow()`
  and `hypot()`; only the outer calculation converts it to zero. Signed zero
  likewise survives internally but becomes ordinary zero at that boundary.
  Infinite outer results clamp to the existing f32 representable range before
  the property's own range policy; this is not Chrome's exact size ceiling.
  Calculation literals retain the tokenizer's negative-zero sign, matching
  Chrome 154 and the pinned WPT `signs-abs-computed.html` tests. The published
  Values 4 prose still describes signed literals as positive zero; this is an
  explicit interoperability choice, not an accidental platform-dependent cast.
- Opacity accepts scalar math and raw percentage math. Percentages are not
  added to numbers: `calc(25% + .25)` is invalid. Unitless line-height math is
  inherited as a multiplier, rather than converting into an inherited length.
- Stylesheet comment removal and computed-value serialization retain CSS token
  boundaries using the existing `cssparser` serialization categories. Thus
  `25/**/%`, `1/**/px` and `calc/**/(...)` cannot become a percentage, dimension
  or function merely by deleting comments. Numeric source spellings remain
  intact during stylesheet preprocessing; strings and URL tokens are not scanned
  as comment contents. The preprocessing nesting budget is 64 levels.
- Variable replacement retains token boundaries too: `var(--n)px` does not
  manufacture a dimension when `--n` contains a number. A winning declaration
  that becomes invalid after substitution computes to `unset`, rather than
  resurrecting a lower-priority declaration. The existing inherited-property
  table chooses inheritance versus initial value. Value substitution, reference
  validation and unused-fallback scanning use a 32-level nesting budget. This
  does not claim complete registered-custom-property or cycle-graph support.
- Flex grow/shrink, z-index and preferred aspect-ratio sides share scalar
  admission instead of accepting Rust's `inf`/`NaN` strings as property values.
  Integer calculation ties round toward positive infinity. Negative literal
  flex factors are invalid; negative calculated factors clamp after evaluation.
  Flex shorthand groups preserve function argument boundaries and require the
  grow/shrink pair to stay contiguous. An omitted basis is `0%`, not `0px`.
  Aspect ratios distinguish their top-level slash from division inside `calc()`.
- Animation and transition durations/delays accept typed `s` / `ms` math.
  Unitless zero and incompatible dimensions remain invalid. Negative literal
  durations are rejected; calculated durations clamp at zero after evaluation,
  while delays retain their negative offset. Animation iteration counts accept
  scalar math, without turning calculated infinity into the `infinite` keyword.
- Existing RGB/HSL/HWB, Lab/LCH/Oklab/OkLCh and predefined-space color
  conversion accepts context-free calculated components. Nested commas and
  division stay inside their math functions. Modern RGB can mix numbers and
  percentages; legacy RGB requires homogeneous component types and legacy HSL
  requires percentage saturation/lightness. `none` remains modern-only.
  Conversion still targets the existing sRGB8 surface, not a new wide-gamut
  painter. Relative-color syntax and deferred relative component context are
  not introduced by this slice.
- Easing functions accept typed calculations in cubic Bézier coordinates,
  integer step counts and linear output/input stops. Their arguments resolve
  through the native CSS parser before reaching the existing animation sampler,
  for both stylesheet animations and Web Animations. Nested commas do not split
  stops; percentages remain distinct from scalar outputs. Linear input groups
  stay contiguous and retain the existing missing/descending-position policy.
  Calculated integers round directly from f64 into bounded i32 storage, so
  adjacent step counts and z-index layers above 2²⁴ do not collapse via f32.
  Calculated step counts clamp to the positive integer argument range; zero
  and negative literal counts remain invalid. A one-stop `linear()` function
  and non-CSS hexadecimal coordinates are rejected by both admission paths.
  When the first two linear stops share an input position, earlier inputs
  retain the first output, as required by the current
  [CSS Easing output algorithm](https://drafts.csswg.org/css-easing-2/#linear-easing-function-output).
  Chrome 154 instead returns the second output in that case. The comparison
  records this difference explicitly; it does not relax the Breeze oracle.
- The existing translation-only transform subset uses tokenized arguments,
  including nested calculations, escaped function names and comments. Invalid
  second coordinates invalidate the declaration rather than becoming zero.
  Animation interpolation preserves percentage and font-relative dependencies
  until the target's current box is available; mismatched translation lists can
  compose through the translation-matrix fallback. This does not add rotation,
  scale or arbitrary matrices. Almost-identity unsupported matrices are not
  accepted using an epsilon. See [CSS Transforms interpolation](https://www.w3.org/TR/css-transforms-1/#interpolation-of-transforms).
- CSSAnimation effect timing queries flush pending stylesheet changes before
  reading calculated duration/count, following
  [CSS Animations 2 pending styles](https://www.w3.org/TR/css-animations-2/#requirements-on-pending-style-changes).
  Native tests check actual opacity samples, negative delays, fractional
  iteration terminal fill, identity preservation and invalid-update atomicity.
- Successful API timing updates claim only the supplied timing members; other
  stylesheet timing changes still apply. Successful keyframe replacement keeps
  its frames, while removing the last matching rule still cancels the animation.
  Replacing or clearing the effect retains API ownership. Play/pause always
  claim play-state control; reverse/start-time changes claim it only when moving
  to or from paused. Failed updates do not claim ownership. This policy applies
  to calls through `Animation.prototype` too, following
  [CSS Animations 2 API precedence](https://drafts.csswg.org/css-animations-2/#animations).

The contracts follow [CSS Values 4 stepped functions](https://www.w3.org/TR/css-values-4/#round-func),
[exponential functions](https://www.w3.org/TR/css-values-4/#exponent-funcs),
[trigonometry](https://www.w3.org/TR/css-values-4/#trig-funcs), and
[special-value semantics](https://www.w3.org/TR/css-values-4/#calc-ieee).

## Limits and verification

The shared admission budget remains 16 KiB of input, 256 expression nodes and
32 nested levels. Calculations do not evaluate JavaScript or consult global
state. Immutable expression branches retain identity during unchanged font and
viewport normalization. Elementary functions evaluate in f64; final layout
lengths remain the existing f32 representation.
Numeric spellings from cssparser-validated scalar/time/angle tokens are converted
to f64 before elementary evaluation: the tokenizer's f32 payload would otherwise
alter decimal remainder results. Token classification and unit decoding still
belong to cssparser; the numeric-prefix helper does not admit declarations.

Rust tests cover typing, arity, every round strategy, positive/negative step
semantics, signed-zero parents, NaN propagation, font and percentage bases,
computed-value serialization, declaration atomicity, inheritance and used layout.
The checked-in `tests/canvas/css-math-functions.html` fixture exposes actual
positioned-box rectangles and support results for a hidden Chrome comparison;
acceptance requires both correct geometry and correct type rejection.
The translation fixture checks 364 native admission/animation/rectangle results,
including box resizing and font changes on retained animations. Hidden Chrome
154 comparisons at 100% and 125% scale match every row within 0.02 CSS pixels.
The October 7 hidden comparison passes all 64 Breeze geometry/type checks at
100% and 125% scale. Chrome 154 matches 62: `pow(NaN,0)` and
`hypot(infinity,NaN)` are explicit differences, not waived Breeze failures.
Chrome returns one/infinity for those arguments; CSS Values 4 requires infectious
NaN, which becomes zero at the outer calculation. Negative-zero literals match
the Chrome/WPT behavior described above.

Additional October 7 release comparisons use fresh hidden profiles, matching
CSS viewports, and both 100% and 125% device scale. Expected-value assertions
remain independent of the cross-browser comparison.

| Original fixture | Breeze checks passed | Chrome expected-value result | Cross-browser result |
| --- | ---: | ---: | --- |
| Typed math geometry/admission | 64/64 | 62/64 | Two explicit infectious-NaN differences |
| Color paint and animation time components | 137/137 | 137/137 | 131 rows within one byte / 0.001; six exact two-byte translucent readback differences |
| Calculated easing and paused samples | 245/245 | 245/245 | 235 rows within 0.002; ten exact duplicated-leading-stop differences |
| Deferred translation interpolation | 364/364 | 364/364 | Every row within 0.02 CSS pixels |
| CSS/API animation ownership | 83/83 | 82/83 | 82 rows within 0.002; one negative-delay painting inconsistency |
| Calculated Grid tracks | 324/324 | 324/324 | Every row within 0.02 CSS pixels |

The color discrepancies come from the existing straight-alpha surface versus
Chrome's premultiplied 8-bit readback: calculated and literal pixels match
within each browser. This slice does not change that surface representation.
For the ownership discrepancy, Chrome reports API timing progress 0.25 with
matching keyframes but paints opacity 1, including on the next frame of a
minimal reproduction. Breeze reports and paints 0.25. The Chrome oracle
failure is retained in the report, not converted to a passing expected value.

This is not universal CSS Values compliance. The context-free scalar helper
still refuses to guess a font or viewport basis. Opacity, flex factors,
line-height, preferred aspect ratios and z-index now retain number-typed trees
until the final cascade supplies those contexts. Animation and transition times
use the same context for scalar subexpressions, but percentage-dependent scalar
time expressions remain unsupported. Additional metric-relative units and
resolution types, full unit algebra and non-translation transforms remain work.
HTML5test points are not an implementation contract.

## Computed scalar contexts

Number-typed calculations may depend on relative lengths even though their final
result has no unit: `sign(1em - 20px)` and `calc(1rem / 10px)` are examples.
These calculations resolve with the final element font, document root font and
viewport, independently of declaration order. Unsupported percentage-dependent
scalar expressions do not receive an invented containing-block basis.

Computed opacity, flex factors and ratio components apply their property range
after evaluation. Integer layers preserve f64 through positive-infinity tie
rounding, rather than losing adjacent z-index values above 2^24 in f32 storage.
Literal overrides and CSS-wide keywords replace pending calculations atomically.
After computation the tree is cleared: explicit inheritance copies the parent's
computed number instead of re-evaluating its `em` leaves in the child. Unitless
line-height inherits the computed multiplier, then uses the child's font size.

Native CSS-owned keyframe resolution uses this same computation, including
line-height resolution after keyframe font declarations. This does not claim
that script-owned animation keyframes interpolate arbitrary math expressions;
their generic numeric interpolation remains a separate limitation.

`tests/canvas/css-math-scalars.html` provides independent expected values for
64 declaration/font/order combinations, ten inherited values across a live
parent-font change, four actual flex widths after a font change, and two root-font
checks. The same fixture exercises native layout and CSSOM in a unit test.

Animation/transition duration and delay lists retain typed time trees until that
same computed phase. This supports `calc(1em / 10px * 1s)` without interpreting
it as a bare number or using an initial-font approximation. Computed durations
clamp to nonnegative seconds; delays stay signed. Shorthands and longhands replace
their lists atomically, including pending trees. CSS-wide inheritance copies
already-computed seconds; layer restoration retains the pending source until the
final font and viewport are known. Unanimated styles continue sharing the initial
animation settings; only settings with pending calculations require mutation.

Authored CSSOM time serialization retains unresolved expressions rather than
writing the placeholder seconds. The 168-check shared time fixture exercises
longhands, shorthands, negative ranges, live font changes, inherited seconds,
root-relative units, and a paused CSS animation whose native timing and painted
opacity must agree with independent expected values. It also covers 32 computed
`linear()` output checks and 64 iteration-count/timeline/terminal-paint checks.

Chrome 154 rejects the context-dependent `linear()` shorthand in this fixture;
its `CSS.supports()` result is false. This is not reported as a passing Chrome
168-check run. `css-math-times-basic.html` runs the remaining 136 timing and
iteration checks independently; both browsers pass those at 100% and 125%
scale, within 0.02. Breeze's native full fixture retains all 168 assertions.

`linear()` output numbers and animation iteration counts can now retain
font-dependent number trees until computed style. Percentage stop positions
remain percentage-typed. This does not add context-dependent `steps()` counts
or cubic-Bézier coordinates. CSS `animation-timing-function` supplies keyframe
easing, not the effect's overall easing: the fixture separately checks native
keyframe easing, effect progress and actual painted opacity. Iteration keywords
remain distinct from calculated infinity. Negative calculated iteration counts
clamp to zero; negative literals and percentage-valued numbers remain invalid.
Lists preserve authored expressions through CSSOM, replace atomically, inherit
computed values, and retain lower-layer context across `revert-layer`.

## Computed text spacing

CSS Text 3 letter/word spacing resolves lengths after the font cascade, not at
the declaration's earlier position. This fixes declaration-order dependence
for `em` calculations, while preserving computed-length inheritance: a child
with a different font receives the parent's absolute spacing. Root and viewport
units use their actual contexts. Native keyframe resolution shares that phase.
Computed zero letter spacing resolves to `normal`, including explicit `0px`,
as required by the legacy [CSS Text 3 resolved-value rule](https://www.w3.org/TR/css-text-3/#letter-spacing-property).
Normal word spacing serializes as zero pixels. Negative lengths remain permitted.

The supported grammar is CSS Text 3 `normal | <length>`; percentages are not
accepted using an invented font-size basis. This is not CSS Text 4 percentage
spacing, full justification or a new font rasterizer. The 80-check shared
`css-math-spacing.html` fixture checks declaration order, live font changes,
inheritance, keyword serialization, invalid replacement atomicity, root/viewport
context, and actual inline-width deltas for positive and negative spacing.

Chrome 154 admits percentage spacing that this Text 3 slice deliberately does
not implement. Its four percentage-replacement rows therefore differ from the
Text 3 invalid-replacement oracle; they are not counted as passing reference
checks. The remaining spacing comparison also exposed the zero-to-`normal`
resolved-value rule, now covered by native regressions and the shared fixture.

## Variable expansion safety

CSS Variables §3.3 requires a defense against exponentially expanding `var()`
chains. Each substitution has a 1 MiB serialized-byte limit and a 65,536-token
expansion-work budget, shared across nested references and fallbacks. Cumulative
referenced-source scanning is capped at 4 MiB and intermediate serialization at
16 MiB per declaration. These additional work limits cover large comments and
repeated intermediate copies even when the final output stays small. Existing
32-level nesting limits remain. Size/work exhaustion invalidates the winning
declaration at computed-value time; it does not select a fallback as though the
variable were absent. Budgets reset between declarations. The serializer checks
capacity before appending, including token escaping and UTF-8 byte lengths.

Numeric source spellings survive literal preparation and substitutions so that
decimal z-index calculations above 2^24 are not rounded by the tokenizer's f32
payload before the typed f64 evaluator sees them. This does not claim complete
computed custom-property graph resolution or registered-property support.
Native regressions exercise empty-result expansion attacks, large legitimate
single tokens, invalid winners, fallback propagation and subsequent recovery.
The curated suite also includes upstream `variable-exponential-blowup.html`.

## Grid track integration

Integer `repeat()` counts use the same typed number evaluator and CSS integer
rounding as other consumers. `repeat(calc(1 + 2), 50px)` therefore creates three
tracks, rather than silently falling back to one. Length-percentage calculations
inside ordinary tracks and `minmax()` retain their used font and percentage
context. Percentage columns use the container's inner width before subtracting
gutters; a fixed `minmax()` row maximum cannot undercut its fixed minimum.

The shared parser rejects malformed fractions, negative bare breadths, flexible
minima, nested repetitions and invalid replacement declarations atomically.
Expanded explicit tracks are capped at 10,000 in total, including multiple repeat
groups; trailing syntax is still validated after that cap. Positive implicit row
placement and named-area track-vector resizing use the same limit before allocation.
Areas spanning past the limit are truncated; wholly out-of-range areas become one
track at the edge, following CSS Grid's overlarge-grid policy. Negative-line and
general implicit-column placement remain incomplete. The source-size and
nested-token budgets remain in force.

Named line tokens can be retained in declarations, but named-line placement,
automatic repetition, subgrid, `fit-content()` intrinsic clamping and calculations
in `fr` values are not advertised by feature queries. These are not represented
as successful fixed-track approximations. The existing named-area shorthand
still routes quoted rows to its separate rectangular-area parser. General Grid
placement and intrinsic sizing remain incomplete.

`tests/canvas/css-math-grid.html` checks 324 actual rectangle components across
calculated repetition, multiple track groups, fractions, percentages with gaps,
invalid replacements and fixed min/max conflicts at three font sizes. The same
fixture runs in native unit tests with real layout invalidation, not placeholder
zero-valued geometry. Fresh hidden Chrome 154 comparisons at both 100% and 125%
scale pass all 324 expected-value checks in each browser and match all rectangle
components within 0.02 CSS pixels.
