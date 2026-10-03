# WebGL 1 texture and HDR foundations

This slice reuses the existing pinned ANGLE renderer and compiler. It adds no
dependency, copied renderer code, or site-specific texture path. WebGL 2 remains
unavailable until its separate API contract is implemented and verified.

## Author capability boundaries

Native capability discovery and successful author extension enablement are
distinct. Extension state is owned by one context and is cleared on restoration.
Merely finding a native GLES capability never admits a new public enum or type.

Implemented contracts in this batch are `OES_texture_float`,
`OES_texture_half_float`, their two linear-filter extensions,
`WEBGL_color_buffer_float`, `EXT_color_buffer_half_float`, and
`EXT_texture_filter_anisotropic`. Depth textures and sRGB are not yet advertised.
Availability depends on the actual native provider, not a hard-coded browser list.

Requesting float/half-float textures implicitly enables the corresponding color
extension when available, as required by Khronos. Color requests also require
their base texture extension. Legacy extension interfaces have no global
constructor; each context caches its extension instance within its lifetime.

## Pixel and native storage contract

- Float uploads use `Float32Array`; half uploads use `Uint16Array` binary16 bits.
  Wrong views, undersized buffers, disabled types and unsupported format/type
  combinations fail before any native author-memory access.
- RGB/RGBA floating images use sized GLES3 storage. Legacy alpha/luminance images
  use red/RG storage with private channel swizzles. Replacement storage resets
  that swizzle; subuploads cannot alias two public formats sharing a native format.
- Private red/RG mappings must not grant color-attachment renderability to legacy
  alpha/luminance images. Framebuffer checks, clear, draw, copy and readback retain
  the public WebGL1 completeness restrictions.
- GLES3's core half-float filtering does not implicitly enable WebGL1's linear
  extension. Before that extension is enabled, filtered half textures sample
  incomplete-texture opaque black; author bindings and filters remain unchanged.
- DOM image uploads reuse the origin-clean Canvas snapshot boundary, including
  flip and premultiplication policy. V8's existing binary16 conversion supplies
  half conversion; there is no second hand-written floating-point converter.
- Float render targets retain HDR values outside `[0,1]`. Their supported read
  pair is `RGBA/FLOAT`; normalized targets retain `RGBA/UNSIGNED_BYTE`. The
  implementation read-type query follows the bound target. Destination offsets,
  padding and guard bytes remain owned and bounded.
- Allocation accounting charges expanded native storage, not just upload bytes.
  Existing context/owner budgets and upload bounds remain in force; rejection
  must not kill the renderer or modify a previously valid image.
- Anisotropy retains fractional values, rejects values below one, and clamps
  values above the provider limit (a policy explicitly permitted by EXT).

Focused tests exercise actual native shader sampling, filtering, DOM conversion,
renderbuffer clears, HDR readback, failed subuploads and framebuffer restrictions.
These tests are not a claim of complete upstream conformance or game readiness.
Upstream and reference-browser fixture evidence will be recorded with the PR.

## Primary contracts

- [OES_texture_float](https://registry.khronos.org/webgl/extensions/OES_texture_float/)
- [OES_texture_half_float](https://registry.khronos.org/webgl/extensions/OES_texture_half_float/)
- [OES_texture_float_linear](https://registry.khronos.org/webgl/extensions/OES_texture_float_linear/)
- [OES_texture_half_float_linear](https://registry.khronos.org/webgl/extensions/OES_texture_half_float_linear/)
- [WEBGL_color_buffer_float](https://registry.khronos.org/webgl/extensions/WEBGL_color_buffer_float/)
- [EXT_color_buffer_half_float](https://registry.khronos.org/webgl/extensions/EXT_color_buffer_half_float/)
- [EXT_texture_filter_anisotropic](https://registry.khronos.org/webgl/extensions/EXT_texture_filter_anisotropic/)
- [ANGLE explicit context version](https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_create_context_backwards_compatible.txt)
