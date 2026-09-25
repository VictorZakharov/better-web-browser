# CSS rectangular clip paths

The first [CSS Masking `clip-path`](https://www.w3.org/TR/css-masking-1/#the-clip-path)
slice implements `none` and the rectangular `inset()` basic shape. This is a
shared rendering primitive, not a site-specific styling rule.

- Parse one to four lengths or percentages with the normal CSS length parser;
  resolve horizontal percentages against the border-box width and vertical
  percentages against its height. Viewport and font-relative units follow the
  normal computed-style resolution path.
- Keep the unclipped layout box and CSSOM geometry. Enclose the element's own
  paint and descendants in a retained rectangular clip; an inset clip creates
  a stacking context. The same clip bounds constrain native hit testing.
- Project ancestor clips into the observer viewport before computing
  `IntersectionObserver` entries. The [observer algorithm](https://w3c.github.io/IntersectionObserver/#compute-the-intersection)
  applies `scrollMargin` to a scroll container's clip, including a basic-shape
  clip; non-scrolling shape clips are not expanded.
- Expose the computed value through CSSOM and report support only for parsed
  values. `CSS.supports('clip-path', 'circle(50%)')` remains false.

The strict pinned WPT gate passes **455/455 files and 4,322/4,322 assertions**
with `intersection-observer/scroll-margin-clip-path.html` newly enabled. Local
tests cover shorthand parsing, non-inheritance and CSS-wide keywords, paint
group bounds, translated hit regions, computed-style serialization, and
unsupported-shape reporting. The upstream test is unmodified.

This is not complete CSS Masking support. Rounded `inset()`, circles, ellipses,
polygons, paths, SVG clip sources, and non-rectangular hit testing still need a
shape-aware paint backend. Their values are deliberately not advertised as
supported. Inline fragment clipping is also not covered by this first layout
slice.

```powershell
./scripts/run-wpt.ps1 -WptRoot ../wpt -Filter 'intersection-observer/'
cargo test --locked --lib clip_path
```
