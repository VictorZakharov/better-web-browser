# Bounded Canvas and Geometry capability slice

The Windows x64 hidden release run of HTML5test.co moved from **323 / 588** to
**334 / 588** for the first Canvas 2D slice, then to **337 / 588** for the
bitmap/OffscreenCanvas slice on 2026-09-23 (1280×720, 125% scale, `en-US`,
fresh profile, 5-second settle). The expanded path, paint, geometry, and DOM
batch in PR #180 also measured **337 / 588** with HTTP 200, zero JavaScript
errors, and no renderer exit. Thus this batch improves tested behavior but
does not raise this particular score. The test page logs that its drag/drop
and editing groups are blacklisted for an unknown browser, and it does not
award new points for the added Canvas/Geometry operations. The score is a
detected-capability count, not a conformance measure. Reproduce it using the
README command.

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

## Embedded DOM presentation

- With scripting enabled, an HTML Canvas is laid out as a replaced element with
  its `width`/`height` content attributes as intrinsic dimensions (default
  300×150). Its transparent bitmap or painted pixels are placed in the page
  display list, scaled by the existing replaced-image paint path. Canvas
  fallback children remain available when scripting is disabled.
- 2D drawing, `bitmaprenderer` transfers, `reset()`, and dimension changes
  invalidate the presentation. At a rendering checkpoint, the renderer sends
  actual straight-alpha RGBA pixels through the decoded-image transport;
  presentation converts them to premultiplied BGRA. Subsequent paints replace
  the same resource key and invalidate its browser-side bitmap cache. The same
  path composes same-process child documents into their iframe paint boxes;
  child Canvas repaints update the bitmap and removing an iframe retires it.
- Up to 64 connected Canvas elements can be tracked, each at most 4,194,304
  pixels, with a 64 MiB paint batch and the existing 64 MiB decoded-image page
  budget. Child-frame decoded images share a separate 64 MiB budget. Detached
  elements relinquish their tracked slot and retire their browser-side bitmap;
  a reattached Canvas republishes its retained pixels without a new draw.
  Tracking detached surfaces is bounded to 64 weak references, so an older
  evicted surface needs a new draw after reattachment. Oversized or
  zero-sized canvases keep their replaced box without a bitmap. A same-realm
  OffscreenCanvas linked by `transferControlToOffscreen()` paints its DOM
  placeholder; a placeholder transferred to a worker does not yet stream
  worker-side updates into the page.
- The 64 MiB renderer presentation wire limit includes layout, accessibility,
  glyphs, image keys, and pixels. When root and child-frame image deltas do not
  fit together, the renderer sends bounded subsets over successive rendering
  checkpoints, prioritizing images never sent and rotating already-sent
  repaints. An update is acknowledged only after its pixels are emitted. A
  single decoded image that cannot fit alongside required metadata cannot be
  carried by the current atomic image format; it remains cached for a later
  checkpoint, reports one diagnostic, and does not cause an immediate retry
  loop.
  The earlier Canvas export batch also retains dirty bitmaps beyond its own
  64 MiB limit and requests another checkpoint; it does not turn overflow into
  a null bitmap (which would mean removing the previously presented pixels).

The relevant HTML requirements are the Canvas element's [intrinsic dimensions,
fallback content, and bitmap](https://html.spec.whatwg.org/multipage/canvas.html#the-canvas-element)
and its [embedded-content rendering](https://html.spec.whatwg.org/multipage/rendering.html#embedded-content-rendering-rules).

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

This is not full Canvas 2D. Pixel antialiasing, color spaces other than sRGB,
and CSS filter URLs are not implemented. Blur uses a bounded
box approximation; curves and arcs use bounded line-segment flattening.
Large bitmaps, paths, and raster workloads fail with `NotSupportedError`
rather than consuming unbounded renderer resources. The independent native
WebGL implementation is described in [its contract](webgl2-foundations.md);
Canvas 2D feature probes are not evidence of WebGL or WebGPU completeness.

## Raw pixel transfer

`getImageData` and `putImageData` copy clipped contiguous row spans rather than
allocating a temporary typed-array view for each pixel. Readback outside the
bitmap remains transparent black. Dirty rectangles are normalized and
intersected with both source and destination before copying. These operations
still bypass transforms, drawing clips and compositing.

Pixel coordinates use the HTML interface's `[EnforceRange] long` conversion:
truncate finite fractional values and reject non-finite, out-of-range, BigInt
and Symbol inputs with `TypeError`. Detached ImageData storage instead raises
`InvalidStateError`. Source data and context identity come from private owners;
forged prototypes or author-replaced byte getters cannot select another bitmap.
The retained 4-megapixel bitmap budget bounds both copies and temporary memory.

Regression tests compare an independent pixel-by-pixel clipping oracle with
row copying, check overload selection and error behavior, and exercise repeated
1024-square texture-map round trips under the unchanged script watchdog.

Behavioral tests cover encoding, bitmap ownership/transfer, worker parity,
path construction, pixel paint and compositing, clipping, stroke geometry,
transforms, gradients, patterns, shadows, filters, and invalid inputs.
Normative references: [HTML Canvas 2D](https://html.spec.whatwg.org/multipage/canvas.html),
[SVG 2 paths](https://www.w3.org/TR/SVG2/paths.html),
[Geometry Interfaces](https://www.w3.org/TR/geometry-1/),
[CSS Compositing](https://drafts.fxtf.org/compositing-1/), and
[Filter Effects](https://drafts.fxtf.org/filter-effects-1/).
