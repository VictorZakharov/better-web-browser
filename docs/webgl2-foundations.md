# WebGL2 native baseline

Windows Canvas and OffscreenCanvas now admit `getContext('webgl2')` through
the real ANGLE backend. This is a bounded baseline, not full conformance
certification or evidence that the sibling game runs. A canvas locks to its
first successfully created context type; failed creation consumes no mode.
`experimental-webgl2` remains unsupported. Ordinary creation dictionaries do
not forward arbitrary author options into the native creation protocol.

The closed internal `api` value selects a GLES2 or GLES3.1 provider through
the existing pinned ANGLE dependency. Both retain WebGL compatibility, robust
resource initialization, disabled client arrays, thread affinity, ownership
budgets and hidden execution. A newer native driver must not silently change
WebGL1's shader rules or extension admission.

## Completed native contracts

- Explicit GLES3.1 construction, private initialized drawing buffer and versioned
  state strings. Failed/unknown creation versions consume no public context.
- WebGL2 front-end shader validation with actual GLES3 limits, followed by
  ANGLE's native WebGL2 compiler. ESSL300 `gl_VertexID`, integer operations,
  explicit fragment outputs and `isnan`/`isinf` produce real pixels. ESSL310 is
  rejected, and the WebGL1 provider still rejects ESSL300.
- Original names are passed to the native WebGL2 compiler after successful
  validation. WebGL2's 1024-byte token boundary reaches ANGLE's hash-name
  no-prefix threshold, so WebGL1's textual prefix decoder is not applicable.
  Tests resolve uniforms at 1022, 1023 and 1024 bytes and reject 1025-byte tokens.
- Core copy, uniform, pixel-pack, pixel-unpack and transform-feedback buffer
  targets and additional usage enums. A first copy-target binding establishes
  other-data classification; later copy bindings preserve an existing type.
  Element/non-element classification and cross-context ownership remain strict.
- Native buffer copies check both ranges and same-buffer overlap before writing;
  successful copies also update the browser-owned index-validation mirror.
- `getBufferSubData` reads actual mapped native storage into bounded owned bytes.
  It never exposes a native address. Zero-length and invalid-range behavior is
  checked; the binary host reply path avoids per-byte JSON serialization.
- Unsigned scalar/vector uniform uploads and queries preserve the complete u32
  range. All six non-square matrix shapes use native column-major storage.
  Reflection determines reply sizes; locations retain their owner/link generation.
- Private surface allocation and capture temporarily unbind pixel buffers and
  restore them on exit. Owned CPU pixel opcodes cannot be confused with future
  PBO-offset overloads. Resize/capture preserve author buffer contents and bindings.
- Core vertex arrays retain element bindings and integer pointer formats across
  object deletion/rebuild. Instancing works with ESSL300 vertex IDs without
  WebGL1's extension-only per-vertex attribute requirement.
- Fixed primitive restart excludes the maximum index marker from attribute
  bounds checks for all three index widths. Range hints cannot reject otherwise
  valid indices; checked indexed drawing remains the shared safety boundary.
- Integer vertex inputs preserve signedness, half-float and packed inputs use
  their real byte widths, and constant queries retain their last upload type.
  Constant values belong to the context, not the vertex array.
- Closed scalar capability queries use native 32/64-bit reply widths. Native
  `MAX_ELEMENT_INDEX` bounds indexed draws; rasterizer discard changes real
  pixel output without disturbing the program or vertex-array state.
- Sized 2D color/integer/depth storage has a closed format/type table checked
  against actual ANGLE allocation. Half-float storage accepts float uploads
  without losing HDR values; unsigned integer sampling preserves u32 values.
- Immutable 2D/cube mip chains are bounded, robustly initialized and cannot be
  redefined. Compatible subimage updates preserve unaffected levels. Mutable
  NPOT mip generation records dimensions and charges growth, not repeated
  regeneration; immutable generation does not allocate storage again.
- Owned 3D/array uploads bound every aligned row and slice, reject PBO confusion,
  and validate subregions before native reads. Immutable volume mips shrink depth;
  array mip levels preserve layer counts. Target bindings/deletion remain separate.
- Private capture and compositor clears preserve independent native read/draw
  framebuffer bindings. Capture restores the private read route even when it was
  `NONE`; resize remaps the retired private read surface without changing an
  author read framebuffer.
- Core read/draw framebuffer bindings, per-framebuffer read routes and deletion
  detach resources from both bound objects. Layered attachments report real mip
  and layer identity; depth/stencil alias writes replace both logical slots.
- Every uninitialized array/volume slice is explicitly zeroed using bounded
  reusable upload tiles. A native test caught the pinned ANGLE backend treating
  a clear of one attached layer as initialization of the whole image; the browser
  must not expose the remaining layers' native allocation bytes.
- Native multisample renderbuffer allocation selects supported sample counts,
  charges actual rounded-up storage, and exposes bounded format-specific sample
  queries. Four-sample resolves copy GPU pixels into a separate texture while
  preserving framebuffer bindings. Invalid extents, aliases, formats and sample
  requests do not alter existing storage; blit rectangle arithmetic is widened
  before applying WebGL2's signed-width overflow rule.
- Typed color/depth/stencil clears and integer/float readback preserve native
  component types, negative values, u32 maxima and HDR values. The WebGL2-only
  `EXT_color_buffer_float` admission enables its seven specified renderbuffer
  formats without leaking WebGL1's narrower color-extension contract.
- Opaque sampler objects maintain independent texture-unit bindings and scalar
  filter/wrap/LOD/compare state. Native pixel tests verify sampler overrides do
  not mutate texture parameters and deletion restores texture sampling on every
  previously bound unit. Cross-context handles and invalid shapes are rejected.
- Uniform blocks reflect actual std140 member offsets, matrix strides, shader
  references and active indices. Indexed uniform base/range bindings retain
  buffers, validate alignment and integer domains, and preserve unrelated generic state
  during deletion. Base sizes track reallocation; explicit ranges do not. Native
  draw validation rejects missing/undersized block storage before changing pixels.
- Owned pixel transfers account for aligned row lengths, skip rows/pixels and
  volume image strides/skips with checked arithmetic and non-overlap constraints.
  Readback preserves skipped destination bytes. Private surface captures and
  zero initialization reset and restore all author pixel-store overrides.
- Pixel-buffer transfers use separate closed offset opcodes, checked alignment
  and storage bounds. A zero offset is a GPU buffer address, not a null CPU
  upload. GPU readback can feed a subsequent texture upload without synchronous
  mapping; native readback preserves skipped bytes and rejects invalid writes.
- Occlusion and primitive query objects use native GPU counters, exact target
  identity, shared occlusion-target exclusion and implicit end on deletion.
  A cached result cannot change during the creating task, even after `finish`.
- Pointer-sized native fence handles never leave the owner thread. Browser names
  are globally distinct from buffer/query names. Zero-time waits preserve the
  interactivity budget, native completion is cached between tasks, and teardown
  destroys fences before EGL. Fixed scalar sync queries never return a pointer.
- Native transform feedback captures interleaved/separate float and unsigned
  outputs into retained object-local ranges, supports pause/resume and reflects
  linked names/types. Tests count real captured primitives, preserve untouched
  range bytes, and reject indexed capture aliases before GPU reads or writes.
- BC1/2/3, compressed sRGB and signed/unsigned BC4/5 reuse ANGLE's existing
  native storage and decoder. Extension availability is independent of browser
  enablement; the linear S3TC extension requires every native DXT family.
  Owned 2D/cube uploads enforce exact block footprints and family-specific mip
  and subregion rules. These complete public WebGL1 slices also run through the
  shared Window/Worker bindings and contained renderer. Their WebGL1 admission
  is independent of the separate public WebGL2 interface.
- Staged core compressed storage covers immutable 2D/cube mip chains and array
  layers. Every level/layer is explicitly initialized in bounded encoded tiles;
  BC1 RGBA uses transparent blocks rather than treating opaque zero blocks as
  transparent. Owned array uploads and distinct pixel-unpack-buffer offsets
  preserve neighboring layers, reject unsafe ranges, and do not expose addresses.
- Core texture facilities no longer advertise obsolete WebGL1 extension names.
  `EXT_color_buffer_half_float` independently admits R16F, RG16F and RGBA16F,
  without requesting an obsolete OES upload extension or granting RGB16F/full
  float rendering. Native HDR readback verifies missing-channel defaults.
- Default framebuffer attachment queries translate BACK/DEPTH/STENCIL into the
  actual private storage, reflect granted alpha/depth/stencil bits, and report
  logical default ownership instead of leaking native texture/renderbuffer names.
  Independent read/draw bindings and read routes remain untouched.
- Texture uploads, copies, immutable allocations and generated mip chains share
  per-image lifetime high-water accounting. Replacing a texture with equal or
  smaller storage does not consume the full budget again; only growth is charged.
  Reservations validate the complete operation before touching native storage
  and commit only after successful allocation. Failed native mip generation
  does not reserve nonexistent levels. Deleted object names intentionally do not
  reclaim the context-lifetime budget while native objects may remain referenced.
- GPU framebuffer copies update checked array-layer subregions without changing
  immutable definitions, charging new image storage, or interpreting author
  pixel-store/PBO state. Read and draw framebuffer identities remain separate.
  Renderable 3D destination slices use private native framebuffer blits, avoiding
  the pinned provider's corrupting 2D-to-3D staging path. The provider still
  validates copy formats, read routes, multisample sources and feedback through
  its fully validated zero-extent operation, which never executes a copy.
  Non-renderable destinations remain rejected pending a GPU conversion path.
- Requesting a promoted WebGL1 extension cannot disable its GLES3 core facility.
  Geometry/shader names are neither advertised nor admitted as legacy objects;
  core derivatives, fragment depth and other operations remain enabled.
- Same-size canvas dimension assignment clears the drawing buffer without
  reallocating its attachments or requiring a temporary resource-budget overlap.
  Scissor/write masks, clear values, viewport and author framebuffer bindings
  survive the mandatory reset. Native activation checks EGL's authoritative
  thread-local context/display/surface bindings before avoiding a redundant bind.
  Ordinary JSON replies avoid a recursive reviver unless special floating-point
  markers are present; tests preserve NaN, infinities, negative zero and strings.

These features are tested with native byte/pixel assertions, malformed commands,
failed-update atomicity, stale locations and peer-context handles. Their internal
presence is not a complete WebGL2 conformance result. Public admission and the
separately measured HTML5test score are described in the README.

## Public bindings and remaining verification

Framebuffer invalidation validates its target, attachment domain and signed
rectangle, then preserves contents. This is the explicitly permitted WebGL no-op
policy, not a discard optimization; native undefined-content hints are not
forwarded. Real integer, HDR, depth/stencil, multisample and separate read/draw
tests establish preservation. True 3D copies now preserve neighboring slices
through private GPU blits; non-renderable RGB float/integer destinations use
the bounded native readback and upload path described below.

Queries and sync use a typed owner-thread task boundary, never an author command.
The host publishes ready native results after document/worker tasks and their
jobs, including between callbacks in a timer batch. Related document realms
share the boundary; unrelated workers do not. Owner enumeration uses weak Rust
bridge references and does not enter V8 merely to publish native results.
`finish`, nested checkpoints,
capture and compositor presentation cannot publish results. Tests cover both
real native fences/queries and task/promise ordering before public admission.
Error replies from `clientWaitSync` must become `WAIT_FAILED` at the realm boundary.

Transform-feedback buffer deletion uses native active detachment while retaining
inactive containers' storage. A generated, never-bound name reservation prevents
the pinned ANGLE backend's numeric-ID detachment from aliasing older allocations.
The reservation retires with the final container reference, without rebuilding
GPU-written storage from a CPU mirror. Allocation failure loses only its owner
context. The existing ANGLE backend
supports transform-feedback arrays when its internal context is GLES3.1. The
WebGL2 provider now requests that version, while the separate WebGL2 shader
validator still rejects ESSL310 and the closed command dispatcher exposes no
GLES3.1-only operations. Array results come from native GPU buffer storage,
not CPU-generated substitutes. Broader conformance verification remains open.

Core entry points use versioned realm bindings and coherent overloads, not
WebGL1 extension objects exposed under new names.

The public realm has a distinct WebGL2 prototype and genuinely branded
query, sampler, sync, transform-feedback and vertex-array objects. All resource
interfaces inherit `WebGLObject`; reflection records and uniform locations do
not. Rejected native deletions do not prematurely retire the JavaScript brand.
Shared shader reflection records now expose readonly, enumerable prototype
getters backed by private native-result snapshots. Forged and proxy receivers
cannot read those slots; deleting a program or losing its context does not
invalidate an existing record. WebGL operations use Web IDL property descriptors
in both versions, including the compressed-texture overloads.
Window and Worker unit tests use ordinary public `getContext('webgl2')`, not
a private admission shortcut. Native interface constructors remain illegal
for author code. Captured WeakMap operations protect context/resource/extension
brands from author replacements of built-in WeakMap methods.

Implemented bindings include indexed buffers, owned-byte buffer readback,
uniform-block reflection, unsigned/non-square uniforms, integer attributes,
independent framebuffer routes, typed clears, multisample queries/resolves,
invalidation, and opaque fences. Buffer and uniform ranges use source elements;
pixel-buffer transfers use byte offsets. Intrinsic view validation rejects
detached and out-of-bounds resizable buffers and accepts genuine shared views
without consulting shadowed author properties or typed-array iterators.
Native fragment-output reflection uses a fixed ANGLE entry point and bounded
NUL-free names. Connected-canvas drawing uses the normal paint checkpoint.

Framebuffer-to-volume copies retain ANGLE's complete copy-format, read-route,
feedback and multisample validation. Renderable destinations use private GPU
blits. The pinned provider's direct and alternate RGB-float 3D copy paths
corrupt neighboring layers, so non-renderable RGB float/integer destinations
use bounded native readback and regional upload instead. This path preserves
HDR and integer values, clips reads without replacing destination neighbors,
and restores native pack/unpack/PBO state without changing author bindings.
It allocates only the copied rectangle and accounts for both simultaneous
buffers against the context resource budget; it is not a GPU-only fast path.

WebGL2 creation honors `antialias` through real four-sample native
color and optional depth/stencil renderbuffers. A private single-sample color
surface receives GLES3 resolves for presentation, client/PBO pixel reads and
texture copies. Explicit author multisample framebuffers retain their ordinary
resolve requirements. Queries and granted attributes agree with actual samples;
the shared restoration path recreates the same kind of drawing buffer. Native
allocation failure rejects creation rather than claiming ungranted antialiasing.
All multisample and resolve attachments count against the existing storage
budgets, including temporary old/new overlap during resize. WebGL1's current
single-sample admission is unchanged. Native and realm tests measure partial
diagonal-edge coverage instead of relying on the antialias attribute alone.

Canvas creation and resize requests negotiate a smaller native extent when
the requested dimensions exceed the bitmap or attachment-storage budget.
The integer size policy includes multisample storage and resize overlap;
texture/renderbuffer allocations keep their strict validation. Canvas content
attributes remain unchanged, while `drawingBufferWidth`/`drawingBufferHeight`
report actual native storage and bitmap transfers use the snapshot's extent.
Every successful assignment clears the native bitmap without resetting the
viewport. If no overlapping storage is available, a smaller existing buffer
may be reused and cleared; an unsatisfiable shrink retains the existing buffer
and reports `OUT_OF_MEMORY`. Tests cover zero dimensions, bounded oversized
requests, memory accounting and preservation of author masks/scissor. Hidden
Chrome reference captures confirm native-size bitmap transfer and the 1×1
minimum for zero-sized canvas requests; implementation size caps can differ.

Texture binding work includes typed 2D/volume uploads, immutable storage,
layer attachments, compressed element-offset/length and PBO overloads, and
typed pixel readback. The WebGL2 numeric constants are derived from
the Khronos IDL's literal declarations, with its compatible permission notice
retained in the table. No executable upstream code was imported. DOM image
sources now support subrectangles, slice strides, flip/premultiplication, and
unsigned-byte/float/half-float conversions using the existing decoded Canvas
snapshots. The private native upload guard restores every unpack scalar and
rejects a bound PBO rather than changing overloads silently. Packed DOM-image
uploads now include RGB565, RGBA4, RGB5_A1, RGB10_A2, and R11F_G11F_B10F.
Synthetic color/low-alpha pixel results are checked against hidden Chrome,
including the eight-bit intermediate used by packed premultiplied uploads.
The DOM-upload table is validated natively and remains narrower than the
application-owned typed-buffer table. `RGBA8`, omitted from the current generated
Khronos IDL but required by GLES3 storage and the WebGL2 2.0.0 specification,
is included explicitly. Genuine ImageData and OffscreenCanvas sources have
private brands; prototype-forged objects cannot become upload sources, including
during context loss. The existing WebGL1 regression suite must stay green
throughout; public context availability is not a full-conformance claim.

Shared Window/Worker tests, unchanged upstream WebGL2 cases and hidden Chromium
fixtures exercise public availability. Running the unchanged sibling game remains a separate end-to-end
acceptance milestone; see [the game roadmap](gd-clone-compatibility.md).

Indexed uniform and transform-feedback ranges may be declared before allocating
buffer storage, and may extend beyond it. They are metadata, not permission to
access nonexistent bytes: real native draws still reject undersized storage.
Capacity checks use widened arithmetic before admitting oversized point capture.

Repeated reads avoid redundant multisample resolves until a default-buffer write.
A separate cache retains at most 64 KiB of actual, successfully read GPU pixels
for an exact tight RGBA8 default-buffer rectangle. Its retained capacity is
charged against context/process budgets; exhausted optional cache capacity falls
back to native reads without changing the result. Author FBOs, pack-buffer
writes, padded and out-of-bounds reads never use this cache. Drawing, clearing,
resize and unpreserved presentation invalidate pixels. A bounded caller-side
fence-poll cache retains only successful task-stable status and side-effect-free
zero-time waits. The trusted task boundary clears it; flush waits always reach
ANGLE. Query-availability polls follow the same task-stable rule. Repeated
`finish()` calls may reuse a successful native finish only while no new non-poll
command has intervened; the first finish still drains real native work. Neither
finish nor these caches publish completion from inside an author task.

Built-in transform-feedback outputs such as `gl_Position` and `gl_PointSize`
are admitted as capture names. The reserved-name restriction for author GLSL
declarations does not apply to selecting existing built-in outputs; ANGLE
validates their availability at link time. Focused tests read actual captured
values and require active capture bindings to remain unchanged after errors.

Generated shader sources have a separate 256 KiB admission budget. Logs and
reflection scratch remain bounded to 32 KiB, translated output to eight times
the source cap, and retained source/driver storage to the existing aggregate
resource budgets. Command transport accounts for worst-case JSON escaping
without truncating author source. Oversized or unbudgeted replacements leave
previous source and compiler state intact. This is a browser-wide policy, not a
Three.js-specific exception.

Location queries follow their own reserved-name rules: linked programs return
`-1` for reserved attribute names and `null` for reserved uniform names, without
an error. Attempting to bind those names remains `INVALID_OPERATION`; unlinked
program queries still fail before the absent-location result. Query names use
the GLSL source character set, rather than accepting every ASCII control or
rejecting valid array/structure punctuation. This is shared by both context
versions and checked at the native boundary as well as through public bindings.
`TexImage`'s integer internal-format parameter uses `INVALID_VALUE` for
unsupported values; immutable `TexStorage` retains its distinct `INVALID_ENUM`
contract. Invalid redefinitions preserve storage and accounting.

### Unmodified framework acceptance

The optional [Three.js fixture](../tests/webgl/three-rendering.html) imports a
caller-supplied, same-origin installed release through its `module` query
parameter. It does not bundle, patch, downgrade or depend on a CDN copy of the
library. The locally inspected Three.js 0.185.1 package identifies its upstream
as `mrdoob/three.js` and includes the MIT license; retain that provenance and
license when preparing a reference checkout. The fixture itself contains only
our small acceptance scenes, not copied third-party renderer/shader sources.

Serve the repository and the checked library from one loopback fixture root.
For example, a URL ending in
`/better-web-browser/tests/webgl/three-rendering.html?module=/reference-three/build/three.module.js`
uses an untouched reference checkout beside this repository. Run it with
`scripts/run-hidden-benchmark.ps1 -FreshProfile -SettleMs 3000`, collecting
`#results` and `.contract`, then the unified-headless Chromium harness against
the same URL. All profiles and reports belong in a task-specific G: directory;
use the owned fixture-server lifecycle and stop it in `finally`.

The cases exercise indexed data textures, instancing, multisampled and float
targets, volume/array sampling, shadowed and physical material shaders, morph
targets, half-float multisample resolve followed by post-processing, and resize.
Every case requires real pixels and no GL error. A successful context lookup or
the library's own capability flag is not accepted as evidence of rendering.
This remains a bounded framework test, not proof of sustained game performance
or that the unchanged sibling game already runs.

The required pinned Khronos manifest keeps the preceding WebGL1 extension cases
and adds WebGL2 public interfaces, buffers, samplers, queries/fences, vertex
arrays, uniforms, transform feedback, texture storage/uploads, readback and
blits. Expectations remain forbidden, and the combined suite must report at
least 6,000 real assertions. This curated gate is not the entire upstream suite.
The final local release replay passes 74 cases and 8,696 assertions, including
the original normalized-16 extension and decoded 10bpc image tests. Upstream
resources are pinned in the manifest so a fresh CI checkout includes their real
helper scripts and PNG data; no local replacement helper is served.
The wider investigation also records gaps in synchronous XHR and blob/nested
worker loading, rather than marking their tests as expected failures. A layered
depth/stencil attachment query conflicts with the latest WebGL2 specification's
different-image rule; the implementation retains that rule rather than changing
it merely to match a legacy assertion. Those cases are not presented as passes.

### Adapter selection and source precision

Production creation tries a suitable hardware D3D11 adapter before software
WARP. `powerPreference` uses trusted DXGI adapter selection when ANGLE exposes
its LUID selection extension; unsupported hints do not prevent ordinary hardware
creation. `failIfMajorPerformanceCaveat` disables the software fallback. A failed
attempt never locks a canvas into a context mode. Native unit tests explicitly
use WARP for reproducibility. Combined texture-unit admission is bounded to 256
owned binding slots; the query, owned arrays, shader validator and native ESSL
built-ins retain the same driver limit. A larger unsupported limit rejects that
backend instead of clamping only the public query.

Context restoration retains the original converted adapter-admission options,
including `powerPreference` and `failIfMajorPerformanceCaveat`. Initial creation
and restoration share one native-options encoder; restoration does not reread
author dictionary getters or fall back to software after hardware-only admission
fails. Failed restoration remains lost and permits a later explicit retry under
the same policy.

Canvas presentation distinguishes author writes to the default drawing buffer
from offscreen framebuffer work. The native owner tracks pending presentation
independently of multisample resolve and CPU-readback caches. A paint checkpoint
checks that state once after draining ordered commands; it does not query driver
bindings per draw. FBO-only clears/draws cannot replace the last displayed Canvas
bitmap with its implicitly cleared unpreserved backing buffer. Explicit exports
and readback still observe the actual backing buffer, while resize and subsequent
default-buffer writes schedule a new bitmap.

The local `tests/webgl/framebuffer-presentation.html` capture checks both parts
of this contract: the unpreserved default buffer reads transparent black after
retirement, while the displayed Canvas remains red after a later offscreen green
clear. October 5 hidden captures reproduce a blank displayed Canvas in the
preceding release and retain red in the patched browser, matching Chromium.
The fixture needs no external library or replaced implementation.

Genuinely decoded 16-bit integer images retain a private precision sidecar for
WebGL floating-point and high-precision packed uploads. The existing `image`
decoder and `moxcms` color transforms are reused; ordinary page painting remains
RGBA8. Color conversion, bitmap cropping/resizing, alpha handling, structured
clone and transfer preserve the decoded words before destination quantization.
Author-visible properties cannot fabricate this provenance. Decode and resize
scratch remain bounded. This does not add float16 ImageData, HDR display output
or a higher-precision decoder for every supported image codec.

The pinned ANGLE provider still has an immutable NPOT base-level storage bug
that can lose the D3D device. Its upstream
[storage-retention fix](https://chromium.googlesource.com/angle/angle.git/+/248abdcad1e48f1753a5ecba1575e38494afe695%5E%21/)
has not been backported here. The wider investigation retains this failure;
the curated gate does not claim coverage of that case. A dependency refresh or
licensed, reviewed backport remains required rather than padding textures or
changing the public API's permitted dimensions.

## Primary contracts and reuse

`KHR_parallel_shader_compile` is advertised only where the pinned ANGLE context
offers its native extension. Author admission enables real shader/program
`COMPLETION_STATUS_KHR` queries, returning booleans from ANGLE's non-blocking
completion state. Completion is distinct from compile/link success, including
failed shaders. The WebGL interface deliberately omits native thread-count
controls. After context loss completion queries return true so retained polling
loops terminate; restoration requires fresh extension admission and objects.
This does not move Breeze's own WebGL shader validation off-thread or claim that
every compilation/linking call is stall-free. Native and public tests cover
admission, successful/failed compilation, object ownership and restoration.
See the [KHR extension contract](https://registry.khronos.org/webgl/extensions/KHR_parallel_shader_compile/).
The local `tests/webgl/parallel-compile.html` fixture matches hidden Chrome in
both context versions: the extension is available, completion is boolean,
valid compilation succeeds, invalid compilation fails but completes, and the
queries leave `NO_ERROR`. This fixture does not require a pending interval:
a small shader may finish before the first poll.

For full local WebGL unit runs, use `cargo test --locked --lib webgl --
--test-threads=8`. These tests share the production GPU owner, whose intentional
process-wide limit is 16 native contexts. Unrestricted Rust test concurrency on
machines with more cores can exhaust that limit while unrelated tests hold
contexts, causing correct context-admission rejection rather than a shader
failure. The bounded run still executes every selected test concurrently; it
does not increase the browser's resource limits or skip compliance tests.

- [WebGL2 specification](https://registry.khronos.org/webgl/specs/latest/2.0/)
- [OpenGL ES API registry](https://registry.khronos.org/OpenGL/index_es.php)
- [ANGLE explicit context version](https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_create_context_backwards_compatible.txt)

All native operations reuse the existing locked `mozangle` 0.7.1 provider and
shader compiler. The small typed entry-point adapters use the pinned Khronos
GLES3 header ABIs; they do not resolve author-supplied symbols, add dependencies,
copy a third-party renderer, or weaken native WebGL validation. Provenance and
licensing are recorded in [the backend documentation](webgl-backend.md).
