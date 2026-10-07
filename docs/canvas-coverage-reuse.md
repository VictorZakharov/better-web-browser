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

## October 6 final release comparison

Baseline is main at `e349b7e` (#227). Fresh hidden release profiles were rotated
between before, after and Chrome 154.0.8037.98 on the same machine. Every timed
operation includes pixel readback; the fixtures use fixed Canvas resolutions.
These are targeted measurements, not matched-viewport page benchmarks or a
general claim that Breeze is faster than Chrome.

| Workload | Before | After | Chrome |
| --- | ---: | ---: | ---: |
| Tall strokes, cold coverage | 390.0 ms | 307.3 ms | 51.3 ms |
| Tall strokes, warm coverage | 358.7 ms | 308.5 ms | 81.5 ms |
| Tall strokes, changed pen | 381.4 ms | 311.8 ms | 2.4 ms |
| Compound clips | 122.9 ms | 27.1 ms | 122.6 ms |
| Clipped strokes | 52.5 ms | 36.5 ms | 130.5 ms |
| Clipped fills | 16.5 ms | 6.4 ms | 0.6 ms |
| Cold shadow | 32.5 ms | 19.8 ms | 162.9 ms |
| Changing shadow paint | 333.7 ms | 142.8 ms | 109.5 ms |
| Changed shadow kernel | 29.1 ms | 16.7 ms | 4.1 ms |

Clip/shadow rows are medians of three completed runs. Tall-stroke rows use four
attempted runs per browser: the baseline's first attempt exceeded the unchanged
two-second script watchdog and produced no timings. Its completed-run median
uses the other three attempts; after/Chrome medians use all four. That censored
failure is retained, not replaced or silently excluded from success counts.
Before/after recorded hashes and pixel samples match across completed runs.
Independent native dense/scalar oracles cover broader pixel contracts; sampled
hashes alone do not establish every pixel or Chrome parity.

Large bitmap premultiplication also shares the image decoder's exact integer
conversion, with zero/opaque alpha fast paths. An exhaustive channel/alpha test
checks the original rounding equation. The unchanged large root/child Canvas
renderer fixture now passes under its existing two-second heartbeat; its size,
wire budget and watchdog were not relaxed.
