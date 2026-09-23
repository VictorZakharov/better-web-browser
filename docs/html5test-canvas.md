# Bounded Canvas and Geometry capability slice

The Windows x64 hidden release run of HTML5test.co moved from **323 / 588** to
**334 / 588** for the first Canvas 2D slice, then to **337 / 588** for the
bitmap/OffscreenCanvas slice on 2026-09-23 (1280×720, 125% scale, `en-US`,
fresh profile, 5-second settle). These are the last measured results before
the additional path, paint, geometry, and DOM work in PR #180; do not interpret
them as the score of the final branch. The score is a detected-capability count,
not a Canvas or browser conformance score. Reproduce it using the README command.

## Bitmap and image sources

- HTML Canvas and OffscreenCanvas own a bounded software bitmap of at most
  4,194,304 sRGB pixels. `ImageData`, fill/clear operations, `putImageData`
  (including its dirty rectangle), and readback use real pixels. Resizing or
  calling `CanvasRenderingContext2D.reset()` clears the bitmap and drawing state.
- `toDataURL()`, asynchronous `toBlob()`, and `convertToBlob()` serialize
  actual PNG, JPEG, or WebP bytes using the existing `image` dependency.
  Unsupported MIME types use PNG. Zero-sized canvases produce `data:,` or
  a null blob. JPEG quality is accepted in the standard `[0, 1]` range;
  encoded output is capped at 24 MiB.
- `ImageBitmap` owns a closeable snapshot. `createImageBitmap()` accepts
  `ImageData`, HTML/Offscreen Canvas, another bitmap, and decoded PNG/JPEG/WebP
  Blob input, with crop, vertical orientation flip, and resize options.
  `drawImage()` samples Canvas and ImageBitmap sources with nearest or
  premultiplied-alpha bilinear interpolation. A zero-area or nonfinite draw
  is a no-op even under a whole-surface composite operator.
- `bitmaprenderer` consumes an ImageBitmap into an exclusive Canvas context.
  OffscreenCanvas shares the same 2D implementation in dedicated workers.
  Structured clone copies bitmaps and transfers ImageBitmap/OffscreenCanvas
  with sender detachment. A transferred DOM placeholder can export the live
  bitmap; worker tests cover path paint, filters, geometry, and reset.

## Paths, paint, and state

- Path2D handles empty/copy construction and SVG path strings with absolute
  and relative M/L/H/V/C/S/Q/T/A/Z commands. Smooth control reflection and
  elliptical arc conversion feed bounded flattened geometry. `addPath()`
  accepts a two-dimensional affine transform. The current path is separate
  from saved context drawing state.
- Rectangles, lines, quadratic/cubic curves, arcs, ellipses, `arcTo`, and
  `roundRect` are rasterized. Nonzero and evenodd fill rules, clipping,
  strokes, dash patterns, caps, joins, miter limits, and point-in-path/stroke
  queries operate on geometry and real pixels. `strokeRect()` uses the same
  stroke path. Affine context transforms affect drawing and paint sampling.
- Live linear, radial, and conic gradients use sorted stops and premultiplied
  alpha interpolation. CanvasPattern supports repetition variants and a
  pattern transform. Gradients and patterns can paint fills and strokes.
- Source-over, Porter–Duff operators, separable and nonseparable blend modes,
  global alpha, Canvas shadows, and a bounded Filter Effects function list
  operate on temporary source layers before final compositing. Filters include
  color matrices/functions, blur, and drop-shadow. Invalid property values
  preserve the previous drawing state.
- DOMMatrix/DOMMatrixReadOnly, DOMPoint/DOMPointReadOnly, DOMQuad, and DOMRect
  support the Geometry Interfaces used by Canvas transforms and patterns.
  Their structured-clone form preserves finite values, NaN, infinities,
  and negative zero. Window and dedicated-worker APIs share the same math.

## Intentional limits

This is not full Canvas 2D. The bitmap is not painted into the page display
list, including an OffscreenCanvas linked to a DOM placeholder. Text drawing,
HTML image-element and video image sources, pixel antialiasing, color spaces
other than sRGB, and CSS filter URLs are not implemented. Blur uses a bounded
box approximation; curves and arcs use bounded line-segment flattening.
Large bitmaps, paths, and raster workloads fail with `NotSupportedError`
rather than consuming unbounded renderer resources. WebGL/WebGPU remain a
separate feature inside this repository ([issue #181](https://github.com/VictorZakharov/better-web-browser/issues/181));
this work does not advertise a partial WebGL context.

Behavioral tests cover encoding, bitmap ownership/transfer, worker parity,
path construction, pixel paint and compositing, clipping, stroke geometry,
transforms, gradients, patterns, shadows, filters, and invalid inputs.
Normative references: [HTML Canvas 2D](https://html.spec.whatwg.org/multipage/canvas.html),
[SVG 2 paths](https://www.w3.org/TR/SVG2/paths.html),
[Geometry Interfaces](https://www.w3.org/TR/geometry-1/),
[CSS Compositing](https://drafts.fxtf.org/compositing-1/), and
[Filter Effects](https://drafts.fxtf.org/filter-effects-1/).
