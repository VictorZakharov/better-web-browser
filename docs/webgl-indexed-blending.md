# Indexed WebGL2 blending

`OES_draw_buffers_indexed` reuses the existing pinned ANGLE GLES backend. It is
available only for WebGL2, only when ANGLE advertises the native OES extension and
all seven drawing-state entry points plus the two typed query functions resolve.
Native availability is not author admission: indexed state methods and the seven
additional `getIndexedParameter` targets require `getExtension` first.

The [Khronos revision 6 contract](https://registry.khronos.org/webgl/extensions/OES_draw_buffers_indexed/)
is shared by Window and OffscreenCanvas workers. Methods have Web IDL receiver,
arity, ordered unsigned-long/boolean conversion and descriptor behavior. There
is no public interface constructor and no native-only `isEnablediOES` method.
Retained extension objects convert arguments after loss but cannot change the
new context after restoration. Query arrays are independent snapshots.

Blend enablement, equations, factors and color masks are per output. Ordinary
WebGL state setters broadcast to all outputs; ANGLE remains the state owner
rather than maintaining a browser shadow that can diverge from global setters.
The WebGL-compatible native context validates equations, factor restrictions,
and mixed constant-color/constant-alpha factors across enabled outputs before
drawing. Tests verify failed updates and failed draws leave pixels unchanged.

Breeze's private drawing-buffer retirement uses indexed color-mask output 0
when the extension is admitted. A global temporary `colorMask` would overwrite
every other output's author state. Regression tests preserve distinct masks
across all admitted draw buffers while clearing the private surface.

`tests/webgl/indexed-blend.html` is a standalone reference fixture. Its shared
script also runs as unit tests in Window and Worker, compiling real shaders,
creating two complete RGBA8 attachments and reading each output. It checks
independent blended pixels, independent channel masks, global broadcasts,
closed capability/index validation and atomic constant-factor rejection.
No dependency, copied provider code, native resource limit or watchdog changed.

A fresh hidden reference run produced identical blended `[127, 0, 128, 127]`
and masked `[0, 255, 255, 255]` pixels in Breeze and Chrome, and both completed
the shared invalid-draw/global-broadcast checks. Chrome's extension brand is
`[object OESDrawBuffersIndexed]`; Breeze follows the IDL interface name
`[object OES_draw_buffers_indexed]`. The fixture reports that difference rather
than treating it as a pixel failure; Breeze's interface-brand test stays explicit.
