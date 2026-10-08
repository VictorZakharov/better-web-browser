# Stylesheet CSS Animations

Breeze discovers `@keyframes` and computed animation declarations in its native
CSS cascade, then samples their effects through the existing Web Animations
timeline and native style/paint path. This is functional rendering support,
not merely the presence of `CSSAnimation` or an accepted property name.

## Implemented behavior

- The `animation` shorthand and eight longhands have native grammar validation,
  list repetition, CSS-wide keyword handling, case-sensitive names, signed
  delays, iteration counts, directions, fill modes, and play states.
- Named keyframes follow stylesheet order and cascade-layer priority. Conditional
  media/supports rules participate only when active. Shadow-scoped references
  use the scope of the winning `animation-name` declaration, including `:host`
  and slotted rules, with enclosing-scope fallback.
- Duplicate offsets merge in source order. Invalid offsets and declarations,
  `!important`, and animation-control declarations other than per-keyframe
  timing functions do not become animation values.
- Native computed-value resolution handles variables, inheritance, relative
  font lengths, and supported shorthand expansion before interpolation.
  Missing endpoints use the underlying cascade, not the previous sample.
- CSS animations expose real `Animation`/`KeyframeEffect` objects. Name-list
  changes match existing effects from the end, preserving their identity and
  clocks where required. Effect ordering places CSS animations before script
  animations and respects element order and the authored name-list order.
- Authored style changes and stylesheet edits update effects. `display:none`
  on the target or an ancestor and disconnection cancel them; restoring the
  rendered tree creates new animations. Script pause is not undone by an
  unrelated DOM mutation.
- API edits have per-member precedence over subsequent CSS changes, including
  effect/keyframe replacement and calls through `Animation.prototype`. Pending
  stylesheet changes are flushed before CSS-owned effect queries and updates.
  The [typed math and ownership contracts](css-math-functions.md) describe the
  implemented timing, easing and translation subset and explicit limits.
- `animationstart`, `animationiteration`, `animationend`, and `animationcancel`
  bubble with `AnimationEvent` data and active elapsed time. Event delivery is
  queued rather than reentering script during native discovery.
- `CSSKeyframesRule`, `CSSKeyframeRule`, live rule lists, parent links, key-text
  parsing, `appendRule`, `findRule`, and last-match deletion edit the native
  stylesheet source. Ordinary, inline, and keyframe declarations share the
  existing native declaration parser for token boundaries and value validation.

Animation and transition samples occupy separate cascade origins. Animations
remain below author `!important`; transitions sit above it. Samples never write
author style attributes or stylesheet declarations. Overlay writes invalidate
presentation without incrementing the authored-change discovery revision.

## Resource contract

Discovery admits at most 64 targets, 16 CSS animations per target, 256 frames
per effect, and 64 interpolated properties per frame. The native snapshot has
an aggregate 1 MiB UTF-8/20,000-value budget; admission stops atomically when
that budget is exhausted. Discovery traversals are limited to 100,000 nodes,
and keyframe collection to 512 definitions and existing CSS nesting/source
limits. Lifecycle-event queues are bounded to 2,048 entries.

These are resource safeguards, not standards-defined limits. Effects beyond
admission limits are not started. The public script-created Web Animations
contract remains separately bounded to 64 frames and 32 properties. A large
time jump does not synthesize an unbounded iteration-event burst.

## Remaining gaps

This is not full CSS Animations conformance. Pseudo-element animations,
scroll-driven timelines, additive/accumulative compositing, compositor
offloading, unsupported/discrete property interpolation, and general 3D
transform interpolation remain open. Existing interpolation limitations apply
to complex mixed-unit calculations. CSSOM does not yet implement every
shorthand expansion or shortest-value serialization rule for all properties.
Animation shorthand CSSOM readback is not a complete canonical serialization.
For native properties lacking a complete value validator, CSSOM preserves
token-valid authored values rather than using the stricter capability-query
result to delete unrelated declarations. Full property-specific grammar checks
for that remainder are still open; `CSS.supports()` is not broadened by this.

Timers and effects execute in the renderer's existing bounded runtime. This
batch does not replace its scheduler or promise complete browser-equivalent
frame pacing under heavy script or layout work.

Computed styles share immutable initial animation lists. Editing an animation
property copies those lists on demand; ordinary static properties do not.
Regression tests cover both sharing and isolation after a write.

## Verification and provenance

Behavior tests assert sampled native computed styles, lifecycle, ordering,
shadow/layer selection, CSSOM mutations, and resource boundaries. A shared
hidden Chromium fixture checks deterministic paused samples without browser
or site-specific branches. Curated upstream parsing tests supplement these
tests; they do not establish complete conformance.

No new dependency or copied implementation is introduced. Parsing reuses the
repository's existing `cssparser` dependency; playback and interpolation reuse
its Web Animations implementation. Compatibility decisions follow:

- [CSS Animations Level 1](https://drafts.csswg.org/css-animations-1/)
- [CSS Animations Level 2 API precedence](https://drafts.csswg.org/css-animations-2/#animations)
- [CSS Scoping: names](https://drafts.csswg.org/css-scoping-1/#shadow-names)
- [CSS Cascade: layers](https://drafts.csswg.org/css-cascade-5/#layering)
- [CSSOM declaration operations](https://drafts.csswg.org/cssom/)
- [CSS Easing Level 2](https://drafts.csswg.org/css-easing-2/)
- [Web Animations Level 1](https://www.w3.org/TR/web-animations-1/)
