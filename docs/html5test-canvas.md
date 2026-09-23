# Bounded Canvas 2D capability slice

The Windows x64 hidden release run of HTML5test.co moved from **323 / 588** to
**334 / 588** on 2026-09-23 (1280×720, 125% scale, `en-US`, fresh profile,
5-second settle). The score is a detected-capability count, not a Canvas or
browser conformance score. The benchmark completed with HTTP 200, no JavaScript
errors, and no renderer exit. Reproduce it using the command in the README.

## Implemented behavior

- The existing Canvas 2D bitmap remains capped at 4,194,304 sRGB pixels.
  `ImageData`, `fillRect`, `clearRect`, and `putImageData` operate on those
  pixels, and resizing resets the context.
- `toDataURL()` and asynchronous-callback `toBlob()` serialize the bitmap as
  actual PNG, JPEG, or WebP bytes using the repository's existing `image`
  dependency. Unsupported MIME types use PNG. Zero-sized canvases return
  `data:,` or a null blob. JPEG quality accepts the standard `[0, 1]` range;
  encoded output is capped at 24 MiB.
- `Path2D` supports empty/copy construction, line/rectangle/ellipse/arc,
  flattened quadratic and cubic curves, a strict SVG M/L/H/V/Z subset, and
  `addPath()` with a two-dimensional affine transform. The current context path
  is separate from saved drawing state. Nonzero/evenodd fills, basic strokes,
  dash patterns, and point-in-path/stroke tests use actual geometry and pixels.
- Linear and radial `CanvasGradient` objects have live, sorted color stops.
  Color interpolation uses premultiplied alpha; gradients can paint rectangles
  and paths, including strokes.
- `globalCompositeOperation` currently supports `source-over`,
  `destination-over`, `copy`, `lighter`, `multiply`, and `screen` on the owned
  bitmap. Unsupported operators are ignored by the setter.

## Intentional limits

This is not full Canvas 2D. The bitmap is not yet painted into the page's
display list. Text, image drawing, patterns, clipping, shadows, transforms,
conic gradients, nonseparable blend modes, most SVG path-string commands, and
pixel antialiasing are not implemented. Stroke caps and joins are simplified;
ellipse/curve geometry is flattened to bounded line segments. Large bitmaps,
paths, and raster workloads fail with `NotSupportedError` rather than hanging
the renderer. The implementation advertises only methods with actual behavior,
not no-op signatures for score probes.

Behavioral tests cover decoded PNG/JPEG/WebP output, asynchronous blob delivery,
blend pixels, path fill rules, ellipse and dashed strokes, SVG input, affine
`addPath()`, gradient interpolation, and invalid inputs. The relevant normative
references are [HTML Canvas 2D](https://html.spec.whatwg.org/multipage/canvas.html)
and [CSS Compositing and Blending Level 1](https://drafts.fxtf.org/compositing-1/).
