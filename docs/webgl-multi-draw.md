# Native WebGL multi-draw

`WEBGL_multi_draw` is available in Window and Worker for WebGL 1 and 2 when
the existing locked ANGLE backend advertises `GL_ANGLE_multi_draw`, all four
fixed native entries resolve, and its instancing dependency is usable. Author
admission remains separate from native availability. WebGL 1 implicitly enables
`ANGLE_instanced_arrays`; WebGL 2 retains its core instanced methods.

The implementation follows the
[Khronos extension](https://registry.khronos.org/webgl/extensions/WEBGL_multi_draw/)
and the [ANGLE native contract](https://chromium.googlesource.com/angle/angle/+/main/extensions/ANGLE_multi_draw.txt).
It does not add another renderer, dependency, copied implementation or public
native pointer. The native ABI adapters use the pinned `gl2ext_angle.h` signatures
and only resolve a closed list of symbols from the linked provider.

## Conversion and validation

The four methods are exposed only on the context-owned extension object, with
Web IDL arity, descriptors and receiver checks. Scalar arguments and numeric
lists convert in order, even during context loss. Genuine Int32Array union arms
use captured intrinsic ranges and owned snapshots at argument conversion;
sequence arms consume their author iterators with signed-long conversion.
Shared typed buffers are accepted, but native code receives owned integers, not
concurrently mutable author memory. Detached/out-of-bounds views fail before IPC.
Offsets use unsigned-long-long conversion and subsequent list-range validation.
Only selected slices are serialized; large typed lists can use nonzero offsets.

Snapshots are bounded at 1,048,576 Int32 elements per typed argument (4 MiB),
iterated sequences at 8,192 elements, and native batches at 4,096 draws. Aggregate
vertex-instance work retains the existing one-million-vertex draw budget.
Excess native batch/work requests produce GL errors; oversized conversion
snapshots/iterators throw the existing bounded-list RangeError. These resource
limits are explicit implementation limits, not claims of unlimited conformance.

Before a driver call, the complete batch validates primitive mode, signed counts,
index type/alignment/ranges, current program, framebuffer, texture sampling, and
all active vertex/divisor buffer ranges. Vertex and instance maxima are independent,
so one shared attribute-reflection pass can validate the entire batch. A late
invalid draw must not partially render earlier draws. Element offsets become
native offset-pointer lists only after validation against the bound buffer.

## Shader and capture ownership

Real native multi-draw entry points preserve `gl_DrawID`, including slots with
zero vertices or instances. The explicit validator enables ANGLE's draw-ID
compiler option only after author extension admission. Its generated private
draw-ID uniform is not fed back as author shader source: the native WebGL
compiler receives validated original source and owns actual per-draw emulation.
Compiler-cache keys include this admission flag. Failed shaders are not cached.

The pinned provider leaves its private draw-ID uniform at the last multi-draw
ID. Ordinary draws therefore use a one-entry native batch while the extension
is enabled, ensuring the specified `gl_DrawID == 0`. This neither rewrites shader
text nor exposes/changes author uniform locations. Without extension admission,
ordinary draws keep their original native entry points.

Transform feedback has browser-owned capture accounting because the provider's
batch validation checks subdraws against the same initial cursor. Native linked
varying reflection determines actual scalar/vector/matrix/array byte strides;
interleaved strides sum and separate strides use the minimum binding capacity.
Complete primitives are counted independently for each draw before multiplying
instances. Previous successful ordinary/batched draws advance the cursor;
errors and paused draws do not, and a new capture resets it. The complete batch
must fit before any native capture can write. ANGLE still owns actual GPU output.
WebGL2's exact-mode and non-indexed active-capture restrictions remain in force.

After context loss, retained extension methods are inert after conversion.
Restoration requires a new extension object and fresh shader admission. Default
multisample resolve/readback/presentation caches invalidate for all four drawing
operations. No watchdog, context count or native resource budget is increased.

## Local evidence

On October 5, 2026, hidden Breeze and Chrome captures of
`tests/webgl/multi-draw.html` matched all recorded observations: red/green/blue
draw-ID pixels, empty-slot preservation, ordinary ID reset, late invalid-range
rejection without pixels, typed-list snapshot timing, combined capture overflow,
actual captured values `[1,2]`, and final `NO_ERROR`. Both captures returned HTTP
200 with no harness error. This is targeted evidence, not Khronos certification,
a game-throughput measurement, or an HTML5test score claim.

Native, public Window and worker unit tests additionally cover all four variants,
WebGL 1 implicit instancing, detached views, ordered conversion, large list
offsets, stale extension objects, primitive/instance capture accounting, range
offsets, pause/resume, previous writes and separate matrix/vector capture.
Full standards suites remain local checks; CI remains a smoke gate.
