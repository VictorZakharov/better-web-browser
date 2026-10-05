# CanvasPattern and 2D matrix dictionaries

Canvas patterns are opaque paint objects with an immutable source snapshot and a
live pattern transform. Source, repetition and matrix now live in private
implementation slots rather than writable `__` properties. Genuine objects keep
their brand after prototype changes; forged prototypes do not gain the method
receiver or paint-object contracts. Window and Worker share this implementation.

The [HTML Canvas contract](https://html.spec.whatwg.org/multipage/canvas.html#dom-canvaspattern-settransform)
uses `DOMMatrix2DInit`, not the DOMMatrix constructor's string/sequence overloads.
One shared converter handles pattern transforms, Canvas `setTransform`, and
`Path2D.addPath`: it reads twelve dictionary members once, in Web IDL name order,
converts each supplied number once, checks short/long aliases, and ignores
unknown members and iterators. Matching NaN and signed-zero aliases agree.
Nonfinite completed matrices leave the existing pattern/context/path unchanged.

Canvas numeric transforms validate receivers and required arguments, convert
left-to-right with unrestricted-double semantics, reject BigInt, and ignore
surplus arguments. Scale, translate and rotate no longer invoke an
author-replaceable public `transform` method. Path addition honors long aliases,
validates the source interface, and retains the existing bounded geometry budget.

`createPattern` requires its two IDL arguments, validates the image interface
before converting repetition, and takes the source snapshot afterward. Empty or
legacy-null repetition means repeat; explicit undefined is not an optional
default. A zero-sized Canvas raises `InvalidStateError`, while an incomplete
image returns null. Existing readable-image/origin policy remains in force.

Shared Window/Worker tests and the standalone `pattern-contracts.html` and
`matrix-contracts.html` fixtures cover source mutation, live matrices, opaque
state, method descriptors, conversion order/errors, aliases and real path/pixel
results. Both fixtures passed a fresh hidden Chrome reference run with their
complete expected result payloads. This slice does not add an HTML5test-only
capability or claim complete image-source/filtering support.

## Bounded native bitmap painting

Repeating RGBA8 snapshots now use the existing BSD-3-Clause tiny-skia bitmap
shader for source-over regions of at least 256 pixels, without a clip. The
context and live pattern matrices compose before converting to region-local
coordinates. Geometry coverage and global opacity are applied exactly once by
the shared owned shader compositor, also used by gradients. Native input uses
closed requests, checked buffer sizes, a one-megapixel source limit and the
existing four-megapixel destination limit. Unsupported repetition, float16,
clipping, operators and unsafe transforms retain the managed fallback.

Sampling remains nearest-neighbor; this does not claim complete image filtering.
The native RGBA8 shader stores premultiplied pixels, so translucent source colors
can incur the usual premultiplication/demultiplication quantization. Tests cover
repeat phase, scaling, region origins, masks, opacity, closed malformed requests,
owned input preservation, and Window/Worker agreement with clipped fallback.
The shared affine/opacity fixture passed hidden Chrome with identical sample
pixels and no failed assertions.

In a fresh-profile optimized-iteration measurement on October 5, eight 256-square
fills with changing live transforms took 102.7 ms before and 14.5 ms after.
All four sampled pixels were unchanged and matched Chrome. These are single-run
Canvas-call timings, not sustained rendering throughput: Chrome measured 0.1 ms
but may defer bitmap work until the untimed readback. No HTML5test score change
or playable-game milestone is claimed from this optimization.
