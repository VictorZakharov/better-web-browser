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

Every lookup returns independent bytes. Author mutation, another Canvas, bitmap
resize, and worker use cannot modify retained coverage. The LRU bounds are
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

Unclipped solid source-over regions of at least 256 pixels now batch the existing
scalar compositing equation in Rust. The private host boundary accepts owned
coverage and destination bytes, finite color channels and opacity, and validates
their exact sizes before producing replacement bytes. Other styles, operators
and clipped draws keep the scalar path. Whole-surface effects still build their
normal source layer first; this fast path can paint that layer without changing
the order of shadow, filter or final compositing operations.

Window tests compare complete batched bitmaps against the scalar clipped path
over opaque/translucent colors, opacity, curved edges, dash coverage and effects.
Native tests use an independent integer alpha oracle, and OffscreenCanvas tests
verify owned destination regions and live paint. The same targeted single run
with batched painting took 45.5 ms; its interior/outside samples were unchanged.
The extra owned-copy/host-call overhead is deliberately avoided for tiny regions.
