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

Stroke coverage reuses the existing BSD-3-Clause `tiny-skia` rasterizer
through `resvg`, with bounded path serialization, point count and mask dimensions.
The context still owns paint, clipping, alpha and compositing. Numeric ranges
outside this native adapter's bounds select the existing bounded software path;
they are not silently replaced with an empty image. Native masks now carry
fractional stroke coverage; the extreme-range software fallback remains binary.
Stroke outlines now use the painting-time affine pen, including non-uniform
scale, rotation, reflection and shear. The current default path retains its
construction-time geometry; a supplied `Path2D` is transformed without mutation.
Inverse-coordinate hit testing and dashed outlines use the same pen coordinate
system. Singular painting transforms have no stroke
area. Tests compare interior/exterior pixels with an independent inverse-matrix
rectangle oracle, and separately check elliptical round caps and path retention.
The `tests/canvas/affine-stroke.html` Chrome reference reports `[255,0,0]`
for each scaled-width, retained-path and elliptical-cap sample triple; a singular
pen produces neither nonzero pixels nor a hit. These samples match the unit
tests, not a claim of full antialiasing parity.

Native dashing runs before stroking, so each on interval receives its own caps,
gaps remove original corner joins, and each subpath restarts its phase. Closed
contours retain a join across the seam only when an on run is continuous there.
All-zero patterns are solid; zero-length on intervals can produce round dots.
Dashed hit queries inspect the same transformed native outline with the existing
MIT/Apache-2.0 `kurbo` curve library, already locked through `usvg`. Boundary
points are included, independently of the drawing clip or bitmap bounds.
The input, dash entries, expansion transitions and final outline have explicit
budgets. Positive intervals that would underflow native float precision do not
silently become an all-zero solid pattern. Requests outside the adapter's bounds
still select the prior bounded software path, whose extreme-range dash geometry
is not claimed to have full parity.

Native, window and worker tests cover caps, joins, subpath phase, negative
offsets, empty off runs, affine pens, state ownership and path retention. The
local `tests/canvas/dashed-stroke.html` fixture matches Chrome's cap/corner hit
results, reset/gap samples and equivalent negative phase. Chrome's zero-on round
dot edge had alpha 213 where Breeze's preceding binary mask had 255. The native
coverage change described below supersedes that binary edge result.

Antialiased strokes combine geometric coverage with source opacity exactly once,
before filters/shadow generation and full-surface Porter-Duff composition.
The same rule applies to ordinary paints, gradients and patterns. A bounded
higher-resolution tiny-skia mask reduces the library's coarse scan-conversion
steps, then area-averages to source coverage. Scratch is capped at 8,388,608 mask
bytes; large regions retain native-resolution antialiasing. Integer clips remain
packed binary regions, and very large native masks may retain the provider's
binary fallback. This is not full antialiased clipping/fill/image parity.

Tests use an independent polygon/pixel intersection-area oracle for affine pens,
including edge pixels, instead of a center-hit/binary-alpha approximation.
Separate tests verify fractional opacity, overlapping-outline union, source-layer
operators, clips and shadow alpha. In the hidden `stroke-coverage.html` fixture,
the half-covered one-pixel stroke has alpha 128 in Breeze versus 127 in Chrome;
both produce 64 at half global opacity and 255 when pixel-aligned.
Remaining differences are recorded rather than ignored: existing straight-alpha
bitmap arithmetic differs from Chrome's premultiplied quantization in some blend
channels, and the sampled `arcTo` edge has alpha 160 versus Chrome's 170 after
adaptive curve flattening (previously 191). Raster coverage and premultiplied bitmap storage remain follow-up
work; these fixtures are not a claim of pixel-perfect Canvas rendering.

Rectangle drawing retains Web IDL double coordinates, normalizes negative sizes,
and converts author arguments once in order. Axis-aligned fill coverage is interval
overlap; general affine rectangles integrate their convex polygon over each pixel
square. This does not change the current path or ImageData's integer coordinates.
`clearRect` ignores paint effects and erases whole pixels, with nearest-edge
intervals for axis-aligned clears matching the hidden Chrome reference. Saved
integer clips remain binary, including for fractional rectangles.

Opaque, integer-aligned solid fills and clears use bounded typed-array row copies,
including axis swaps/reflections. Translucency, blends, clips, gradients/patterns
and fractional edges retain the general painting path; source-layer generation
still precedes shadows, filters and compositing. No bitmap ownership or resource
limit changes are required. Captured copy/fill intrinsics avoid invoking replaced
author prototype methods during these row operations.

The hidden `rectangle-coverage.html` probe draws twenty 256-square opaque fills
and includes a final pixel readback in the timed interval. Three fresh-profile
runs on 2026-10-05 measured Breeze medians of 72.7 ms before and 1.1 ms after;
Chrome measured 0.2 ms. These tiny fixture timings are not game throughput or
end-to-end startup measurements. Both produce the correct final red pixel.
Chrome matches clear erasure, shear coverage and conversion order; reflected
edge alpha differs by one. A quarter-offset square has exact-area alpha 143 in
Breeze versus Chrome's rasterized 191. The implementation fixes coordinate
truncation, but does not claim to match every rasterizer's antialiasing convention.

Filled paths also reuse the existing tiny-skia compound-path rasterizer, with the
same bounded coverage-mask owner as strokes. Nonzero/evenodd winding is applied
to all contours together before opacity; duplicated overlapping subpaths do not
darken twice. Open contours are implicitly closed for filling without altering
their later stroke geometry. The stored default path remains in construction-time
bitmap coordinates, while retained Path2D instances apply the painting transform.
Paint sources, clips, source layers, filters and shadows retain their existing
owners. Out-of-range native geometry selects the existing binary bounded fallback.

Native admission tests cover invalid requests, point/mask budgets, empty geometry,
opposite winding and ROI offsets. Window and worker tests cover fractional alpha,
overlap, holes, transforms, open paths and effect ordering. In the hidden
`fill-coverage.html` fixture, Breeze matches Chrome's half-covered edge (alpha 64
at half opacity), duplicated-contour union, evenodd cancellation and open-path
geometry. With adaptive curves, the sampled filled arc has alpha 231 versus
Chrome's 216 (previously 202), and the rounded corner has 79 versus 86 (previously 63). These
are recorded limitations, not evidence of full curve or clipping parity.

Curve construction reuses the existing MIT/Apache-2.0 kurbo dependency instead
of fixed 24-segment Béziers and coarse small-arc sampling. The bounded native
adapter flattens transformed quadratic/cubic curves and ellipses with a 0.025
bitmap-coordinate error budget. Arc-to-cubic and cubic-to-line approximation
split that budget; transformed controls are checked before adaptive work.
Requests cap their serialized input at 2 KiB, coordinates at 16,384, and retained
points at the existing 8,192-point path limit. Out-of-range requests retain the
bounded software fallback. Path2D stores author-coordinate geometry; scaling an
already-flattened retained path can still magnify its approximation error.

An empty Bézier subpath begins at its first control point, and a curve following
closePath starts from the closed contour's first point. Arc angles normalize with
modulo rather than repeated turn subtraction, so very large finite inputs cannot
spin indefinitely. Native tests measure distance from independently sampled
analytic curves; window/worker tests cover origins, construction transforms,
closed-contour continuation, degenerate arcs and geometry budgets. These changes
improve real Canvas behavior without introducing a new feature-detection claim.

Source-over compositing uses scalar premultiplied-alpha arithmetic without
allocating temporary arrays or per-pixel typed-array views. An independent
alpha oracle, repeated 512-square closed paths and overlapping clipped strokes
cover the optimized paths.

Canvas shadow generation reuses the existing MIT/Apache-2.0 `image` library's
separable Gaussian filter, with the HTML `shadowBlur / 2` deviation. Alpha is
normalized floating point, padded with transparent pixels, and filtered only
over its occupied region. The native adapter caps sigma at 64 and working
storage at 8,388,608 pixels; requests outside its storage/dimension bounds
retain the older bounded software fallback. This is not a claim that every
Canvas filter or out-of-bitmap shadow source is fully conformant.

Shadow eligibility requires nonzero blur or offset, not just a nontransparent
shadow color. The drawing clip is applied after source/shadow generation, and
the shadow and source are composited separately using the selected operator.
The local `tests/canvas/shadow-gaussian.html` fixture produced alpha samples
`[4,23,67,106,117,99,55,16]` in Breeze versus
`[4,23,67,106,116,98,55,16]` in hidden Chrome on October 5. Its clipped shadow
sample matches exactly: `[0,0,255,255]`. Whole-page layout/font parity is not
inferred from these pixel samples.
Normative references: [HTML Canvas 2D](https://html.spec.whatwg.org/multipage/canvas.html),
[SVG 2 paths](https://www.w3.org/TR/SVG2/paths.html),
[Geometry Interfaces](https://www.w3.org/TR/geometry-1/),
[CSS Compositing](https://drafts.fxtf.org/compositing-1/), and
[Filter Effects](https://drafts.fxtf.org/filter-effects-1/).
