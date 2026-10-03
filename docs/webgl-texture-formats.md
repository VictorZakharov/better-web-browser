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
`EXT_texture_filter_anisotropic`, `WEBGL_depth_texture`, `EXT_sRGB`, and
`WEBGL_draw_buffers`.
Availability depends on the actual native provider, not a hard-coded browser list.

Requesting float/half-float textures implicitly enables the corresponding color
extension when available, as required by Khronos. Color requests also require
their base texture extension. Legacy extension interfaces have no global
constructor; each context caches its extension instance within its lifetime.

## Pixel and native storage contract

- Float uploads use `Float32Array`; half uploads use `Uint16Array` binary16 bits.
  Wrong views, undersized buffers, disabled types and unsupported format/type
  combinations fail before any native author-memory access.
- RGBA32F uses ANGLE's sized GLES2 floating-color extension storage. Legacy
  alpha/luminance and half-float images retain their native GLES2 channel rules;
  no private red/RG swizzle is needed. Subuploads must retain the image's public
  format/type, even if a provider could perform an implicit conversion.
- Color-attachment renderability must not be granted to legacy alpha/luminance
  images. Framebuffer checks, clear, draw, copy and readback retain
  the public WebGL1 completeness restrictions.
- Native half-float filtering does not implicitly enable WebGL1's linear
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

## Multiple render targets

`WEBGL_draw_buffers` uses the pinned ANGLE implementation, with at least four
native color attachments and outputs before it can be advertised. Routing is
framebuffer-local. The default surface exposes only `BACK`/`NONE`; its private
color attachment never becomes a public handle or enum. Invalid routes leave
the previous routing intact. Private retirement clears still initialize the
drawing buffer when an author has disabled its color output.

The validator freezes `gl_MaxDrawBuffers` at compilation and gates the ESSL100
`GL_EXT_draw_buffers` directive by author admission. Missing outputs reject
draws unless all color channels are masked, and `gl_FragColor` does not broadcast
across targets. Duplicate color-image attachments are unsupported. Every color
slot participates in public attachment reflection and retained-resource deletion.
Window, Worker and restoration tests exercise distinct native output pixels.

The unchanged pinned Khronos fixtures pass **17/17 cases, 1,798 assertions** in
the development build, with no expected-failure exemptions. This includes the
four upstream MRT cases and all earlier texture/geometry regressions. Final
release-head measurements remain a separate verification step.

## Depth textures and framebuffer lifetime

Depth images are render-only WebGL1 textures: level zero and `TEXTURE_2D`.
Allocation uses legacy unsized depth storage, preserving both `NEAREST` and
non-comparison `LINEAR` sampling through ANGLE's WebGL1 compatibility path.
Sized GLES3 depth storage has a different completeness rule and must not replace
this public contract merely because it provides the same depth precision.

Depth images require null allocation data and the specified unsigned
depth/packed-depth-stencil types.
Subuploads, framebuffer copies and generated mipmaps are rejected. A native
shader samples values written by depth clears; packed images retain at least
24 depth bits and 8 stencil bits on the pinned WARP provider.

WebGL1's depth, stencil and combined depth/stencil slots are logical assignments,
not GLES3 aliases. Forbidden simultaneous assignments produce
`FRAMEBUFFER_UNSUPPORTED`; all reads/writes fail without changing pixels.
Removing a conflicting assignment restores the surviving logical attachment.
Storage formats must match their attachment point even when GLES3 would accept
a broader pairing.

Framebuffers retain texture/renderbuffer storage and public identity until their
last attachment is removed. Deletion detaches resources from the current
framebuffer and texture/renderbuffer bindings, but an inactive framebuffer may
retain its storage. Deleted objects cannot be rebound or newly attached. Final
detachment and framebuffer deletion release the retained native resource.

WebGL1 permits floating framebuffer values to be copied into normalized texture
images. Full and subimage copies use a bounded owned RGBA-float read,
destination-channel selection, clamping and normalized upload. Pack and
unpack alignment are restored, out-of-bounds storage is initialized, and failed
destination updates preserve the previous image. State queries on an
incomplete read surface return `INVALID_OPERATION`; an actual read still returns
`INVALID_FRAMEBUFFER_OPERATION` without changing the destination.

## Owned readback transport

The script/native boundary returns pixel data as owned bytes, using the existing
typed-array host adapter rather than serializing a JSON number for each byte.
Only the closed `readPixels` operation is admitted on this path. Context ownership,
payload limits, framebuffer/type validation, view offsets and destination guards
are unchanged. A failed read returns no bytes; a lost owner/context follows the
existing context-loss lifecycle. Worker realms use the same native owner path.

The unchanged pinned Khronos texture suite (`714857a28445`) passes all twelve
development cases and 1,352 assertions after this change in a debug build. Before
binary transport, both linear-filter cases hit the unchanged 2,000 ms script
watchdog even in release. This is execution-cost evidence, not an exemption from
the watchdog or a claim of complete WebGL conformance. The shared
`tests/webgl/texture-depth-hdr.html` fixture passes 71 assertions in Breeze and
unified-headless Chrome 154, including actual HDR readback, converted copy pixels
and interpolation of four separately rendered depth values.

## sRGB and realm registration

`EXT_sRGB` admits only unsigned-byte `SRGB_EXT`/`SRGB_ALPHA_EXT` texture images
and `SRGB8_ALPHA8_EXT` renderbuffers. ANGLE owns sRGB decoding, encoded writes,
and linear-space blending; the browser does not approximate these in shaders or
CPU pixel conversions. Alpha is not gamma converted. RGB-only sRGB images are
not color-renderable in this WebGL1 extension, even if a newer native provider
could render to its sized format. Framebuffer encoding queries remain extension
gated, and generated sRGB mipmaps remain forbidden by the WebGL1 contract.

Window and Worker include the same WebGL bootstrap block. Worker tests exercise
real HDR renderbuffers, binary destination subviews, sRGB/depth attachments,
independent capability state and shutdown. Restoration tests verify fresh
extension identities, disabled author capability admission and invalidated old
resources. The shared native-pixel fixture also passes inside the hidden Windows
AppContainer and transports the interpolated image through the renderer protocol.

With sRGB, the unchanged pinned Khronos gate expands to thirteen required cases
and 1,414 observed assertions. Its minimum assertion floor rises from 350 to
1,000; expectation overrides and unsupported-required-extension skips remain
forbidden. These are capability-specific conformance results, not a complete
WebGL2 or game-readiness claim.

## Primary contracts

- [OES_texture_float](https://registry.khronos.org/webgl/extensions/OES_texture_float/)
- [OES_texture_half_float](https://registry.khronos.org/webgl/extensions/OES_texture_half_float/)
- [OES_texture_float_linear](https://registry.khronos.org/webgl/extensions/OES_texture_float_linear/)
- [OES_texture_half_float_linear](https://registry.khronos.org/webgl/extensions/OES_texture_half_float_linear/)
- [WEBGL_color_buffer_float](https://registry.khronos.org/webgl/extensions/WEBGL_color_buffer_float/)
- [EXT_color_buffer_half_float](https://registry.khronos.org/webgl/extensions/EXT_color_buffer_half_float/)
- [EXT_texture_filter_anisotropic](https://registry.khronos.org/webgl/extensions/EXT_texture_filter_anisotropic/)
- [WEBGL_depth_texture](https://registry.khronos.org/webgl/extensions/WEBGL_depth_texture/)
- [EXT_sRGB](https://registry.khronos.org/webgl/extensions/EXT_sRGB/)
- [WebGL1 framebuffer constraints](https://registry.khronos.org/webgl/specs/latest/1.0/#6.5)
- [ANGLE explicit context version](https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_create_context_backwards_compatible.txt)
