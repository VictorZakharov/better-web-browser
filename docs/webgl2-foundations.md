# Internal WebGL2 foundation

This is a staged native implementation, not an advertised canvas context.
`canvas.getContext('webgl2')` still returns null. The ordinary WebGL1 creation
dictionary does not forward arbitrary author options into the native creation
protocol. Do not add an interface merely to satisfy a feature probe.

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
  targets, additional usage enums, and neutral copy-target classification.
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
  buffers, validate alignment and bounds, and preserve unrelated generic state
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
  shared Window/Worker bindings and contained renderer. They do not depend on
  admitting an incomplete WebGL2 canvas interface.
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
presence is not a complete WebGL2 conformance result or a new HTML5test score.

## Remaining admission work

Framebuffer invalidation validates its target, attachment domain and signed
rectangle, then preserves contents. This is the explicitly permitted WebGL no-op
policy, not a discard optimization; native undefined-content hints are not
forwarded. Real integer, HDR, depth/stencil, multisample and separate read/draw
tests establish preservation. True 3D copies now preserve neighboring slices
through private GPU blits; non-renderable destinations still need a conversion
path before coherent public admission.

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
not CPU-generated substitutes. Public admission still requires broader tests.

Coherent realm overloads need their own bindings. Public core entry points need
versioned realm bindings, not WebGL1 extension objects under new names.

The staged realm now has a distinct WebGL2 prototype and genuinely branded
query, sampler, sync, transform-feedback and vertex-array objects. All resource
interfaces inherit `WebGLObject`; reflection records and uniform locations do
not. Rejected native deletions do not prematurely retire the JavaScript brand.
Private unit-test admission reaches the real GLES3 backend without advertising
partial support through public `getContext('webgl2')` or interface globals.

Implemented staged bindings include indexed buffers, owned-byte buffer readback,
uniform-block reflection, unsigned/non-square uniforms, integer attributes,
independent framebuffer routes, typed clears, multisample queries/resolves,
invalidation, and opaque fences. Buffer and uniform ranges use source elements;
pixel-buffer transfers use byte offsets. Intrinsic view validation rejects
detached and out-of-bounds resizable buffers and accepts genuine shared views
without consulting shadowed author properties or typed-array iterators.
Native fragment-output reflection uses a fixed ANGLE entry point and bounded
NUL-free names. Connected-canvas drawing uses the normal paint checkpoint.

Texture binding work includes typed 2D/volume uploads, immutable storage,
layer attachments, compressed element-offset/length and PBO overloads, and
typed pixel readback. The 262 WebGL2 numeric constants are derived solely from
the Khronos IDL's literal declarations, with its compatible permission notice
retained in the table. No executable upstream code was imported. DOM image
sources now support subrectangles, slice strides, flip/premultiplication, and
unsigned-byte/float/half-float conversions using the existing decoded Canvas
snapshots. The private native upload guard restores every unpack scalar and
rejects a bound PBO rather than changing overloads silently. Packed DOM-image
conversions and the complete interface contract remain admission work, not
claimed features. The existing WebGL1 regression suite must stay green
throughout.

Only after the coherent interface is admitted should shared Window/Worker tests,
unchanged upstream WebGL2 cases and hidden Chromium fixtures establish public
availability. Running the unchanged sibling game remains a separate end-to-end
acceptance milestone; see [the game roadmap](gd-clone-compatibility.md).

## Primary contracts and reuse

- [WebGL2 specification](https://registry.khronos.org/webgl/specs/latest/2.0/)
- [OpenGL ES API registry](https://registry.khronos.org/OpenGL/index_es.php)
- [ANGLE explicit context version](https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_create_context_backwards_compatible.txt)

All native operations reuse the existing locked `mozangle` 0.7.1 provider and
shader compiler. The small typed entry-point adapters use the pinned Khronos
GLES3 header ABIs; they do not resolve author-supplied symbols, add dependencies,
copy a third-party renderer, or weaken native WebGL validation. Provenance and
licensing are recorded in [the backend documentation](webgl-backend.md).
