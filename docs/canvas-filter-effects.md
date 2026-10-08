# Canvas Filter Effects and bitmap ownership

Canvas filter admission uses the existing CSS tokenizer, component grammar,
typed calculations, length consumers and color parser. It is not a second
regular-expression implementation. Invalid assignments leave the previous
filter unchanged; valid assignments preserve the original getter string.
This includes comments, escaped function names, adjacent functions and CSS
Syntax's end-of-input function closure.

Supported functions are blur, brightness, contrast, drop-shadow, grayscale,
hue-rotate, invert, opacity, saturate and sepia. SVG URL references remain
unsupported; accepting their spelling without executing the referenced filter
would incorrectly advertise support. Each input is limited to 16 KiB and 64
functions, with the CSS parser's existing nesting and work limits.

## Geometry and color

The [HTML drawing model](https://html.spec.whatwg.org/multipage/canvas.html#drawing-model)
filters the source before applying shadows and the final drawing clip.
Off-bitmap ink can therefore contribute a blurred edge or an offset shadow
inside the destination. A bounded transparent halo surrounds the temporary
source. Its translation changes only the private source coordinate space;
the current path, transform, clip and operator are restored before commit.
Drawing a canvas into itself snapshots the original bitmap first.

The filter blur length is Gaussian sigma, unlike the Canvas shadowBlur
attribute whose sigma is half its value. The implementation reuses the pinned
MIT/Apache-2.0 image-rs backend already used by this repository. It filters
premultiplied, gamma-encoded sRGB channels with transparent padding. Readback
converts back to straight channels, with transparent pixels canonicalized to
zero. No additional dependency or copied upstream source is introduced.

Working sets are limited to four Mi pixels, sigma 64, and an estimated 512 Mi
channel-sample operations. The library uses floating-point intermediate
surfaces, so this pixel cap is not a four-byte-per-pixel memory promise.
Admission failure raises NotSupportedError before committing partial pixels;
it does not silently replace a Gaussian with a box blur or clamp large sigma.

## Relative lengths

The [HTML filter contract](https://html.spec.whatwg.org/multipage/canvas.html#filters)
specifies em lengths relative to the font style source object's computed
font size at assignment. The host reads native cascade state rather than an
author-replaceable getComputedStyle method. A disconnected or non-rendered
element and worker OffscreenCanvas use the default 10px basis. Root-font and
viewport units use the document's corresponding native bases. The current
transformation matrix does not scale filter lengths.

The owned readback fixture also records reference-browser behavior. Chrome
154 resolves the tested em offset differently when the context font is 100px
and the element's font size is 2px. This is documented as a reference
divergence, not counted as a matching readback. Unsupported length units are
rejected rather than replaced with guessed metrics.

## Native color-function chains

Contiguous color-function chains use a bounded native straight-sRGB pipeline.
Each primitive clamps its result, but fractional intermediate channels are
retained until final RGBA8 quantization. Combining matrices across those clamps
would change the result, so the backend does not do that. Gaussian and shadow
operations remain explicit boundaries in the original function order.
Coefficients and trigonometric terms are prepared once per operation, not once
per pixel; bridge-owned source storage can be consumed without another copy.
Color admission permits at most 64 operations and 32 million pixel-operation
steps, validated before modifying the temporary bitmap. This uses existing
serialization and ownership guards and adds no dependency.

An owned 128×128 fixture performs 24 paints per workload and includes full pixel
readback in the measured interval. Three fresh, alternating local runs against
merged #229, the intermediate resource-lifecycle build, and Chrome 154 produced
these medians. CPU sampling was disabled; the workload is not a whole-browser
ranking and these are not final-PR-head measurements.

| Completed workload | Merged #229 | Native color chain | Chrome 154 |
| --- | ---: | ---: | ---: |
| brightness(.5) | 38.2 ms | 14.8 ms | 72.0 ms |
| hue-rotate / saturate / sepia | 78.4 ms | 16.9 ms | 113.0 ms |
| brightness / contrast / invert / opacity | 101.9 ms | 16.5 ms | 5.5 ms |

Brightness and the matrix-chain whole-bitmap hashes are unchanged from Breeze's
baseline. The four-function hash intentionally changes because fractional
intermediate values are no longer rounded between functions. Chrome matches
the tested matrix-chain pixels, but differs by one channel level in brightness
and by one or two levels in the opacity chain. Those rounding/premultiplication
differences remain visible; this is not a claim of pixel-perfect Chrome output.

## Origin-clean ownership

Filter Effects marks currentColor-dependent primitives as tainted. Explicit
currentColor and an omitted drop-shadow color therefore taint on drawing,
not merely on assignment. The flag belongs to the bitmap, never save/restore
drawing state. Clearing pixels and putImageData do not clear the flag.
Resetting dimensions creates a new clean bitmap.

Drawing a tainted canvas or ImageBitmap propagates its flag. A CanvasPattern
owns a snapshot including the flag; assigning a tainted pattern as fillStyle
or strokeStyle taints immediately. Resetting the original source cannot
launder the pattern. Author-owned properties cannot overwrite private flags.

getImageData, toDataURL, toBlob, OffscreenCanvas conversion, and pixel-reading
image consumers reject tainted input before exposing bytes. createImageBitmap
can create a tainted bitmap, preserving the flag through crop and resize.
ImageBitmap serialization and transfer through structured cloning instead
reject with DataCloneError before detaching or encoding pixels.

bitmaprenderer takes the source bitmap's flag along with pixel ownership.
Replacing its output with a clean bitmap or null installs a new clean output,
as specified by HTML. OffscreenCanvas.transferToImageBitmap moves the old
bitmap's flag and creates a new blank, clean backing for subsequent drawing.

The owned privacy fixture records all these contracts, including reference
differences: Chrome 154 retained taint after bitmaprenderer replacement in
this run and did not taint the tested OffscreenCanvas currentColor filter.
Unit tests follow the specified ownership contract rather than silently
adopting those differences. This does not extend admission of opaque network
images or claim complete SVG filter support.

Private bitmap operations capture their original weak-map accessors, typed
array constructors and getters, copying operations, and arithmetic functions.
They do not freeze public prototypes or suppress author argument conversions.
Filter lists and saved drawing records are never handed to replaced array
iteration or push/pop methods. The bridge serializer retains its existing
null-prototype wire snapshots. Adversarial tests replace those public methods,
getters and constructors and try to relabel private filter records; neither
protected storage nor its origin flag may reach the callbacks.

Native presentation follows the same rule: records containing backing pixels
use private own-property insertion, not the page's Array.prototype.push.
Connected/detached bookkeeping uses captured collection accessors and indexed
iteration; overridden sort, iterators, or weak-reference methods cannot receive
those private records. Presentation remains allowed for origin-tainted surfaces,
but author readback remains denied. Regression tests check both the denied
readback and the native pixel snapshot, including detach/reattach lifecycle.

## Final-source completed-work replay

Three fresh alternating runs compare merged #229, the complete October 8 source
release, and Chrome 154 using the same owned 128×128 fixture. Timing includes
full readback, CPU sampling is disabled, and no builds/tests run concurrently.
These later samples replace neither the intermediate observations above nor
their explicit pixel limitations.

| Completed workload | Merged #229 | October 8 release | Chrome 154 |
| --- | ---: | ---: | ---: |
| brightness(.5) | 89.2 ms | 29.8 ms | 77.0 ms |
| hue-rotate / saturate / sepia | 174.8 ms | 37.3 ms | 173.0 ms |
| brightness / contrast / invert / opacity | 239.6 ms | 35.3 ms | 8.1 ms |

Before/after brightness and matrix hashes still match exactly; the matrix
pixels also match Chrome. The opacity-chain hash changes for the documented
fractional-intermediate correction, and still differs from Chrome. Both the
baseline and new release timings vary between replay sets. The within-set gains
do not establish isolated causes, identical Chrome pixels, or game acceptance.
