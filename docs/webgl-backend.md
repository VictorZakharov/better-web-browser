# WebGL 1 native backend

Windows Canvas and OffscreenCanvas expose real WebGL 1 rendering, including worker
rendering and Canvas painting/export. This is a bounded baseline, not a full WebGL
conformance certification. HTML5test is guidance, not an acceptance substitute.
The initial native baseline measured **502/588**, up from **487/588**.
Subsequent extensions are judged by actual rendering and conformance assertions,
not by an assumed HTML5test score increase.

## Backend and provenance

The Windows backend uses exactly pinned [`mozangle` 0.7.1](https://github.com/servo/mozangle),
a BSD-3-Clause packaging of Mozilla's ANGLE fork. EGL and GLES are compiled from
the locked Cargo source; there is no downloaded browser DLL, author-provided native
library, custom GLSL parser, or validation-only NULL renderer. ANGLE provides the
GLSL compiler and D3D11 renderer. The initial policy uses D3D11 WARP, which performs
actual software rendering and supports hidden CI without a native window.

The published package's `UPSTREAM`, root license, native ANGLE license, Cargo
manifest, build script and relevant shader/context implementations were inspected.
That is a provenance/build-policy review, not a claim of a comprehensive native
security audit. The build script compiles local source and generates bindings;
the selected build path does not execute downloaded installer scripts. Release
archives retain wrapper, native ANGLE, Chromium, zlib, Khronos and embedded-source
notices. Packaging fails closed if the pinned native revision changes.
`UPSTREAM` pins `FIREFOX_153_3_0esr_RELEASE`, revision
`861fdeb0d32fe1bd101fea886687e680f612d735`.

`.cargo/config.toml` selects ANGLE's upstream-supported
`ANGLE_STD_ASYNC_WORKERS=0` policy. This keeps shader compilation within the
renderer task and avoids the default `std::async` worker's exception-runtime symbol
conflict with the pinned static V8 archive. It does not disable shader validation,
rewrite third-party headers, or suppress linker diagnostics. The actual EGL context
requests WebGL compatibility, robust resource initialization and disabled client
arrays. Backend failure must produce context-creation failure, not a fake context.

The WebGL1 provider requests **exact GLES 2.0**, with ANGLE's backwards-compatible
context upgrade disabled. A GLES3 WebGL-compatible provider applies WebGL2 shader
rules even to ESSL100: its `gl_FragData` array has one element, breaking valid
`WEBGL_draw_buffers` programs. The backend version must match the author contract;
turning off native WebGL validation is not an acceptable workaround.

Private format dependencies (`OES_rgb8_rgba8`, `EXT_texture_storage`, `OES_depth24`
and `EXT_draw_buffers`) are enabled internally without granting author APIs.
RGBA32F uses ANGLE's `CHROMIUM_color_buffer_float_rgba` storage contract; legacy
float/half-float and sRGB formats use their GLES2 extension tokens directly.
The separate WebGL1 validator enforces author extension admission. Its reversible
name prefix is removed from validated ESSL before native recompilation, preserving
legal 256-byte identifiers and public reflection without doubling their length.
WebGL2 remains unavailable pending its separate API and backend contract. Its
[internal GLES3 foundations](webgl2-foundations.md) are tested without advertising
a partial context. See the
[texture/HDR contract](webgl-texture-formats.md) for storage and validation details.

## Implemented native boundary

- Thread-affine context ownership, pbuffer lifecycle and per-realm teardown on
  document cancellation and worker shutdown. Destroying one context must not
  terminate ANGLE's shared display or invalidate a surviving peer.
- A dedicated native owner thread for display, GLSL compiler TLS and resource
  lifetime. Realm threads submit bounded commands, never move native contexts.
  Requests have a three-second deadline and one pending-upload slot. Owner leases
  ensure timed-out creations cannot retain orphaned contexts; retirement does not
  wait behind a full upload queue.
- Browser-owned RGBA (or opaque RGB) drawing framebuffer, with optional depth/stencil storage.
  EGL's pbuffer is not the author drawing buffer. Initial pixels are cleared;
  snapshot destinations are initialized and sized exactly before readback.
- Opaque, typed resource names that are unique across contexts. Author commands
  cannot select an arbitrary driver entry point or pass a client memory pointer.
- Initialized buffer allocation, bounded uploads/subuploads, shader compilation,
  program linking, vertex/index drawing and checked attribute byte ranges.
- Distinct sticky GL errors, invalid-command rejection, lifetime high-water
  resource accounting, top-left Canvas snapshot conversion and opaque alpha.
- Native post-presentation clearing for unpreserved buffers, restoring the
  author's framebuffer, clear values, scissor and write masks afterward.

Initial limits: eight contexts per realm and sixteen per native owner, 1,024 live objects per context,
64 MiB resource budget per context and 128 MiB across native contexts, 16 MiB uploads, 32 KiB shader source,
one million vertices per draw, four million drawing-buffer pixels and maximum
admitted dimensions of 4,096. The budget includes drawing-buffer attachments and both native buffer
storage and its CPU mirror. Charges are not reclaimed on deletion because GLES can retain references after
public deletion. Limits must remain explicit and fail closed.

The native unit tests exercise real shader-generated pixels, zero initialization,
out-of-bounds draw rejection without pixel damage, malformed commands, wrong object
types, peer-context lifetime, cross-context names and presentation retirement.
These are not a WebGL conformance claim.

## Author-facing integration and limitations

The bindings implement uniform queries, texture uploads/copies, framebuffer and
renderbuffer attachments, typed parameter queries, branded IDL objects and
receiver/arity checks. Shader/program deletion waits for retained native references;
uniform locations track link generations. Drawing validates active attribute ranges
and rejects every enabled null attribute buffer, including inactive attributes.
Canvas resize preserves live objects and author state while reinitializing pixels.
Readback preserves destination padding and out-of-bounds pixels.

The Canvas presentation checkpoint clears unpreserved native buffers after exporting
the browser-owned paint bitmap, not after arbitrary readback or `toDataURL`.
Premultiplied WebGL pixels convert once into the straight-alpha Canvas pipeline.
Opaque-alpha buffers ignore author alpha writes. Worker OffscreenCanvas rendering
uses the same native owner; bitmap export resets pixels without destroying resources.
Structured transfer of an active WebGL canvas is rejected rather than moving native
ownership incorrectly.

Texture sources reuse existing origin-clean decoded image, Canvas, ImageData and
ImageBitmap paths. ImageBitmap creation options are not overridden by unpack flags.
This does not establish complete TexImageSource/CORS/video coverage. Antialiasing
remains unavailable. Real context loss/restoration, instancing, vertex arrays,
unsigned indices and three shader extensions are described in the
[lifecycle and extension contract](webgl-lifecycle-and-extensions.md). Failed creation or
loss produces actionable events instead of a fake context. Rendering is Windows-only
software WARP; other platforms do not advertise a native WebGL backend.

Focused unit tests cover real pixels, isolation, malformed requests, resize
under author masks, deferred deletion, alpha, padding, presentation and worker export.
The shared `tests/fixtures/webgl-rendering.html` checks eighteen rendering/API
contracts against unified-headless Chrome, including indexed textured geometry
and Canvas-copy orientation. A required renderer smoke test runs the same eighteen
checks inside the AppContainer and verifies both owned bitmaps across IPC.
Full upstream WebGL conformance, accelerated adapters
and wider resource limits remain follow-up work. No WebGL 2, WebGPU, WebVR, WebXR
or unsupported extension is advertised. Measurements are recorded in the README
and PR; there is no score-specific browser behavior.

The long-term application target is the sibling Last Stand game; its real
WebGL 2, HDR and rendering-budget requirements are tracked in the
[gd-clone compatibility roadmap](gd-clone-compatibility.md).

The binding-parser preflight is `./scripts/prepare-angle.ps1`. It accepts an
explicit LLVM directory or discovers the installed LLVM `bin` directory, requires
`libclang.dll` version 19 or later, and sets `LIBCLANG_PATH` (including persistence
across CI steps). It does not download a compiler or bypass STL version checks.
The initial local native build used the official LLVM 23.1.2 Windows release
archive, verified against its published SHA-256
`ceaee048142fece144752c6f6431cb0905a7a6160f78ab8cf5cf0b6216f99418`,
with only its binding-parser DLL extracted to task-owned storage on G:.

Behavior follows the [WebGL 1 specification](https://registry.khronos.org/webgl/specs/latest/1.0/),
particularly drawing-buffer preservation, resource initialization, origin restrictions
and enabled-attribute range checking. ANGLE performs native shader translation;
the browser retains responsibility for WebGL's API and security contracts.
