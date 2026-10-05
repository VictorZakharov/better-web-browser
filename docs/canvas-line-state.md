# Canvas line-state conversion

Line width, miter limit, dash offset, caps, joins and dash sequences share one
implementation in Window and Worker. Their descriptors are enumerable and
configurable on both context prototypes. Receivers are checked before argument
conversion. Numeric attributes use Web IDL ToNumber semantics, rejecting BigInt
and Symbol instead of accepting `Number(BigInt)`. Nonfinite values preserve
existing state; width and miter limit also ignore zero and negative values.
Cap and join values undergo DOMString conversion once, before validating the
allowed values. Invalid strings preserve the existing enum state.

`setLineDash` converts an iterable object to a sequence of unrestricted doubles.
It obtains the iterator and its next method once, converts each yielded value
before requesting another, and rejects malformed iterator results. A numeric
conversion failure propagates without calling an author's iterator return
method, following [Web IDL sequence conversion](https://webidl.spec.whatwg.org/#create-sequence-from-iterable).
The previous drawing state is unchanged on failure. The
[HTML line-style algorithm](https://html.spec.whatwg.org/multipage/canvas.html#line-styles)
then rejects nonfinite/negative entries, duplicates an odd sequence, and stores
an owned copy. Returned dash arrays are independent copies. This does not alter
native stroke budgets, curve flattening or the existing drawing watchdog.

The shared `tests/canvas/line-state.html` fixture checks conversion order,
exceptions, descriptors, owned copies, save/restore, ignored invalid input and
real dashed stroke pixels. The Chrome reference produced red inside the dash
and transparent pixels in its gap, with the complete expected error/trace
payload. Window and Worker tests run the same assertions.
