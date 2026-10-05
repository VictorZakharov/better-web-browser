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
