# Canvas geometry transport and private-record ownership

This changes an implementation-only transport, not Canvas geometry semantics or
feature detection. The existing tiny-skia dependency still performs compound
fill coverage and stroke-outline rasterization. Paint sampling, clipping,
source-over compositing, admission budgets and the software fallback remain
separate. There is no new dependency, imported implementation or public API.

## Packed point payload

Normal fill/stroke painting sends a small, strictly validated JSON metadata record
with an empty `parts` list, plus an independently copied byte payload. Legacy JSON
geometry remains available for existing native callers and fallback. A present
invalid byte argument never silently selects the legacy provider or an empty path.
Supplying both nonempty JSON geometry and packed geometry is rejected.

The versioned private `CPG1` format uses little-endian fields:

| Field | Encoding |
| --- | --- |
| Magic | Four bytes `CPG1` |
| Subpath count | `u32` |
| Each subpath's point count | `u32` |
| Each subpath's closed flag | `u32`, exactly 0 or 1 |
| Each point | Two `f64` coordinates |

Coordinates retain JavaScript precision until the existing native `f32`
rasterization boundary. The decoder applies the original coordinate limits
after that conversion, including finite values rounding onto the boundary.
It rejects nonfinite coordinates, unknown versions/flags, truncation, trailing
bytes, excess subpaths and aggregate points. Counts and remaining byte lengths
are checked before reserving point storage. The existing 8,192-point budget is
shared across all subpaths; it is not 8,192 points per subpath. No borrowed author
or shared-memory span is retained by Rust or the cache.

Coverage caching compares operation, metadata and geometry bytes independently
and exactly. Packed and legacy requests cannot collide; no hash-only equality
is used. The existing bounded cache counts owned metadata, geometry and mask
storage in its byte budget. A draw still evaluates current paint and clipping.

## Mutable author hooks

The private encoder captures its numeric DataView writers and the intrinsic
typed-array buffer getter at bootstrap. It does not use author iterators,
`toJSON`, replaceable DataView methods or replaceable buffer accessors.
Snapshots belong to the draw request, not the live retained Path2D.

The metadata serializer previously used a mutable array iterator for its copied
key list and an ordinary property-descriptor literal. Author prototype hooks
could therefore be invoked during private snapshot creation. It now iterates
dense owned keys by index, reads only own data descriptors, and creates data
properties on null-prototype snapshots. Unexpected accessor records are rejected
without evaluating their getters. These checks secure this boundary; they are
not a claim that every Canvas helper has been audited against all prototype hooks.

## Verification

- Complete packed-vs-JSON coverage comparisons exercise compound winding,
  fractional edges, translated regions, pen transforms, cap/join styles,
  dash patterns and offsets, and both antialiasing settings.
- Borrowed and owned fused painters preserve exact clipped destination pixels;
  rejected packets cannot return partial pixels or mutate a borrowed destination.
- Every truncated prefix of a valid packet is rejected. Boundary counts,
  nonfinite values and cache ownership/accounting have dedicated native tests.
- Window and Worker tests execute the actual encoder and serializer with mutable
  author hooks, signed zero, double precision and independent request snapshots.
  The three serializer regressions failed against the previous implementation.

`tests/canvas/geometry-throughput.html` is a controlled 64-square, moderately
dense Path2D workload using normal APIs and a timed readback fence. Its results
must preserve every Breeze before/after bitmap checksum. Reference-browser
antialiasing can differ; a stable Chrome checksum is not cross-engine pixel parity.
This fixture alone cannot establish game startup or whole-browser performance.

Three fresh profiles per browser, alternating Breeze before/after order, produce
the following nine warmed batches. The first of four batches in each profile is
excluded as warmup; Chrome is headless at the same 1262-by-539 CSS-pixel viewport.
CPU sampling is disabled, and no build runs concurrently with the measurements.
The before binary includes the preceding region/clipped-shader improvements;
the after binary adds private-record hardening and packed geometry together.

| Dense Path2D readback workload | Before | After | Chrome 154 |
| --- | ---: | ---: | ---: |
| Median batch time | 51.5 ms | 7.2 ms | 5.5 ms |
| Warmed range | 50.5–58.2 ms | 5.6–8.4 ms | 5.2–6.3 ms |
| Stable whole-bitmap checksum | 538068976 | 538068976 | 4227674552 |

This is about an 86% reduction for the combined transport changes on this
workload, not an isolated attribution to one patch. Chrome's separate stable
pixels differ. Cold first batches also differ substantially and are not hidden
inside the warmed median or used to claim whole-browser startup parity.
