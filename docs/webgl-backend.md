# WebGL backend work in progress

This branch does not yet expose `WebGLRenderingContext` or return a WebGL context
from Canvas. The native foundation is implemented; author-facing WebGL must stay
unadvertised until the remaining integration and compatibility tests pass.
HTML5test is guidance, not an acceptance substitute. The fresh pre-change Windows
baseline is **487/588**. No score increase is claimed for this foundation.

## Backend and provenance

The Windows backend uses exactly pinned [`mozangle` 0.7.1](https://github.com/servo/mozangle),
a BSD-3-Clause packaging of Mozilla's ANGLE fork. EGL and GLES2 are compiled from
the locked Cargo source; there is no downloaded browser DLL, author-provided native
library, custom GLSL parser, or validation-only NULL renderer. ANGLE provides the
GLSL compiler and D3D11 renderer. The initial policy uses D3D11 WARP, which performs
actual software rendering and supports hidden CI without a native window.

The published package's `UPSTREAM`, root license, native ANGLE license, Cargo
manifest, build script and relevant shader/context implementations were inspected.
That is a provenance/build-policy review, not a claim of a comprehensive native
security audit. The build script compiles local source and generates bindings;
the selected build path does not execute downloaded installer scripts. License
packaging and the native build preflight still need integration before release.

`.cargo/config.toml` selects ANGLE's upstream-supported
`ANGLE_STD_ASYNC_WORKERS=0` policy. This keeps shader compilation within the
renderer task and avoids the default `std::async` worker's exception-runtime symbol
conflict with the pinned static V8 archive. It does not disable shader validation,
rewrite third-party headers, or suppress linker diagnostics. The actual EGL context
requests WebGL compatibility, robust resource initialization and disabled client
arrays. Backend failure must produce context-creation failure, not a fake context.

## Implemented native boundary

- Thread-affine context ownership, pbuffer lifecycle and per-realm teardown on
  document cancellation and worker shutdown. Destroying one context must not
  terminate ANGLE's shared display or invalidate a surviving peer.
- A dedicated native owner thread for display, GLSL compiler TLS and resource
  lifetime. Realm threads submit bounded commands, never move native contexts.
  Requests have a three-second deadline and one pending-upload slot. Owner leases
  ensure timed-out creations cannot retain orphaned contexts; retirement does not
  wait behind a full upload queue.
- Browser-owned RGBA drawing framebuffer, with optional depth/stencil storage.
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

Initial limits: eight contexts per realm and sixteen per renderer, 1,024 live objects per context,
64 MiB resource budget per context and 128 MiB across native contexts, 16 MiB uploads, 32 KiB shader source,
one million vertices per draw and four million drawing-buffer pixels. Resource
The resource budget includes drawing-buffer attachments and both native buffer
storage and its CPU mirror. Charges are not reclaimed on deletion because GLES can retain references after
public deletion. Limits must remain explicit and fail closed.

The native unit tests exercise real shader-generated pixels, zero initialization,
out-of-bounds draw rejection without pixel damage, malformed commands, wrong object
types, peer-context lifetime, cross-context names and presentation retirement.
These are not a WebGL conformance claim.

## Remaining before author exposure

Complete uniform queries, textures, framebuffer/renderbuffer objects and typed parameter
queries; correct deleted-object and link-generation semantics; skip unused vertex
attributes during range validation; implement Canvas resize without destroying
live resources. Integrate branded IDL bindings, context attributes, loss/restore,
Canvas/OffscreenCanvas painting and export, and origin-clean texture restrictions.
The presentation hook must clear after actual compositing, not after arbitrary
snapshot/export reads. Premultiplied-alpha compositing requires explicit handling.

Complete native/transitive license packaging, run the existing local checks and
regression suites, then perform hidden
pixel comparisons against Chromium and fresh HTML5test runs. No WebGL 2, WebGPU,
WebVR, WebXR or unsupported extension should be advertised by this slice.

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
