# WebGL uniform-location ownership

Uniform locations are not driver objects. They are opaque browser records for
one program, one link generation, one native location, and one reflected type.
Location zero is valid; the null JavaScript location is a separate no-op value.

The old registry allocated a full GPU-object record per uniform location and
counted it against the 1,024 GPU-object allowance. Four small programs with 300
active array-element locations could therefore prevent a new shader from being
created, even though they used only twelve GPU objects.

Locations now have two compact, fixed-key maps, capped at 16,384 records per
context. Aliases reuse a record through constant-time lookup; browser IDs
remain monotonically allocated across contexts and are never driver pointers.

GPU objects have a separate 8,192-name admission limit, increased from 1,024
after an asset-heavy application's loading phase independently exhausted that
pool. This bounds browser records and driver object names; it does not reserve
texture or buffer storage for each name. The existing resource-byte limits
remain unchanged. Uniform queries do not consume this allowance.

A controlled Chrome 154 run of the sibling game reached the lobby after
creating 5,487 buffers and 2,277 vertex arrays, along with 640 other native
objects. Deletions also occur during startup, so cumulative creations are not
the peak live count. Nevertheless, both the old 1,024 and intermediate 4,096
limits failed the ordinary Breeze replay. The new bound admits this measured
workload without removing the independent storage caps. A successful Chrome
reference does not itself establish Breeze game acceptance.

Every link attempt advances the program generation and removes its old location
records, including unsuccessful links. Deleting a current program does not
reclaim its locations prematurely: its existing executable and locations remain
usable until the final current reference is released. Actual program retirement
and context teardown reclaim the location metadata. Expired IDs cannot alias
newly created locations.

Native regression tests query 1,200 real GLES locations, then create another
shader; they also exercise relinking and deletion while current. Pure registry
tests cover the metadata cap, duplicate aliases at capacity, zero locations,
owner-scoped retirement, and independent GPU-object admission.

This addresses a general allocation/lifecycle defect, not a site exception. A
successful allocation test alone does not establish that a particular game can
finish loading; that requires a separate unchanged-game run.

Reference: [WebGL shader and program operations](https://registry.khronos.org/webgl/specs/latest/1.0/#5.14.9)
and [uniform operations](https://registry.khronos.org/webgl/specs/latest/1.0/#5.14.10).

## Linked reflection reuse

Native `getUniformLocation` and link-status validation still execute for each
query. Resolving a new browser location previously rescanned active uniforms
until it found the reflected type, repeating the same scan for each location.
The native owner now retains an ordered table of actual reflected array-family
names and types for the program's browser ID and link generation. A table is
built only from native active-uniform queries, not parsed author declarations,
guessed numeric locations or cached values.

Scalar array aliases share their reflected family. Array-of-struct fields keep
their `[]` and field separators, so different fields never collapse together.
Uniform values and driver locations are not stored in this optimization; their
existing native/type/generation checks remain unchanged. Compiling an attached
shader alone does not change the linked program's reflection. Every relink
removes the preceding table before submitting the new invocation, even if that
invocation later fails. A browser ID cannot alias a recycled native name.

The per-context LRU retains at most 32 program tables and one MiB of charged
vector/string capacities. Staging has its own one-MiB admission limit; stable
sorting can require additional bounded temporary storage. These are metadata
allocation bounds, not a claim of total allocator/driver memory usage. Cache
pressure declines caching and continues checking native reflection rather than
rejecting an otherwise legal uniform. Context teardown drops all tables; a
retired program's metadata may remain until bounded eviction but retains no
native object or executable.

Pure tests cover owner/generation separation, count/byte eviction, allocation
capacity, native-order duplicate families and retirement. Native tests verify
that 64 distinct locations need one scan, still render their uploaded values,
and retain exact scalar/vector/matrix types across successful and failed relinks.
The owned `tests/webgl/uniform-reflection-throughput.html` workload times 768
new locations after actual compilation/linking, then checks every native value
and every program's pixel output. It must be measured separately from shader
submission and unchanged-game startup.

## Measured reflection lookup workload

The owned `uniform-reflection-throughput` fixture links eight actual WebGL2
programs with 96 active scalar uniforms each before timing. All 768 locations
are then used and queried; every program must render its expected native pixels.
Three alternating fresh hidden runs per version use the same local bytes,
1262-by-539 CSS viewport, 100% device scale and ten-second settling interval.
No build runs during these timed captures.

| Median measurement | Before reflection cache | After reflection cache | Chrome 154 |
| --- | ---: | ---: | ---: |
| Lookup 768 native uniform locations | 32.4 ms | 24.7 ms | 0.7 ms |
| Lookup, upload, synchronous queries and pixel assertions | 74.8 ms | 66.1 ms | 187.7 ms |
| All values and native pixel assertions pass | 3/3 | 3/3 | 3/3 |

Chrome remains much faster on location lookup. The second interval includes
many synchronous native queries/readbacks and is specific to this fixture; it
is not evidence that Breeze renders ordinary applications faster than Chrome.
The comparison isolates the uniform cache from later validator, attribute and
owned-transfer changes. Neither interval measures game startup or HTML5test
score, and no point gain follows from this internal metadata optimization.
