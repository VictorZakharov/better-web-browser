# Exact-center Canvas image copies

The owned native Canvas painter recognizes a narrow 1:1 image-copy case before
using the existing general affine sampler. This applies to ordinary `drawImage`
from every admitted source, in Window and dedicated Worker; it is not a game or
site-specific path. The public overload conversion, source usability checks,
origin-clean propagation, source snapshot and native storage bounds are unchanged.

An admitted copy has an identity linear transform, integer inverse translation,
equal positive source/destination extents, full global alpha and source-over
composition. Coordinates and extents are limited to exactly representable small
integers. These conditions map every included destination pixel center to an
exact source texel center with either smoothing setting. Fractional, scaled,
rotated, large-coordinate and other-compositing cases retain the original scalar
evaluation order, including its rounding at transformed boundaries.

The copy intersects destination bounds, the drawn rectangle and the original
source bitmap. Out-of-image crops do not smear an edge texel into untouched
pixels. An opaque unclipped row copies only its admitted byte span. Mixed-alpha
or clipped rows reuse the original source-over arithmetic and the full
destination clip stride. Individual opaque source pixels and pixels over a
zero-alpha backdrop likewise replace channels directly; a covered all-transparent
result clears hidden RGB, while a clipped pixel is untouched. Every integer color
and source alpha is checked against the scalar equation for this case. Other
alpha combinations retain the original arithmetic. No source buffer aliases
the owned destination, including during self-drawing. There is no SIMD or
third-party pixel-format substitution that
would silently quantize the existing straight-alpha contract.

Native unit tests compare the unchanged scalar sampler byte-for-byte, including
all 65,536 source/backdrop alpha pairs, deterministic crop/translation/bounds/clip
combinations, opaque row padding, empty intersections and exact-coordinate
admission boundaries. Public tests compare native-sized draws with equivalent
small JavaScript-sampled tiles and overlapping self-draws in both realms. The
general sampling, filters, blend modes, origin-clean and IDL suites still run.

Performance requires separate release measurements. Correct pixels alone are
not evidence that copying dominates game startup or that this matches Chrome.
For cross-browser frame benchmarks, include a pixel readback completion barrier
inside the timed block; timing only command submission favors deferred GPU work.

Compatibility basis: [HTML Canvas image drawing](https://html.spec.whatwg.org/multipage/canvas.html#drawing-images).
