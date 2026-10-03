# Internal WebGL2 foundation

This is a staged native implementation, not an advertised canvas context.
`canvas.getContext('webgl2')` still returns null. The ordinary WebGL1 creation
dictionary does not forward arbitrary author options into the native creation
protocol. Do not add an interface merely to satisfy a feature probe.

The closed internal `api` value selects an exact GLES2 or GLES3 provider through
the existing pinned ANGLE dependency. Both retain WebGL compatibility, robust
resource initialization, disabled client arrays, thread affinity, ownership
budgets and hidden execution. A newer native driver must not silently change
WebGL1's shader rules or extension admission.

## Completed native contracts

- Exact GLES3 construction, private initialized drawing buffer and versioned
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

These features are tested with native byte/pixel assertions, malformed commands,
failed-update atomicity, stale locations and peer-context handles. Their internal
presence is not a complete WebGL2 conformance result or a new HTML5test score.

## Remaining admission work

Queries and sync currently use an internal simulated task-completion opcode in
native tests. No realm forwards that opcode. Public admission needs a trusted
host completion hook after the task and its microtask checkpoint; `finish`,
capture and compositor presentation must not stand in for event-loop completion.
Error replies from `clientWaitSync` must become `WAIT_FAILED` at the realm boundary.

Transform feedback still rejects deletion of a buffer attached to an active
capture: the retained native-name model needs a deletion path which preserves
other objects while allowing native active detachment. The exact GLES3.0 provider
also rejects array capture at link time; no CPU-generated capture result conceals
that limitation. These require further admission tests and provider decisions.

Coherent realm overloads need their own bindings. Public core entry points need
versioned realm bindings, not WebGL1 extension objects under new names.

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
