# WebCodecs audio

This slice implements immutable audio resources and real asynchronous Opus/PCM
processing in Window and dedicated Worker realms. It is not an audio player,
container demuxer, device capture implementation, or a claim to support every codec.

## Standards and provenance

The compatibility contracts follow the [WebCodecs working draft](https://www.w3.org/TR/webcodecs/),
the [Opus registration](https://www.w3.org/TR/webcodecs-opus-codec-registration/),
and the [PCM registration](https://www.w3.org/TR/webcodecs-pcm-codec-registration/).
The implementation was checked against the September 21, 2026 WebCodecs draft
and the June 8, 2026 PCM registration. Important implementation limits are below.

The native codec uses the repository's existing locked `opus` 0.4.0 dependency
and libopus, including its decoder, encoder controls and repacketizer. No codec
implementation, third-party JavaScript, new dependency or generated sample asset
was copied into this batch. The Rust wrapper is MIT/Apache-2.0 and native libopus
is BSD-3-Clause; their existing notices continue to apply. OpusHead parsing reuses
the existing WebM/Opus parser rather than introducing another header parser.

## Resource contracts

- `AudioData` owns an immutable byte snapshot, accepts the eight registered
  interleaved/planar sample layouts, and converts them through `copyTo`.
- Copy options operate on frames and planes, not byte offsets. Destination views
  respect their own offset and length; same-kind rearrangement preserves sample
  bits, including float NaN payloads and negative zero.
- Integer narrowing uses arithmetic-shift precision. Floating-point conversion
  clamps integer output, maps NaN to silence, and saturates infinities.
- `clone()` retains samples independently. `close()` is idempotent and releases
  the resource while preserving its timestamp. Copying closed data fails.
- `EncodedAudioChunk` snapshots only the supplied view and preserves type,
  timestamp and nullable duration. Chunk copying does not decode the packet.
- Live audio data is serializable and transferable; chunks are serializable but
  not transferable. Closed data cannot be serialized. Audio data is not allowed
  in persistent storage; chunks are, through the existing storage serializer.
- Window/MessagePort native cloning and Worker cloning retain private sample
  state and receiving-realm constructors. A failed graph or transfer validation
  does not detach other accepted buffers. Author getters, constructors and
  `close` replacements are not serialization hooks.

## Codec admission

| Path | Supported configurations |
|---|---|
| Opus encoder/decoder | 8, 12, 16, 24 or 48 kHz; mono or stereo |
| Opus encoder bitrate | Omitted/automatic, or 6,000–510,000 bits/s |
| Opus packet duration | 2.5–120 ms in 2.5 ms increments |
| Raw PCM decoder | `pcm-u8`, `pcm-s16`, `pcm-s24`, `pcm-s32`, `pcm-f32` |
| PCM dimensions | 1–384,000 Hz; 1–32 channels; complete interleaved frames |

PCM is decoder-only. Twenty-four-bit PCM expands to signed 32-bit samples with
eight low zero bits; it is not rounded through float. No AAC, MP3, FLAC or video
codec support is advertised by these classes. Unsupported configurations resolve
`supported: false`; configuring them produces an asynchronous `NotSupportedError`.
Invalid dictionaries still fail Web IDL conversion and validation.

Opus encoder options support `application`, `signal`, `complexity`, `packetlossperc`,
FEC, DTX, bitrate mode and packet duration. Durations not directly accepted by a
single libopus call combine homogeneous 2.5 ms frames with libopus's repacketizer.
The packet is verified by actual decode tests at every admitted rate and duration.

Default `opus.format: "opus"` emits raw packets without a description. The
registered `"ogg"` format emits **raw packets plus an OpusHead description**, not
Ogg container pages. The header records encoder pre-skip, input rate and channels.
Decoding honors channel validation, scaled pre-skip and header output gain.

Flush emits pending PCM and encoder history before its promise resolves. Raw Opus
packets have no end-of-stream trim field: final padding remains observable, and
decoded sample count is not presented as exactly equal to the original input.
Packet timestamps account for encoder lookahead. Reconfiguration drains old
accepted PCM under its old metadata before replacing the native encoder.

## Scheduling and bounds

Codec constructors require a secure context, including the existing ancestor
trust checks. Audio resources remain available independently of secure context.
Dictionary and input snapshots happen synchronously; native codec work, outputs,
support-query resolution and dequeue notifications happen on later tasks.

The browser thread never waits for a codec job. Each codec has one outstanding
native command and bounded command/result channels. Reset and close invalidate
queued generations, abort flush promises and suppress stale output. Callback
exceptions are reported as script errors without turning into codec failures.
Document/Worker teardown cancels native sessions; worker lifetime permits are
released on actual thread exit, so repeated close/reopen cannot evade limits.

| Resource | Current limit |
|---|---:|
| `AudioData` snapshot | 16 MiB |
| Encoded chunk snapshot | 8 MiB |
| One native PCM input | 512 KiB |
| JavaScript queued input | 8 MiB / 64 commands |
| Codec workers per realm / process | 8 / 64 |
| Native message graph, including platform snapshots | 16 MiB |

Native timestamp input currently requires exact safe JavaScript integers. These
are explicit resource/admission limits, not claims that the full WebCodecs
configuration space is implemented. There is no resampling, channel remapping,
hardware acceleration, audio device output, encrypted decoding or packet-loss
concealment substitution for malformed packets.

## Verification

The project-owned fixture uses synthetic in-memory tones and sample arrays. Its
assertions cover immutable data, layout/conversion, clone/transfer, actual decoded
energy, Opus metadata, reset, and exact 24-bit PCM. Run it without visible UI:

```powershell
./scripts/test-audio-codecs.ps1 -Browser ./target/release/better-web-browser.exe `
  -OutputDirectory target/audio-proof -ExpectedPassed 17
./scripts/test-audio-codecs.ps1 -Chrome -End 16 `
  -OutputDirectory target/audio-chrome-proof
```

The Chromium harness must already be built. The wrapper uses the existing
unified-headless reference launch and hidden Breeze automation guard. `-Begin`
and `-End` isolate a probe; they do not change the default full fixture. Reports
record each failure rather than treating constructor presence as a pass.

The pinned upstream WPT subset adds AudioData construction, AudioData copying
and EncodedAudioChunk tests. Original tests also cover dictionary getter order,
private branding, native resource bounds, all packet durations, worker teardown,
codec callback ordering and AppContainer execution.

### Known reference differences

Chromium 154's tested build rejected the explicit registered `"ogg"` encoder
format. Its signed 24-bit PCM fixture did not settle within the reference harness
timeout, so the timed-out case is isolated rather than counted as a pass. Integer
narrowing comparisons allow one destination LSB for Chromium's float conversion;
Breeze's own precision tests retain exact arithmetic expectations.

The tested Chromium constructor also rejected a zero sample rate with
`NotSupportedError`, whereas the current draft requires `TypeError` for an invalid
AudioData initializer. The fixture retains the draft's expectation rather than
silently accepting either exception. Its transfer safety assertion is separate
from that exception-name check.

The pinned older encoder-config WPT expects some unsupported bitrates to throw
as invalid. The current draft's generic validity algorithm instead validates
codec/rate/channels and registered extensions, leaving those bitrate values to
support checking. This implementation follows that current distinction and does
not curate the conflicting older test as passing.
