# Bounded Canvas coverage reuse

Fill and stroke coverage is pure geometry. The renderer reuses successful native
coverage masks for exactly identical requests; it does not cache Canvas pixels,
paint objects, clipping, global alpha, blend operators or destination contents.
This preserves the [HTML Canvas drawing model](https://html.spec.whatwg.org/multipage/canvas.html#drawing-model)
while avoiding repeated outlining and supersampled rasterization.

Each renderer thread retains at most 32 entries and 8 MiB of request-plus-mask
payload. Requests larger than 256 KiB and masks larger than 1 MiB bypass storage
without changing rendering. Exact operation and complete request bytes are the
key, including transformed points, pen, dash phase, fill rule, antialias setting
and output region. Successful empty coverage is reusable; invalid/unsupported
requests still take normal validation/fallback on every invocation.

Every JavaScript lookup returns independent bytes. Fused native painters share
immutable coverage storage internally; they do not clone the mask merely to
consume it once. Author mutation, another Canvas, bitmap resize, and worker use
cannot modify retained coverage. The LRU bounds are
independent of the existing raster and Canvas allocation limits. No watchdog or
resource limit was raised, and no dependency was added.

## Targeted measurement

On the development Windows machine, the hidden `tests/canvas/mask-reuse.html`
fixture performs 200 fills and dashed strokes of one curved path, changing solid
paint each iteration and clearing the destination. One fresh-profile run before
reuse took 135.9 ms; after reuse it took 51.4 ms. The Chrome reference took 87.2 ms.
All three produced an opaque blue interior and transparent outside sample.
These are single targeted samples, not browser-wide or game-loading claims.
Full shape-edge rasterization still differs from Chrome at some fractional
pixels, and changing geometry naturally misses this cache.

## Solid source-over painting

Solid source-over regions of at least 256 pixels now batch the existing
scalar compositing equation in Rust. The private host boundary accepts owned
coverage and destination bytes, finite color channels and opacity, and validates
their exact sizes before producing replacement bytes. Other styles, operators
keep the scalar path. Packed binary clips are evaluated against whole-bitmap
coordinates before each painted pixel. Whole-surface effects still build their
normal source layer first; this fast path can paint that layer without changing
the order of shadow, filter or final compositing operations.

Window tests compare complete batched bitmaps against the scalar gradient path
over opaque/translucent colors, opacity, curved edges, dash coverage and effects.
Native tests use an independent integer alpha oracle, and OffscreenCanvas tests
verify owned destination regions and live paint. The same targeted single run
with batched painting took 45.5 ms; its interior/outside samples were unchanged.
The extra owned-copy/host-call overhead is deliberately avoided for tiny regions.

## Exact supersample reduction

Changing geometry still needs fresh rasterization. Its 2-by-2 and 4-by-4 coverage
reduction now accumulates source rows rather than repeatedly indexing individual
samples. Four-byte integer lane sums avoid carries between pairs; the final
rounding remains exactly `(sum + sample_count / 2) / sample_count`. Independent
tests compare every output byte against the original nested-sample equation,
including odd row widths and constant coverage extremes. Sample counts, quality,
allocation limits and raster budgets are unchanged.

Supersampled rasterization keeps one temporary tiny-skia mask per renderer
thread, bounded to 8 MiB. An exact width/height match can reuse its allocation,
but every checkout clears all samples; it is not a geometry-result cache.
The full raster coordinate space is retained because translating a cropped
surface can change edge rounding. Native-resolution outputs remain owned bytes
and are not retained just to copy them. Tests compare reused surfaces against
independent fresh-surface rasterization and cover incompatible strides, nested
ownership, and the retention limit.

One fresh hidden run of `tests/canvas/coverage-reduction.html`, which changes the
pen width for all 40 strokes to avoid reuse hits, took 153.4 ms before and 116.1 ms
after. The sampled alpha values remained `[255, 48, 0, 0]`. This is a targeted
single-run measurement, not a claim that game startup is fixed.

## Compact native shadows

Solid fill/stroke source layers can remain in native coverage storage while the
host generates their shadow and final composition. The source rectangle keeps
its whole-bitmap origin; only storage is compact. The final clip is applied after
source coverage and shadow generation, not before it.

The Gaussian filter still comes from the existing image-rs dependency, with
sigma `shadowBlur / 2`, the existing sigma cap, transparent padding, and the
same fractional-offset sampling and byte conversion. Exact immutable kernels
can be reused when the cropped alpha, dimensions, padding and sigma match.
Color, offsets, clips, operators and destination pixels remain live inputs.

Sparse layers preserve omitted pixels as transparent source. Destructive
Porter-Duff operators still process those pixels; transparent hidden RGB and
opaque-canvas rules are preserved as well. Native regression tests compare the
compact pipeline against the frozen dense algorithm across every supported
blend mode, clipping, offsets, alpha extremes and opaque destinations.

The geometry JSON decoder uses serde_json's exact float-roundtrip mode. This
protects binary-double texel boundaries in transformed glyphs and path requests;
an epsilon or coordinate-snapping workaround would alter valid geometry.
