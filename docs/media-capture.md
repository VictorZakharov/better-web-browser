# Camera and microphone capture

Breeze's `getUserMedia()` path is owned by the browser process. The renderer sends a document-
scoped request, not a device handle or an origin string. The browser resolves the registered
top-level client and checks the current document, renderer session, secure origin, selected tab,
visible foreground window, and user decision before it starts a contained capture process.
Child-frame capture is denied until a Permissions Policy path exists. Hidden benchmarks cannot
open devices or show permission UI.

Each Start receives a fresh camera/microphone decision; a Yes never authorizes a later request.
A No is remembered only for the same origin and device kind during this BrowserApplication
session, so a page cannot repeatedly open a modal prompt. No capture decision is written to the
profile. The permission dialog's owned window is not mistaken for external focus loss, but
navigation, external focus loss, tab retirement, hiding, minimization, and renderer exit revoke
the grant. All native startup, sample reads, and process destruction run off the UI thread.

Camera and microphone use separate AppContainer capture children even when a request asks for
both. This allows `MediaStreamTrack.stop()` to stop only its corresponding device; when the last
track stops, the stream ends. A pending request that loses its document or foreground status is
canceled; if native startup has already begun, the capture Job is terminated promptly rather
than waiting for the device-start timeout. The browser maps each native sample back to the active
document/request only after rechecking ownership and grant state. Video uses bounded NV12
samples with latest-frame replacement; microphone PCM16 remains ordered with a bounded queue.
An overloaded renderer cannot silently lose a required control update, and a slow renderer
cannot make the browser UI wait for hardware.

Granted camera frames can be attached to a `<video>` through `srcObject`; unchanged-size
frames use the direct video presentation path. A microphone track feeds
`AudioContext.createMediaStreamSource(stream)` through a bounded PCM ring and the existing Web
Audio graph. It does not play through speakers merely because capture began. Disabled or stopped
tracks clear queued speech, and a source remains bound to its selected track if the track is
later removed from the stream. The HTML `<audio srcObject>` monitoring path is not implemented.

Current scope deliberately does not claim advanced constraint selection, device picking, or
capture from child frames. Unsupported optional `advanced` preference sets are skipped, while
unsupported mandatory basic constraints reject before prompting. Add selection behind the same
browser-authoritative admission and
capability checks, not by exposing the private capture broker to page script. The security and
track lifecycle decisions follow the [Media Capture and Streams specification](https://w3c.github.io/mediacapture-main/).

The fake-provider unit tests cover one-shot grants and denial, wrong client/document/session,
stale generations, per-track Stop, tab retirement, mailbox backpressure, and revocation while
native startup is blocked. They never activate real camera or microphone hardware.
Renderer tests use synthetic NV12 and PCM frames rather than physical devices.
