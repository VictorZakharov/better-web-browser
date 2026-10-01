# Complete-file WebM/Opus audio

Breeze plays and decodes real audio-only WebM/Opus and records a granted
microphone into incremental WebM/Opus. This is container integration with the
existing Opus codec, not a MIME-only implementation. Complete-file support does
not imply WebM SourceBuffer, video codecs, WebRTC, DRM, or arbitrary Matroska.

## Decoder and presentation ownership

The shared `opus_audio::Stream` selects Ogg or WebM after structural admission.
Web Audio and the contained media worker use the same presentation frames and
native codec. Symphonia owns WebM packet extraction, including fixed, Xiph and
EBML lacing. Breeze's bounded metadata visitor reads timing and identification
fields that the upstream packet API does not expose; it is not a second packet
demuxer. The existing safe libopus wrapper produces actual mono/stereo float PCM.

| Input fact | Presentation behavior |
| --- | --- |
| One `A_OPUS` audio track | Other codecs, video, multiple tracks and encryption reject |
| Family-0 `OpusHead` | Mono/stereo, versions 0–15 with bounded compatible extensions |
| `CodecDelay` | Must agree with header pre-skip at 48 kHz, allowing integer-nanosecond rounding |
| Initial pre-skip | May span packets/laces; trimmed packets still feed predictive history |
| Header output gain | Applied once by libopus; Web Audio keeps unclipped float output |
| Timestamp scale and signed block offset | Convert to 48 kHz sample positions with checked arithmetic |
| Quantized adjacent timestamps | Snap within one tick; do not accumulate packet-by-packet rounding drift |
| Real timestamp gaps | Emit bounded timed silence, not concatenated compressed audio |
| Overlapping/backwards blocks | Reject rather than silently reorder predictive history |
| Positive/negative `DiscardPadding` | Remove suffix/prefix samples, including padding across laced packets |
| `SeekPreRoll` | Exact seeks replay history from the start, retaining all pre-roll |
| `FlagLacing = 0` | Reject any lacing flag, including a one-frame lace |

The Opus output clock is always 48 kHz. The admitted `SamplingFrequency` must
agree with the original-input-rate hint in `OpusHead`; it does not retune the
decoder. An explicit output frequency must be 48 kHz. This is a bounded profile,
not a claim that every conflicting legacy Matroska rate convention is accepted.
Non-neutral track scaling, deprecated track offsets and audio emphasis reject:
ignoring such transforms would produce the wrong presentation. Optional scalar
fields cannot repeat or use malformed widths. Supported neutral schema defaults
are preserved, including an empty `TrackTimestampScale` of 1.0.

Finite and unknown-sized Segments/Clusters are supported for complete input.
An unknown Cluster ends at its next valid top-level sibling or the complete
input boundary. Unknown sizes on other elements reject. Clean upstream EOF is
accepted only after envelope validation and consumption of every declared
block/lace. A demuxer EOF exception during extraction is never successful
partial audio. Empty packets reject rather than requesting packet-loss
concealment; native output duration must match the admitted packet duration.

## Integrity and resource limits

Shared WebM preflight verifies optional EBML CRC-32 for Vorbis and Opus. The CRC
must be the unique first child of its master, contain four little-endian bytes,
and match the parent's encoded data excluding that CRC element. Coverage
includes ignored metadata, Void bytes and nested child checksums. Unknown
Cluster coverage stops at the Cluster boundary, not its following sibling.
IEEE CRC uses the already-linked `crc32fast` implementation, not a new checksum
algorithm. Hashing is chunked for cancellation and does not copy the parent.
This complete-file policy rejects checksum failure; it does not implement
Matroska's optional damaged-parent recovery or partial playback policy.

- Encoded input and the retained compressed-packet ledger are each capped at
  8 MiB. Metadata work, nesting and leaf sizes retain the shared envelope caps.
- At most 16,384 packets are admitted. Each nonempty packet is at most 61,440
  bytes and 120 ms / 5,760 output frames.
- Raw predictive duration and presented duration/bytes are bounded separately.
  Trimming cannot disguise excessive codec work. The maximum admitted raw
  duration is one hour; consumer byte budgets can stop it earlier.
- Streaming playback retains compressed packets, a bounded ledger, and one
  packet-sized scratch buffer, not whole-file PCM. Timed silence is chunked.
- Web Audio keeps its 16 MiB decoded/resampled buffer cap and two owned jobs.
  Actual samples resample to the context clock; failure exposes no successful
  partial `AudioBuffer`. Cancellation retires owned jobs and cannot deliver a
  late successful result to a replacement document.
- Media playback saturates PCM only at the PCM16 device boundary. Seeking
  reconstructs decoder history and returns the exact presentation suffix.
  Decoder and seek deadlines remain per operation.

## Incremental microphone recording

`audio/webm;codecs=opus` records one granted mono/stereo microphone track.
An unspecified `audio/webm` recording MIME chooses Opus; the empty/default
request still chooses FLAC. Native Opus capture rates are 8, 12, 16, 24 and
48 kHz. A 44.1 kHz capture fails explicitly because no streaming resampler is
implemented. Decoder rate support is not evidence of encoder rate support.

Ogg and WebM share the real encoder, its queried lookahead, 20 ms packet
accumulation, bitrate target and constant/variable configuration. The WebM
serializer writes one unknown-sized Segment, a single track with the actual
`CodecDelay` and 80 ms `SeekPreRoll`, and finite bounded Clusters. Final padding
is written as `DiscardPadding`, preserving exact captured duration even for
subpacket tails. The Segment writer does not accumulate the complete recording.

Timeslice and `requestData()` drain bytes without resetting the codec/header,
timestamps, or cumulative budget. Individual Blobs need not be playable;
concatenating completed Blobs in event order produces one complete WebM file.
Empty drains do not emit repeated headers. Pause omits inactive time, disabled
capture records silence, and track end/removal seals the admitted prefix.
Format changes error; a fresh session can recover. Navigation releases native
encoders and all eight shared recorder slots.

Recorder limits remain 960 captured frames per callback, 16 MiB pending Blob
bytes, 8 MiB cumulative encoded output, and 16,384 Opus packets. Final flush is
reserved before new PCM admission. With 20 ms packets this is about 5.46 minutes,
including codec delay; encoded-byte exhaustion may end it earlier. A failed
writer cannot resume, and drain checks cumulative size before consuming bytes.
No camera/video recording or live network-stream parser is added.

## Open-source integration and audit boundary

No codec or Matroska writer is copied into the repository. The pinned
`webm-iterable` 0.7.1 serializer and its `ebml-iterable` 0.7.1 / specification
0.4.0 / derive 0.4.0 dependencies use MIT. Their published checksums are locked.
Existing Symphonia 0.6.1 demuxing (MPL-2.0), safe `opus` 0.4.0
(MIT OR Apache-2.0), bundled libopus 1.6.1 (BSD-3-Clause), and `crc32fast` 1.5.0
(MIT OR Apache-2.0) are reused. [Generated notices](../THIRD_PARTY_NOTICES.md)
describe the complete linked Windows graph and packaged license files.

Review covered package manifests, writer buffering/flush behavior, EBML numeric
serialization and static derive code generation. The added WebM/EBML packages
have no package build script. The derive crate generates enum/trait code from
static schema tokens; it does not fetch schema at build time. Upstream's ignored
network-based schema-generation helper is confined to its own `cfg(test)`
module and dev dependencies, not Breeze's build/runtime. The writer receives
bounded browser-generated tags, never arbitrary author-provided tag trees.
Published documentation and code were treated as untrusted reference material,
not instructions. This review narrows exposure; it does not prove absence of
upstream defects. Existing media containment and native build policy remain.

## Regression evidence

The existing independently FFmpeg-encoded WebM tone now exercises playback,
seeking and Web Audio resampling. Owned Ogg mono/stereo packets are independently
remuxed into test-only WebM envelopes, avoiding a writer/reader agreement as the
only oracle. Tests compare actual complete PCM, not only reported duration.

Coverage includes every finite byte truncation, unknown-sized master boundaries,
all lacing modes, mixed 2.5–60 ms native packets, a real repacketized 120 ms
packet, cross-packet delay/padding, gain, signed timestamps, gaps, ordering,
contradictory metadata, CRC coverage/order/damage, cancellation, deadlines and
separate source/PCM budgets. Recorder tests decode actual emitted chunks and
compare Ogg/WebM predictive PCM, native-rate tails, stereo, silence, pause,
track changes, restart and document retirement. Hidden renderer tests cover
ordinary playback, decode and browser-owned capture routes. Fixture provenance
is in [the media inventory](../tests/fixtures/media/README.md).

HTML5test is a feature inventory, not proof of these contracts. Its measured
before/after belongs in the README and PR; no score gain is inferred from tests.

## Primary references

- [WebM container guidelines](https://www.webmproject.org/docs/container/)
- [Matroska A_OPUS codec mapping](https://www.matroska.org/technical/codec_specs.html)
- [Matroska elements and timing](https://www.matroska.org/technical/elements.html)
- [Matroska, RFC 9559](https://www.rfc-editor.org/rfc/rfc9559.html)
- [EBML CRC-32, RFC 8794 §11.3.1](https://www.rfc-editor.org/rfc/rfc8794.html#section-11.3.1)
- [Pinned WebM writer](https://docs.rs/webm-iterable/0.7.1/webm_iterable/)
- [MediaStream Recording](https://www.w3.org/TR/mediastream-recording/)
