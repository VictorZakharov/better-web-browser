# ImageData rectangle copying

The implementation follows HTML's [pixel manipulation algorithms](https://html.spec.whatwg.org/multipage/canvas.html#pixel-manipulation).
Reads create independent ImageData storage, copy the bitmap intersection and
retain transparent padding outside it. Writes intersect the dirty rectangle
with source and destination extents before copying. Transform, clipping and
compositing state do not affect these operations.

When each surviving row fills both buffers' strides, the vertical intersection
is a single contiguous byte span. That span now uses one captured typed-array
copy instead of allocating a temporary view for every row. Any horizontal crop
or unequal stride retains row-wise copying, so unrelated pixels and padding
cannot be overwritten. Opaque destination alpha is still normalized only in the
written region. The existing bitmap and detached-storage checks are unchanged.

Internal readback accesses ImageData's private data slot, not an author-replaced
`ImageData.prototype.data` getter. Writing also uses the existing captured
WeakMap lookup. Intrinsic buffer accessors and typed-array writes remain captured;
no author ArrayBuffer view or returned ImageData aliases the Canvas-owned bitmap.

The shared `tests/canvas/image-data-rectangles.html` fixture makes 630 independent
pixel/ownership checks over equal and unequal strides, negative dimensions,
clipped dirty regions, vertical intersections and transparent padding. Window
and Worker tests use the same oracle. A separate test replaces the public data
getter and confirms that internal copying does not invoke it.

## Controlled measurements

`image-data-throughput.html` performs twelve full 512-square put/get round trips
per batch through normal APIs. The context requests CPU/readback-oriented storage.
Every returned pixel is checked outside the timer against an opaque RGBA source.
Three fresh runs per browser alternate Breeze order; the first of four batches
is warmup. No build or CPU sampling runs alongside these measurements.

| Nine warmed batches | Before | After | Chrome 154 |
| --- | ---: | ---: | ---: |
| Median batch | 3.6 ms | 2.8 ms | 5.4 ms |
| Range | 3.2–7.1 ms | 2.6–5.7 ms | 4.9–6.0 ms |
| Stable complete-bitmap checksum | 1375469568 | 1375469568 | 1375469568 |

The small median reduction is about 22% on this workload; the ranges overlap.
It is not a claim of generally faster Canvas, a GPU win, cross-engine alpha
rounding parity, or successful game loading. These opaque pixels avoid
premultiply/unpremultiply differences. Arbitrary translucent round trips can
legitimately differ, as the HTML specification notes.
