# Owned Canvas compositing

Canvas source layers include transparent pixels outside the drawn shape. They
cannot be reduced to calls that paint only covered geometry: `copy`,
`source-in`, `destination-in`, and related Porter–Duff operators affect the
backdrop outside that geometry as well. Shadows and source images are separate
compositing operations; filters run before those operations and the drawing
clip applies afterward.

The native `canvasCompositeLayer` operation consumes owned RGBA8 source and
destination bytes and an optional packed clip bitmap. It evaluates the same
straight-alpha sRGB equations as the scalar Canvas implementation. Each result
is separately owned; neither the source snapshot nor a previous returned result
can modify later operations. Unknown operators, malformed lengths, incorrect
clip lengths, empty bitmaps and requests beyond the four-million-pixel budget
decline before drawing. The existing bounded scalar path remains the fallback.

The backend handles the eleven Canvas Porter–Duff operators, eleven separable
blend modes and four nonseparable blend modes. A source with zero alpha must
still be evaluated: it clears the destination for `copy` and can remove the
backdrop for source/destination-in. Clipped-out pixels retain their original
bytes, including any hidden RGB in transparent pixels.

This is not a new rasterizer or an imported graphics package. Geometry and
gradient/pattern shading continue to use the already licensed tiny-skia backend.
Compositing uses the existing scalar model's f64 arithmetic and final RGBA8
rounding. Moving this particular step to premultiplied-u8 library storage would
introduce additional quantization before the existing readback boundary.

## Standards decisions

- [CSS Compositing and Blending Level 1](https://www.w3.org/TR/compositing-1/)
  defines the general formula, Porter–Duff factors and blend modes.
- Color-dodge checks a black backdrop before testing a white source.
  Color-burn checks a white backdrop before testing a black source. The scalar
  fallback now observes the same ordering as the native backend.
- The nonseparable modes use luminosity and saturation from that specification,
  not an HSV color conversion or a color-management shortcut.
- [HTML Canvas drawing model](https://html.spec.whatwg.org/multipage/canvas.html#drawing-model)
  defines filter, shadow, clipping and compositing order. Acceleration does not
  alter it or remove the JavaScript execution watchdog.

## Verification

`tests/canvas/composite-layers.html` runs 416 shared cases: all 26 operators,
clipped and unclipped drawing, separate shadow generation, transparent and
partially transparent sources and black/white blend endpoints. It compares
large surfaces using native layer compositing to small surfaces using scalar
layer compositing. The same JavaScript fixture runs in Window, OffscreenCanvas
Worker tests and unified-headless Chromium. Native tests additionally cover
malformed payloads, non-byte-aligned clip bitmaps and ownership isolation.

Exact Breeze native/scalar agreement does not imply every sample is identical
to Chromium: its premultiplied storage can differ slightly at RGBA8 readback.
Reference runs must report these differences rather than declaring pixel parity
from an equal test count. `tests/canvas/layer-throughput.html` supplies changing
stroke geometry and repeated whole-surface source/shadow operations for release
measurements; it does not change capability detection or standards scoring.

## Canvas ownership and conversion

Canvas backing dimensions and detached status belong to private platform
storage. Drawing and exports do not consult author-shadowed width, height, or
getAttribute members. HTML canvas dimensions use unsigned-long Web IDL
conversion and HTML non-negative-integer attribute parsing; OffscreenCanvas
dimensions instead use EnforceRange unsigned long long. Both setters reset 2D
pixels and drawing state even when the dimension has not changed.

HTML dimension attribute mutation uses the DOM attribute steps for ordinary
setAttribute, namespace-aware mutation, attached Attr values, and NamedNodeMap
replacement. A namespaced attribute called width is not a bitmap dimension.
A placeholder canvas rejects dimension sets before mutating its attributes.
An HTML bitmaprenderer retains an explicitly transferred output bitmap across
content-attribute changes, until a null transfer restores its blank output.

[HTML OffscreenCanvas transfer steps](https://html.spec.whatwg.org/multipage/canvas.html#the-offscreencanvas-interface)
allow transferring a canvas only before creating its rendering context. This
includes 2D and bitmaprenderer, not only WebGL. Use ImageBitmap to transfer
painted pixels. Rejected transfers retain the source canvas and its context.
Window and Worker tests run the same dimension, enum, conversion-order, and
detachment contract.

Transfer steps run after message-graph getters, even for resources not present
in the graph. They run in transfer-list order, not as a rollback transaction:
a later failing transfer does not restore an earlier detached resource.
Internal Canvas clone hooks are captured privately; author replacements and
inherited JSON `toJSON` hooks cannot change the wire records. Deserialization
defines own data properties, including a property named `__proto__`, rather
than invoking inherited setters.

## Path binding and parser reuse

CanvasPath methods convert unrestricted doubles with Web IDL ToNumber, rejecting
BigInt and Symbol, before inspecting the current transform or changing geometry.
Each argument is converted once in left-to-right order. The rounded-radius union
accepts real iterables and DOMPointInit dictionaries; failed sequence conversion
propagates without IteratorClose, as required by Web IDL. Fill/hit-test overloads validate Path2D's private brand and CanvasFillRule
even when the path is empty or coordinates are nonfinite. Reflected roundRect
contours preserve winding and begin the specified fresh subpath afterward.

Path2D SVG parsing now reuses **svgtypes 0.16.1**, already present in the locked
resvg/usvg graph, under **MIT OR Apache-2.0**. It is a direct dependency, not
copied/vendored source. The adapter was checked against the published package's
manifest, license and path parser; it adds no unsafe code, ambient I/O, build
script or new transitive package. It consumes only bounded SVG strings and emits
owned commands, retaining original f64 coordinates and arc parameters for the
existing kurbo/tiny-skia geometry path. Invalid syntax stops at the last completed
segment according to [SVG 2 path error handling](https://www.w3.org/TR/SVG2/paths.html#PathDataErrorHandling).
Byte/command/flattened-point budget failures remain explicit failures instead of
silently accepting a truncated path.

The fused native solid-path operation reuses the same coverage cache, fill/stroke
rasterizer and source-over compositor as separate calls. It avoids transporting a
mask to JavaScript only to send it immediately back for painting. It does not
replace clipped, gradient, pattern or unsupported-range fallbacks, and validates
the destination/paint before rasterizing. Shared Window/Worker fixtures compare
120 geometry, transform, pen and alpha cases against the clipped fallback.

## Image usability and rectangle bindings

`drawImage` and `createPattern` use private platform source brands rather than
author-controlled prototypes. Image request state comes from the native source
attributes and private completion records, not shadowed `src`, `complete`, or
natural-size getters. An incomplete image is a drawing no-op or a null pattern;
a broken request throws `InvalidStateError`. IDL conversion still runs first,
and nonfinite draw coordinates return before image usability. An incomplete
image cannot erase a backdrop through a temporary `copy` layer.

The decoded source is tied to its current request URL: a replaced attribute
cannot expose an old decoder's bitmap. Connected HTML images now share the
document resource owner's immutable decoded storage with the script realm,
including responsive picture/srcset selection and child documents. Snapshots
unpremultiply the page's BGRA bytes into independently owned straight-alpha RGBA.
No extra Fetch, decoding pass, mutable DOM properties or layout-only pixel
metadata can supply author-readable pixels. Embedded data images are published
before initial scripts; network images are published before their load handlers.

The final browser Fetch response supplies readback policy. Basic/CORS-readable
responses permit snapshots; an opaque response remains forbidden even if script
later sets `crossOrigin`. As with existing video snapshots, opaque inputs throw
SecurityError **before drawing**. This is a fail-closed intermediate boundary,
not full standards-compliant tainted Canvas drawing/readback/transfer support.
The img loader's CORS-mode selection and tainted bitmap propagation remain work
to do; no compatibility flag or score is claimed for those missing behaviors.

Rectangle methods check their private context receiver and required four
coordinates before drawing. Coordinates convert once, left to right, including
after an earlier NaN. Conversion may change the transform; drawing observes the
resulting current transform. Rectangles do not alter the current path.

`tests/canvas/binding-contracts.html` runs shared assertions against either
browser and records reference observations separately. Reference discrepancies
are not rewritten into permissive assertions to make a report pass.

## Private context state

Drawing state lives in a private WeakMap, not page-writable `__fill`, `__path`,
`__transform`, clipping, shadow, filter, pen, or save-stack expandos. Internal
reset and rectangle helpers are captured and removed from public prototypes.
The platform 2D context constructors are illegal for author code. Every context
member checks its private receiver before argument conversion, and IDL members
have configurable, enumerable descriptors.

Path, curve and image bridge records use bounded owned snapshots with null
prototypes before captured JSON serialization. This prevents inherited `toJSON`
from retaining live renderer geometry. It does not mutate the original path's
arrays or strip their prototypes. Window and Worker tests compare poisoned
context expandos to ordinary pixel output, verify reset/resize and receiver
ordering, and poison JSON hooks while drawing curves, strokes and images.

## Drawing-state bindings and opaque contexts

Unrestricted numeric attributes use ToNumber, not the BigInt-accepting `Number`
constructor. String and enum attributes perform DOMString conversion first;
invalid enum assignments preserve the old value, but conversion exceptions
propagate. Text operations convert text and every coordinate before deciding
whether drawing is a no-op. Empty text, nonfinite coordinates and nonpositive
maximum width must not erase the backdrop through a `copy` source layer.

Canvas settings are private context state, independent of save/restore, reset
and resizing. Their dictionary members are read and converted in lexical order
only when creating the context. Existing-context lookups do not examine new
options. `getContextAttributes()` returns a fresh description of actual settings.
The software backend supports sRGB/unorm8, synchronous presentation and optional
readback optimization. Valid display-p3/float16 requests currently decline
context creation with null; no unsupported format is advertised as implemented.

[HTML Canvas alpha rules](https://html.spec.whatwg.org/multipage/canvas.html#the-canvas-settings)
require `alpha:false` output to start and clear to opaque black. Source alpha
still participates in compositing. Opaque output keeps premultiplied result RGB
and fixes output alpha to one; changing alpha after quantized straight-alpha
compositing would produce the wrong colors. Both native and scalar compositors
implement this rule, including separate shadow/source operations and transparent
pixels outside geometry. Intermediate source/filter/shadow layers stay transparent.
Raw `putImageData` ignores source alpha without changing the source array.
Out-of-bitmap readback remains transparent, and OffscreenCanvas bitmap transfer
replaces its output with a new opaque-black bitmap.

Shared Window/Worker tests compare 104 opaque operator/shadow/clip cases across
native and scalar paths, along with initialization, raw pixels, reset, resize,
transfer ownership, dictionary failure ordering and unsupported-format admission.

## ImageData and measured-text ownership

ImageData's borrowed array is a real private slot. Its constructor uses intrinsic
typed-array metadata, unsigned-long dimensions and lexical dictionary conversion,
and rejects shared/resizable buffer sources. Prototype changes and shadowed
`data`, dimensions, buffer or byte-length properties do not change platform
storage. Raw Canvas methods instead use their specified EnforceRange long
coordinates, required arity and conversion ordering. Only sRGB/RGBA8 is currently
admitted; this is not float16 or display-p3 pixel support.

Structured cloning sub-serializes the actual ImageData view, retaining graph
identity and shared backing-buffer relationships with other views. It does not
flatten the pixels into an unrelated array. Buffer transfer consequently keeps
the receiving ImageData and other receiving views on the same transferred buffer.
Old persistent standalone-RGBA8 records remain readable. Clone bindings capture
private platform state and constructors rather than consulting author getters.

TextMetrics is an immutable platform-owned snapshot with readonly, receiver-checked
IDL getters and an illegal public constructor. Ink and font distances are relative
to the selected text baseline; negative ascent/descent values are retained.
Whitespace advance is not counted as ink, and transforms do not alter measurement.
The font backend's existing synthesized hanging/ideographic baseline estimates
remain; OpenType BASE-table baseline extraction is not implemented by this slice.

## Text shaping controls

Letter/word spacing is parsed with the existing CSS tokenizer, length/calc parser
and CSSOM serializer, not a Canvas-specific regular expression. Specified em/rem
and viewport terms are retained and resolved when shaping; percentages and
`normal` are not Canvas <length> values, even when a calc cancels percentages.
Supported units follow the shared page length parser; font-metric units such as
ex/ch and newer CSS math functions remain parser gaps. Fixed absolute CSS units
now use the same reference-pixel ratios both literally and inside calc().

The existing HarfRust shaper applies spacing at cluster boundaries, preserving
combining sequences and required script ligatures. Non-zero letter spacing disables
optional liga/clig/dlig/hlig features. Canvas fontKerning controls the real kern feature,
while direction provides the paragraph's bidi base level and lang selects localized
OpenType shaping. Page text also uses its computed CSS base direction. Each
Canvas/Worker owns these settings, including save/restore and resize resets.

Window/Worker tests check shaped advances, combining marks, CSS conversions,
font changes, real fill/stroke pixels, kerning pairs and mixed-script glyph order.
No measurement or drawing watchdog has been relaxed for these additions.

## Loaded web fonts

Repeated glyph outlines use a realm-owned immutable raster cache (2048 entries,
8 MiB retained data, at most 1 MiB per cached glyph). The key includes the actual
font blob/face, weight/italic synthesis, size, glyph and stroke width. Font-registry
changes clear it. Exported typed arrays are independent copies.

Solid-color text painting consumes an owned glyph packet in one native call,
using the existing straight-alpha compositor. Only the affected destination
region crosses the bridge; sampling and clip indices remain in full-canvas
coordinates. Masks and intrinsic color glyphs preserve scalar byte behavior.
Gradient/pattern paints and rejected packets keep their existing fallback.
Tests compare 416 Window/Worker cases across all 26 operators, transformed and
clipped text, shadows, alpha/opaque surfaces, fill/stroke and maxWidth scaling.

Window Canvas text now owns a document-local font catalog instead of using the
a shared system-only thread-local catalog. The renderer's validated stylesheet
font snapshot and script-created FontFaceSet membership both reach this catalog.
Loading a FontFace without adding it to the set does not change Canvas text.
Adding, deleting, clearing or changing descriptors invalidates selection, shaping
and scaler state; equal font counts no longer suppress replacements. Catalog
identity compares immutable Arc-backed bytes and matching descriptors without
rehashing the font data on every text operation.

Document cancellation drops its font resources and Canvas provider. A later
document on the same thread cannot use an earlier document's private font names.
The existing sixteen-webfont admission limit applies before script installation,
not only after a page checkpoint silently declines a loaded resource.
Dedicated workers expose their own initially empty FontFaceSource and use the
same validated font-byte admission as documents. Each worker owns its Canvas
font catalog; document/other-worker fonts are not inherited. Worker close drops
the registry and provider. Failed loaded-face admission leaves set membership
unchanged, and deletion frees registry capacity.

FontFace BufferSource conversion uses captured buffer/view intrinsics and copies
the actual view range. Shared/resizable sources fail Web IDL conversion; detached
or undecodable sources cannot report successful loading. Descriptor getters run
once in dictionary order. Font state and automatic lifecycle operations are
private rather than author-overridable helpers.

FontFaceSet check/load reuse the CSS shorthand/family parser and existing CSS
weight ordering. Quoted commas, all listed named families, style preference and
empty-text queries have shared Window/Worker tests. Generic families do not
select named downloadable faces. This is not full font-loading conformance:
width/variation/metric overrides,
and layout-blocked ready semantics remain gaps.

Buffer-backed automatic loading and downloaded-font completion now use the
dedicated, non-cancelable font-loading task source in both Window and Worker.
Promise checkpoints cannot complete a pending font task. Every load() returns
the face's existing loaded promise. FontFaceSet ready settles before the later
trusted loadingdone/loadingerror events; those events expose private, read-only,
stable FrozenArray sequences, with Web IDL iterator and dictionary conversion.
Cancellation discards queued tasks. This does not yet integrate document layout
readiness with the set's environment-pending condition.

CSS-connected font discovery now uses the live cascade's owner-ordered inputs,
including imports, adopted sheets and shadow-root sheets. Disabled/disconnected
owners and inactive media/supports groups do not contribute faces. Membership
queries follow edits and removal; retained removed faces become ordinary faces.
Seven unmodified upstream CSS Font Loading files pass (eight assertions).

The existing cssparser UnicodeRange implementation parses and serializes bounded
descriptors. Real document/worker Canvas selection checks both descriptor coverage
and the decoded font's character map. Equal-style subsets can supply distinct
files; excluded characters use fallback fonts, and descriptor changes invalidate
selection, run and glyph caches. CSS downloads carry range metadata into native
font snapshots and request only best-style subsets intersecting visible text,
live controls or generated content. Capitalization conservatively considers
original and uppercase codepoints until loading shares inline-run word boundaries.
Rule-object identity for duplicate equivalent CSS rules, CSS-first set ordering,
descriptor-to-rule mutation and per-face stylesheet load tracking remain gaps;
this does not claim complete CSS Font Loading conformance.

## Font sources and OpenType properties

Font sources are parsed by the existing cssparser dependency. URL escapes,
quoted punctuation, per-component error recovery, `format()` and `tech()` hints
therefore use CSS tokens rather than extension checks or regular expressions.
Unknown hints are skipped, unhinted extensionless URLs are allowed, and unsupported
local aliases are not misreported as malformed syntax. CSS and script-created
faces try supported remote sources in order after network, HTTP or decode failure.
Every alternative independently passes normal Fetch admission and font decoding.
Stylesheet alternatives retain the original face identity, descriptor coverage
and features; changed or removed rules cannot install stale font responses.
Opaque stylesheet-font responses are rejected before byte installation.

Descriptor parsing retains the last valid declaration and ignores `!important`.
It does not split quoted family names or URLs at semicolons. Explicit loading of
an otherwise unused CSS face retains its complete supported source list. Local
font aliases remain unimplemented, and automatic stylesheet load periods are not
yet fully integrated with FontFaceSet's environment-pending state.

CSS `font-feature-settings`, `font-kerning`, `font-variant-ligatures` and
`font-variant-numeric` now reach actual HarfRust shaping. The order is face
descriptor features, high-level properties, spacing/kerning defaults, then
explicit low-level property features. Explicit low-level settings can re-enable
optional ligatures disabled by letter spacing. Duplicate tags use the last
value, and computed feature maps serialize in ASCII tag order. Authored values
retain tag order and duplicates; serialization omits the default value of one.
Resolved kerning policy overrides face defaults in both directions. Shape/run caches
include these settings, and changing a loaded face's feature descriptor clears
its actual font selection and shaped/rasterized results.

CSSOM font shorthand assignments expand supported longhands atomically and keep
authored relative units. Mixed priorities or noninitial reset-only components
cannot serialize as a lossy shorthand. Equal writes do not generate attribute
mutations. The font shorthand parser rejects duplicate style/weight components;
unimplemented caps/width synthesis and system-font shorthands remain declined.
Font variants currently cover ligature and numeric groups, not caps, position,
East Asian variants, named alternates or variation axes. This is not a claim of
complete CSS Fonts conformance. Feature values currently accept literal CSS
integers, not container-dependent `calc()`/`sign()` expressions. The upstream
computed-feature diagnostic includes those expressions and is not claimed as a
fully passing conformance file.

Deterministic tests derive a font in memory from the existing CC0 Ahem fixture,
with checksummed GSUB ligature/numeric tables and a real kerning pair. Tests
observe changed glyph identities, UTF-16 clusters, advances and rasterized pixels,
not just property strings. The fixture is neither shipped nor installed. Window,
Worker, native shaping, CSSOM and renderer-process tests cover the same ownership
and loading contracts. Renderer protocol major version 17 includes base direction,
kerning, variant groups and bounded feature tags in text snapshots.

CSS direction now reaches the text shaper and logical start/end alignment. This
does not yet provide complete `unicode-bidi`, paragraph-wide bidi across separate
inline runs, or every RTL flex/grid/table/form layout behavior.

The unchanged W3C Ahem test font (public-domain/CC0, documented source and SHA-256
in `tests/canvas/fonts/README.md`) verifies exact real advances, rasterized pixels,
unregistered faces, deletion, replacement and cross-document isolation. It is not
installed as a system font or embedded in the release browser.

## Reference-browser boundary

The shared fixtures are standards contracts, not a declaration that every
assertion passes in Chrome 154. Its hidden reference runs differ for some
TextMetrics prototype fields, quoted FontFace family matching, and opaque bitmap
initial/readback cases. P3/float16 admission and provider hints intentionally
differ because Breeze declines unsupported storage. Report these observations
separately from native/scalar pixel equality and upstream WPT results; do not
relax the contract or claim pixel parity solely from a fixture's pass count.

The final October 6 replay passes all 572 curated WPT files (6,174 assertions)
and 74 Khronos files (8,696 assertions). The separate unmodified computed-font
feature diagnostic passes eight cases and fails two container-dependent math
cases; it is not included as a passing curated file. HTML5test remains 507/588
before and after, versus 579/588 in the fresh Chrome 154 reference.

The small FontFace binding probe reports `passed` in both browsers, although
the Chromium capture harness classifies its six-character body as structurally
empty. Other reference fixtures record genuine differences: exact Ahem pixel
counts, image interpolation rounding, quoted-family matching and the platform
contracts listed above. Preserve these observations; successful Breeze fixture
results alone do not establish Chrome parity. The README records fresh rotating
three-run, readback-fenced Canvas medians and the remaining layer-performance gap.
