# Canvas coverage and bridge ownership

Canvas native painting owns copies of private raster regions. It does not
borrow a mutable author ArrayBuffer, cache painted output, or bypass the
context's drawing state. These boundaries apply in document and worker realms.

## Bridge-owned destination reuse

V8 argument conversion already copies each byte view into an independent Rust
allocation. The closed stateless-paint dispatcher may consume that destination
allocation instead of cloning it again. Input conversion and the final bitmap
commit still copy bytes; this is not a zero-copy author-buffer interface.

The ordinary borrowed dispatcher remains available. Both entry points share
validation, clipping, rasterization and composition. Document profiling and
document/worker lifetime checks remain at the existing host boundary. Resource
registry operations, DOM mutations and policy-dependent operations cannot enter
this dispatch path.

Malformed input returns the normal native-decline result. A consumed private
allocation need not survive a decline, but no partial pixels are returned or
committed to the context bitmap. Separate copies of source and clipping inputs
are not consumed, including when author views originally alias one another.

Allocation-identity tests compare owned and borrowed results byte for byte,
verify that the returned allocation is the supplied destination, and verify
that every other argument remains unchanged. Truncated packets and dead hosts
exercise the same rejection contract.

## Lossless coverage storage

Coverage is geometry, not source alpha or a painted result. Its logical length
and every retained sample remain unchanged. A private representation can omit
exactly zero runs; consumers use each retained run's original bitmap index.
Dense masks remain contiguous when span metadata would erase the saving.

The compact representation is admitted only for masks of at least 1,024 bytes
and when samples plus span records save at least one quarter of the dense
storage. Temporary growth is bounded before extending either vector. Public
byte results expand omitted positions to zero in independent owned storage.

The exact-key coverage cache retains its eight-MiB byte budget, 256-KiB key
limit and one-MiB logical-mask admission limit. Its metadata entry cap is 512,
allowing many compact thin strokes to fit without increasing the byte budget.
Both key fields and actual span/sample storage count toward that budget. A
consumer can retain immutable coverage after eviction; that cannot mutate a
later cached or author-visible mask.

No coordinate normalization, hash-only equality, reduced sampling resolution,
or floating-point coverage quantization is introduced. Request text, packed
geometry bytes and operation kind must match exactly. Pen, transform, region
and fill-rule differences therefore retain separate cache identities.

## Shadow sampling and rectangles

Shadow sampling hoists bilinear axis indices and weights out of the per-pixel
loop. The image-rs blur backend, support bounds, subtraction order, four-tap
evaluation order and byte rounding remain unchanged. Independent scalar tests
cover fractional offsets, tails, single-pixel surfaces and empty intersections.

The rectangle native path ports the existing double-coordinate area integrator
rather than substituting tiny-skia's float-coordinate path coverage. Solid
source-over fills and clears retain transforms, subpixel interval overlap,
reflection, clipping and opaque context storage. Affine fills intersect a
convex quadrilateral with each pixel using bounded stack storage. Unexpected
overflow declines instead of dropping vertices or returning partial output.

Native admission does not change WebIDL argument conversion or the current
path. Clear operations ignore paint effects. Gradients, patterns, unsupported
compositing and oversized affine work retain their established fallback.
See the [HTML rectangle drawing contract](https://html.spec.whatwg.org/multipage/canvas.html#drawing-rectangles-to-the-bitmap).

## Controlled measurements and limits

The initial compact-coverage/shadow-sampling release was compared with merged
#229 and Chrome 154 in three alternating-order fresh-profile runs. Fixtures
explicitly request CPU/readback contexts. All Breeze before/after complete
readback checksums matched; timings do not assert byte-identical Chrome edges.

| Warm workload median | Merged #229 | Compact coverage | Chrome 154 |
| --- | ---: | ---: | ---: |
| Sixteen curved path paints | 8.4 ms | 7.1 ms | 1.1 ms |
| Twelve shadow path paints | 120.5 ms | 93.9 ms | 3.7 ms |
| Three hundred tall strokes | 203.4 ms | 64.4 ms | 1.2 ms |

These are intermediate measurements before the native rectangle extension,
not final-head results or a general browser-speed ranking. Cold shader/provider
initialization is not comparable to steady-state painting. Source sampling and
compilation were disabled during the paired timing runs.

The unchanged game still fails ordinary startup acceptance at this stage.
Its fresh 30-second unprofiled replay retains the loading overlay and a
document promise-job watchdog error. The harness page-ready milestone is not
time-to-lobby. A separate sampled replay is diagnostic only: absence of its
timeout error does not establish completion.

Later slices in this batch address the general script deadline and cancellation
policy, then independent WebGL metadata and object admission. These are not
included in the intermediate timing table above. Ordinary game acceptance must
be rerun after each release; passing Canvas fixtures is not evidence of reaching
the lobby. No texture resolution, game asset or shipped V8 backend was changed.

## Final-source release replay

Three fresh alternating runs compare merged #229 with the complete October 8
source release and Chrome 154. Every interval includes pixel readback, sampling
is disabled, and no builds/tests run concurrently. All corresponding Breeze
before/after whole-bitmap checksums match in every capture.

| Completed workload median | Merged #229 | October 8 release | Chrome 154 |
| --- | ---: | ---: | ---: |
| Tall-stroke weave, cold geometry | 382.2 ms | 250.2 ms | 1.7 ms |
| Same weave, warm geometry | 304.6 ms | 73.6 ms | 1.7 ms |
| Weave with changed pen | 382.9 ms | 254.6 ms | 2.7 ms |
| Fractional rectangle fills | 12.9 ms | 3.6 ms | 0.2 ms |
| Affine rectangle fills | 28.5 ms | 17.9 ms | 0.3 ms |
| Affine rectangle clears | 6.2 ms | 2.0 ms | 0.3 ms |
| Linear-gradient path | 7.3 ms | 5.0 ms | 1.7 ms |
| Radial-gradient path | 6.6 ms | 4.8 ms | 1.4 ms |
| Conic-gradient path | 5.3 ms | 4.7 ms | 1.5 ms |
| Repeating-pattern path | 5.2 ms | 5.0 ms | 5.4 ms |

The warm weave benefits much more than cold or changed geometry; reporting only
that row would hide a remaining startup cost. Its before observations range
233.2–360.1 ms and after observations 73.1–84.7 ms. The path/pattern observations
overlap substantially, so their small median changes are not asserted as
significant gains. Chrome's checksums differ except for affine clearing; no
pixel-perfect or whole-browser parity claim follows from these timings.
