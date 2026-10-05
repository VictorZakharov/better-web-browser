# Canvas gradient contracts

Canvas gradients keep opaque, branded private state shared by Window and
OffscreenCanvas. Color stops remain live after assigning a gradient as paint.
Author properties resembling internal fields cannot replace geometry/stops,
and a forged prototype or Proxy is not a genuine gradient. Captured WeakMap and
array operations protect the retained stop list from replaced intrinsics.

[HTML fill and stroke styles](https://html.spec.whatwg.org/multipage/canvas.html#fill-and-stroke-styles)
requires independent interpolation of color channels and alpha. Breeze now
interpolates straight sRGB values, then applies premultiplied contributions in
the compositor. A transparent red-to-opaque blue midpoint is translucent purple,
not translucent blue. The older test asserting a fully blue midpoint described
our implementation bug and was corrected to assert the normative color.

Stops at equal offsets preserve insertion order. Colors extend before/after the
first/last stop; a degenerate linear gradient paints nothing. Geometry follows
the painting transform, not a newly introduced creation-time transform.
Conic start angles normalize modulo one turn before subtraction, preserving
finite angular differences when an author supplies a very large finite angle.

The creators validate context brands and required arguments before conversions.
Finite `double` conversion uses ToNumber and rejects BigInt, NaN and Infinity
with TypeError. `addColorStop` converts both arguments in Web IDL order before
checking offset range; an out-of-range finite offset still produces
IndexSizeError, an unparseable color SyntaxError, and Symbol color conversion
TypeError. Gradient methods retain their standard prototype identity, arity and
enumerable method descriptor.

The hidden `tests/canvas/gradient-contracts.html` Chrome reference on October 5
produced a midpoint `[129,0,129,127]`, an over-green result `[64,64,64,255]`,
red/blue/blue duplicate-stop samples, and the expected ordered exception names.
Breeze's straight-alpha arithmetic tests assert `[128,0,128,128]` at the midpoint.
The one-byte difference is output quantization, not an assertion of exact
premultiplied storage or complete Canvas color-space conformance. No new
HTML5test feature-detection claim is introduced by correcting these contracts.

## Native gradient source shading

The renderer reuses existing tiny-skia 0.12.0 linear, two-circle radial and sweep
shaders (BSD-3-Clause, already locked through resvg), not a new shader interpreter.
Source colors interpolate without premultiplication in the encoded sRGB channel
space; the native source bitmap is then demultiplied for the shared source-over
compositor. Path coverage and global opacity remain independent factors.

Unclipped source-over path regions and integer-aligned rectangles of at least
256 pixels can use this adapter. The payload snapshots geometry, current paint
transform, live stops and opacity using null-prototype arrays/records. Inherited
author `toJSON` hooks cannot observe those private objects. The input caps at
64 KiB, 256 stops, 16,384-coordinate magnitude, existing Canvas pixel limits and
32 Mi pixel/stop work units (including two potential backend endpoint stops).
Unsupported numeric ranges, larger stop lists and degenerate provider shortcuts
return to scalar painting; they do not become blank successful draws.

The tiny-skia single-stop radial shortcut discards the cone and returns a solid
shader. The adapter duplicates that stop before construction so native radial
geometry still preserves transparent pixels outside the cone. Singular paint
transforms are rejected before native inversion, including zero axis scales.
Output quantization can differ from scalar arithmetic and Chrome; no claim of
pixel-perfect Canvas parity is made.

One fresh hidden run of `tests/canvas/gradient-native.html` took 102.4 ms with the
scalar implementation and 28.2 ms with native shading for twelve 256-square
fills. Samples remained `[[63,0,192,255],[127,0,128,255],[255,0,0,255]]`.
The Chrome reference took 91.3 ms and sampled
`[[63,0,191,255],[126,0,128,255],[254,0,0,255]]`. These targeted single samples
include the fixture's immediate painting work, not a general throughput ranking.
