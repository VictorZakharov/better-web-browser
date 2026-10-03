# WebGL 1 lifecycle and extension contract

This slice extends the Windows ANGLE/WARP backend. It does not introduce another
renderer, download a native library, implement a replacement shader language, or
advertise WebGL 2. Canvas and worker OffscreenCanvas use the same native owner.

## Supported extensions

| Extension | Actual implementation | Important boundary |
|---|---|---|
| `WEBGL_lose_context` | Retires and recreates real driver contexts | Restoration needs a canceled loss event |
| `ANGLE_instanced_arrays` | Native instanced array/index draws and divisors | Bounds include every instance |
| `OES_vertex_array_object` | Native vertex-array objects plus an ownership mirror | Element bindings, attribute buffers and divisors are per VAO |
| `OES_element_index_uint` | Native unsigned 32-bit element indices | Disabled until requested; full-width index validation |
| `OES_standard_derivatives` | ANGLE GLSL derivatives and hint state | Both context extension and shader directive are required |
| `EXT_frag_depth` | Fragment-depth writes feed native depth testing | Both context extension and shader directive are required |
| `EXT_shader_texture_lod` | Native 2D/cube mip, projection and gradient sampling | Complete upstream conformance remains a known gap |

The native whitelist checks ANGLE's enabled/requestable capabilities and required
entry points. Merely having an extension name in a driver string is insufficient
for an extension that requires callable functions. Unsupported names return null.
Requesting an extension enables it only in that context. Restoration resets this
enabled state, even when the backend's availability is unchanged.

Extension objects have private brands and no public constructors. Their identity
is stable within an epoch. After restoration, geometry extensions receive new
objects; methods on old objects cannot mutate or draw with the new driver state.
The loss extension itself remains usable to request restoration.

## Context lifecycle

`loseContext()` immediately changes observable loss state and retires the native
context. The trusted, cancelable `webglcontextlost` event is delivered as a browser
task, not synchronously and not as a promise job. During loss:

- Drawing-buffer dimensions are zero.
- Object-creation and ordinary parameter queries return their lost-context values.
- `getError()` reports `CONTEXT_LOST_WEBGL` once for that loss.
- Argument/receiver conversion still runs before the operation's lost-context no-op.
- Readback must not modify an author destination.
- Old resources retain their JavaScript identity but cannot alias new native names.

The page must cancel the loss event before restoration is allowed. A simulated
loss additionally requires an explicit restoration request. Requests made before
the loss event completes fail; duplicate queued restoration requests do not create
multiple native contexts. Browser tasks use a private scheduler and cannot be
canceled or redirected by replacing author `setTimeout`/`clearTimeout` functions.

Restoration creates a fresh native context at the canvas's current size with its
original granted settings. It initializes the drawing buffer, program/buffer
bindings, masks, unpack flags, extension state and error state. The original
JavaScript context is retained. The restoration event is trusted and cancelable,
matching the checked Chrome behavior. Calling `getContext` on that canvas returns
the same object without rereading a new options dictionary.

If native admission fails, the context stays lost: there is no partially usable
context and no success event. Releasing a peer and retrying can subsequently
restore it. The realm/owner resource limits remain enforced during this process.

Creation options use Web IDL dictionary conversion: each known member is obtained
once in lexicographic order, inherited properties are allowed, enum conversion can
throw, and unknown properties are not read. Conversion errors do not masquerade as
driver failures. `failIfMajorPerformanceCaveat` rejects the software backend with
a trusted, cancelable creation-error event and an actionable status message.
Antialiasing and desynchronized presentation are not reported as granted merely
because the caller requested them. `powerPreference` is an accepted hint, not a
claim that WARP changed into an accelerated adapter.

## Native drawing and resource lifetime

VAOs own attribute formats, enable bits, array-buffer references, divisors and
their element-buffer binding. Generic current attribute values and `ARRAY_BUFFER`
binding remain context state. Switching VAOs restores the corresponding native
state rather than changing only query results.

Deleting a buffer detaches its current bindings but does not prematurely destroy
storage retained by an inactive VAO. That storage is released after its last
reference. A deleted public handle cannot be rebound. Deleting the current VAO
returns to the default VAO; deleting a peer's object is an invalid operation.

Instanced drawing checks vertex ranges and divisor-derived instance ranges before
native execution. Index validation reads complete unsigned values, checks type
enablement and alignment, and avoids signed truncation. Enabled attributes with a
null buffer reject drawing, even when the linked program does not consume them.
Zero-offset null attribute-pointer setup itself is allowed, matching WebGL.
The existing draw-work limit covers vertex count multiplied by instance count.
The Chrome fixture tests misaligned index offsets rather than requiring an error
for out-of-range vertices: native robust-access paths may permit the latter.

Extension draws mark the canvas dirty through its normal presentation path.
They therefore produce real browser paint pixels, not only successful `readPixels`
results. Private clearing after presentation preserves the author's framebuffer,
clear values, scissor and unsigned stencil masks. Resizing similarly reinitializes
the buffer without resetting unrelated author state.

## Shader compiler reuse and public reflection

The existing pinned `mozangle` dependency supplies its explicit WebGL shader
validator. The browser configures it with real native capability limits and the
extensions enabled in the owning context. ANGLE parses, validates and translates
the shader; the browser does not substitute a site-specific GLSL parser.

The validator rejects non-WebGL languages, invalid loop bounds and unavailable
extension constructs. Translation legally relocates extension directives for the
native compiler. Original author source remains available through `getShaderSource`.
Failed validation also invalidates the real driver shader, preventing a later link
from reusing a previous successful compile. Compiler diagnostics are UTF-8-safe
and bounded, and source/translation/log storage is charged to resource admission.

ANGLE's pinned ESSL output prefixes user identifiers. Attribute binding, uniform
lookup and active-variable queries translate those names at the native boundary,
including structure members and array indices. Author names beginning with `_u`
round-trip without being shortened. WebGL 1's 256-character token/location limits
are retained; internal encoded names are not subjected to the author-facing cap.

Uniform locations keep the owning program and link generation. Native active-
uniform metadata determines query type and component count, rather than author
IPC fields. A fixed sixteen-component destination covers all WebGL 1 types.
Each location caches its real type when resolved, so repeated `getUniform` calls
do not rescan every active uniform in the linked program.

Web IDL numeric conversion handles signed/unsigned wrapping, float rounding,
DOMString errors, argument order and ignored extra arguments. Sequence conversion
obtains and invokes an iterator once, and closes it on conversion/budget failure.
Nonfinite floats and negative zero have a closed private transport encoding;
JSON serialization must not silently replace them with null or positive zero.
Native command parsing still rejects unknown fields and unrecognized encodings.

## Diagnostics are not protocol frames

ANGLE and its native shader compiler can write warnings to C stdout/stderr. Those
bytes cannot share the framed renderer protocol. The child retains a dedicated
protocol handle, redirects native output to a separate inherited diagnostics pipe,
and the broker drains that pipe until shutdown. Forwarded output is bounded;
excess diagnostics are discarded without blocking the native owner. A failing
parent stderr sink likewise does not stop draining.

The inheritance allowlist retains only the intended IPC, media and diagnostics
handles. No parser resynchronization, warning suppression or permissive frame
magic was introduced. Integration tests emit actual native stdout/stderr and then
require a successful protocol ping and orderly renderer shutdown.

## Verification and upstream provenance

The curated manifest uses the official MIT-licensed
[`KhronosGroup/WebGL`](https://github.com/KhronosGroup/WebGL) suite, pinned at
`714857a28445e8f5d8d6ae1c78498578009534d8`. Tests are checked out externally;
unmodified upstream fixture bytes, license and revision are verified before use.
The reporting adapter only forwards upstream assertions. It additionally requires
the selected native capability so an optional-extension skip cannot become a pass.
It preserves failures and precondition failures, transports Unicode safely and
rejects ambiguous completion. It does not replace shaders or tested APIs.

```powershell
./scripts/checkout-wpt.ps1 -Destination G:/Git/breeze-webgl-conformance `
    -Manifest tests/webgl/khronos-manifest.json
./scripts/run-wpt.ps1 -WptRoot G:/Git/breeze-webgl-conformance `
    -Manifest tests/webgl/khronos-manifest.json -BuildProfile debug
```

The main CI WPT worker also runs this curated suite with no failure expectations
and a minimum assertion count. PR renderer smoke coverage includes native output
isolation and the shared lifecycle/instancing fixture. All browser execution stays
hidden. The shared `tests/webgl/lifecycle-instancing.html` is also suitable for a
unified-headless Chrome comparison of the same geometry and lifecycle contracts.

Known gaps are explicit in `tests/webgl/exploratory-manifest.json`, outside the
curated pass gate. Two upstream index/bounds tests need synchronous XMLHttpRequest,
which Breeze does not implement; their helper is not rewritten. The full upstream
texture-LOD case exceeds the unchanged two-second script task watchdog in debug
builds. Direct native tests cover 2D/cube mip selection, projection, gradients,
depth ordering, extension gating and invalid ranges, but do not erase those gaps.
The final release build completes that upstream texture-LOD case in one local
run (reported as an unexpected pass against the debug-gap manifest). It is not
promoted to the required gate while the debug build cannot complete it within
the unchanged watchdog. The two synchronous-XHR cases still fail in release.

Antialiasing, accelerated adapters, wider texture extensions, WebGL 2, WebGPU and
full Khronos certification remain open. No HTML5test-specific capability or score
behavior is added. Benchmark evidence belongs in the README/PR and must distinguish
first presentation, settled score, native rendering contracts and process memory.

## Standards references

- [WebGL 1 API and context lifecycle](https://registry.khronos.org/webgl/specs/latest/1.0/)
- [Web IDL conversion](https://webidl.spec.whatwg.org/)
- [ANGLE instancing](https://registry.khronos.org/webgl/extensions/ANGLE_instanced_arrays/)
- [Vertex-array objects](https://registry.khronos.org/webgl/extensions/OES_vertex_array_object/)
- [Fragment depth](https://registry.khronos.org/webgl/extensions/EXT_frag_depth/)
- [Explicit texture LOD](https://registry.khronos.org/webgl/extensions/EXT_shader_texture_lod/)
