# Bounded derived index ranges

Repeated indexed draws often reuse unchanged element-buffer bytes. Rescanning
the same range before every ordinary, instanced or multi-draw invocation adds
linear CPU work even though the resulting maximum is unchanged. Breeze caches
only that derived maximum, not a prior successful draw or native geometry.

## Validation and ownership

One context retains at most 64 fixed-size records, keyed by browser buffer
owner, byte offset, index count and index width. No index data, author arrays,
native pointers or executables are retained in these records. Count pressure
evicts the least recently used record before insertion. Browser names are
monotonic, so a deleted buffer's bounded stale record cannot alias a later name.
Destroying the context destroys all its records.

The byte-range arithmetic and current buffer extent are checked **before** every
cache lookup. A cached maximum still goes through the actual context's admitted
maximum-element limit. Current program inputs, enabled attributes, buffer/VAO
state, stride, offset, divisors, framebuffer, transform feedback and native draw
validation remain live. No cache hit overrides any GL error or storage limit.

The real index decoder remains shared by ordinary, instanced and multi-draw
preflight. WebGL2's fixed restart markers are excluded from its maximum; an
all-restart range is distinct from both a missing cache record and index zero.
WebGL1 retains its different marker semantics. These follow the
[WebGL2 primitive restart contract](https://registry.khronos.org/webgl/specs/latest/2.0/#4.1.4)
and the [WebGL indexed-draw bounds rules](https://registry.khronos.org/webgl/specs/latest/1.0/).

## Content invalidation

Successful `bufferData`, `bufferSubData` and native `copyBufferSubData` discard
every derived range for the destination buffer, including disjoint ranges.
Pixel-pack writes and transform-feedback begin/resume also invalidate their
destinations conservatively. Failed validation does not mutate storage or
discard a still-valid range.

The cache cannot use an invalid CPU mirror. That path discards derived ranges
and obtains a full real native readback before decoding. Buffer-class admission
normally prevents GPU capture/pixel writes into an element-class buffer, but
the cache's correctness does not depend on assuming that restriction will
never change. Native readback failure remains a real error, not a guessed maximum.

## Tests and measurements

Pure tests cover every key dimension, the all-restart representation, fixed
capacity, eviction order, replacement and owner-scoped invalidation. Native tests
render real indexed triangles and exercise partial writes, storage replacement,
numeric zeroing, GPU copies, changed vertex buffers/VAOs, failed writes and all
three index widths in WebGL1 and WebGL2. A direct test-only native write checks
that an invalid mirror refreshes from GPU truth rather than hitting old metadata.

The owned `index-range-throughput` fixture submits 100 real 120,000-index draws
under rasterizer discard, fences completion, then renders and reads a triangle.
It changes the same index range, records the permitted vertex-bounds policy,
then shrinks storage and requires the mandatory out-of-bounds index-fetch error.
Breeze's native tests continue requiring its stricter vertex-bounds rejection.
Submission time, native completion and successful pixels are separate evidence;
this fixture does not establish gameplay acceptance or an HTML5test score gain.

The final October 8 source release was compared with merged #229 and Chrome 154
in three fresh, alternating runs without concurrent builds/tests or CPU sampling.
All browsers used the same viewport at scale 1. The interval includes native
finish and the final triangle readback, not just asynchronous draw submission.

| Median, 100 draws of 120,000 indices | Merged #229 | October 8 release | Chrome 154 |
| --- | ---: | ---: | ---: |
| Submission | 5.6 ms | 2.0 ms | Below timer resolution |
| Completed native work | 7.0 ms | 3.4 ms | 3.3 ms |
| Pixels and mandatory index-bounds rejection pass | 3/3 | 3/3 | 3/3 |

Individual completion observations are 7.0/8.6/6.9 ms before, 4.5/3.4/2.7 ms
after, and 4.5/3.3/3.0 ms in Chrome. The after/reference ranges overlap; no
significant advantage over Chrome follows. This compares the combined batch,
not an isolated proof that the range cache explains every improvement.
