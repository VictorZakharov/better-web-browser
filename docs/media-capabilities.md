# MediaCapabilities queries

`navigator.mediaCapabilities` is a same-object instance of the exposed
`MediaCapabilities` interface in Window and dedicated Worker realms. The
interface has an illegal constructor and branded `decodingInfo()` and
`encodingInfo()` methods. Pure queries do not initialize microphones, native
encoders, output devices or media playback. Worker exposure does not add
`getUserMedia` or `MediaRecorder` to Workers.

## Conversion is separate from support

Each method returns a new Promise. Web IDL conversion snapshots inherited
`MediaConfiguration` members before derived members, recursively converting
each dictionary when its getter is observed. Numeric, enum, string and iterable
conversion failures reject; they are not unsupported results. Returned
configuration objects are independent snapshots, not aliases of author input
or other query results. Encoding ignores `keySystemConfiguration` entirely,
including an otherwise throwing getter, because that member is not in the
encoding dictionary.

A configuration requires audio or video. MIME validation distinguishes invalid
configurations from valid but unsupported ones. Audio/video MIME kind and codec
parameter validity are checked separately from the real decoder/encoder matrix.
Optional members preserve applicability rules: spatial rendering and HDR/color
members apply to file/source decoding; scalability mode applies to WebRTC
encoding, not file decoding. Unsupported valid WebRTC, video recording or
encrypted decoding resolves without claiming support.

Successful queries settle through owned media tasks after conversion, not
synchronously in the calling task. Window and Worker share the same pure native
support functions. Worker task handles are not author timers: guessing an ID and
calling `clearTimeout()` cannot cancel a capability Promise. Worker termination
retires owned work rather than running callbacks in a dead realm.

## Supported queries

| Query | Supported scope |
| --- | --- |
| File audio decoding | Existing bounded complete-file decoder matrix, including audio-only WebM/Opus |
| File/MSE video decoding | Existing contained video/parser matrix and finite output budgets |
| `record` encoding | One mono/stereo audio configuration, FLAC or Ogg/WebM Opus |
| `webrtc` encoding/decoding | Valid configurations resolve `supported: false` |

Explicit file Opus rates describe its 48 kHz output, independent of capture
rates. Explicit recording rates must match real capture admission: Opus uses
8/12/16/24/48 kHz; FLAC uses 8–48 kHz. Numeric mono/stereo counts are admitted,
not inferred channel-layout names. Omitting optional rate/channel/bitrate
members lets the implemented encoder choose its supported format; it does not
promise every possible capture format. Opus bitrate queries stay within the
500–512,000 bit/s native target range. FLAC accepts positive lossless bitrate
hints without claiming fixed achieved bitrate. Decoder availability never
implies an encoder.

All results keep `smooth` and `powerEfficient` false. No delivery/performance or
hardware-power measurement supports a positive claim. Encoding results contain
no `keySystemAccess`; decoding results retain `keySystemAccess: null`. Encrypted
queries in Workers reject with `InvalidStateError`; insecure Window encrypted
queries reject with `SecurityError`. Secure Window queries still do not claim
DRM support. Source/creator trust is browser-owned, not inferred from an author
mutation of `location` or a fake `document` global.

Tests cover conversion/getter order, inherited members, exceptions, unsigned
numeric conversion, dictionary snapshots, receiver brands and descriptors,
task/microtask ordering, timer cancellation isolation, Window/Worker parity,
encrypted-context errors, MIME validity, and actual capture-rate boundaries.
The [WebM/Opus contract](webm-opus.md) documents the real media path behind the
new positive MIME results. Queries are not full conformance certification.

Primary references: [Media Capabilities](https://w3c.github.io/media-capabilities/),
[Web IDL dictionary conversion](https://webidl.spec.whatwg.org/#es-dictionary),
and [MediaStream Recording](https://www.w3.org/TR/mediastream-recording/).
