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
