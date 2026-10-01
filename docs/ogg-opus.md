# Complete Ogg/Opus audio

Breeze decodes real Opus PCM for Web Audio and contained audio playback.
`audio/ogg;codecs=opus` describes this path, not WebM, RTP, MediaSource, DRM,
or every channel mapping. Inventory scores are measured separately: exposing
a MIME type is not evidence of a working codec.

## Shared presentation contract

The admitted shape is one complete Ogg logical stream containing a mapping
family 0 mono/stereo `OpusHead`, bounded `OpusTags`, and valid Opus packets.
Physical framing, CRCs, consecutive page sequence, a single stream serial,
BOS/header placement, continuation, complete EOS, and absence of trailing or
chained data are checked before predictive decoding. The existing `ogg`
reader assembles packets; admission does not replace its demuxer.

Granules and output use 48,000 samples per second per channel. The header's
original input rate is metadata, not the output rate. Initial nonzero granule
origins are normalized. Pre-skip can cross packets; native decoding still
processes that predictive history. Only samples after pre-skip and before EOS
trimming enter the `AudioBuffer` or playback queue. Header gain is applied once
by libopus. Non-finite PCM or disagreement between admitted and decoded packet
duration is a terminal error, not successful partial decoding.

Compatible minor header versions through 15 may contain bounded extensions;
newer incompatible versions and other channel mappings reject. Unknown bounded
comments are not interpreted as images or decoder commands. Empty packets
reject: native empty input requests packet-loss concealment, not complete-file
decoding. Valid nonempty packets containing empty codec frames are distinct.

## Budgets and lifecycle

- Encoded sources remain at most 8 MiB. Header/comment packets are at most
  256 KiB; audio packets at most 61,440 bytes and 120 ms (5,760 frames).
  Admission limits audio packets to 16,384 and raw duration to one hour.
  Raw and presentation PCM byte counts are checked independently: trimming
  cannot hide an oversized predictive workload.
- The shared stream retains encoded input, a bounded duration ledger, and a
  5,760-frame mono/stereo float scratch buffer, not a whole-file PCM buffer.
- Web Audio retains its 16 MiB decoded/resampled cap and two document-owned
  asynchronous jobs. Actual planar samples are resampled to the context rate;
  detachment, callbacks, promises, and cancellation use the existing lifecycle.
- Media playback saturates finite float PCM to PCM16 at the output boundary;
  Web Audio retains unclipped float samples. Existing cumulative decoded-source
  and queue budgets remain. Streaming may exceed an `AudioBuffer` budget
  without retaining all decoded samples.
- Seek recreates the predictive decoder and discards exact presentation frames.
  Repeated paused seeks return the same sample suffix. This favors exactness
  over fast random access. Decode/seek deadlines are per operation: a long
  pause does not expire the decoder's lifetime.
- Cancelled/failed streams cannot resume. Reopening immutable input creates
  a fresh decoder and repeats admission.

File capability queries admit 48 kHz numeric mono/stereo, not inferred channel
layout names or spatial decoding. `smooth` and `powerEfficient` remain false:
this support matrix does not measure delivery or device efficiency.

## Incremental microphone recording

One browser-granted mono/stereo microphone track can be recorded as
`audio/ogg;codecs=opus`. An unspecified `audio/ogg` recording request chooses
Opus; the empty/default recording request still chooses FLAC. Playback support
is not reused as evidence of an encoder: recording has its own MIME admission.

The safe libopus encoder accepts native 8, 12, 16, 24, or 48 kHz capture only.
It accumulates 20 ms frames, queries the actual encoder lookahead, converts that
delay to the mapping header's 48 kHz pre-skip, and writes one Ogg logical stream.
Stop drains predictive history with zero padding; the EOS granule trims it to
the exact captured duration. Subpacket tails are retained, not discarded or
reported as full padded frames. Recording does not implement a streaming
resampler: 44.1 kHz capture produces `NotSupportedError`, rather than mislabeled
samples. Starting a fresh recording can recover after an unsupported format.

Timeslice and `requestData()` drain encoded page bytes without resetting the
codec or the cumulative budget. Individual Blobs need not be playable;
concatenating the completed recording's Blobs in event order forms one complete
stream. Pause omits captured duration, disabled tracks record silence, and gaps
in the capture sequence preserve the existing bounded silence policy. Format
and track changes terminate with actionable errors. Navigation retires all
document-owned native encoders, including all eight active session slots.

The requested bitrate hint is reflected without changing WebIDL conversion;
the bounded native encoder clamps its target to 500–512,000 bit/s. A hint is
not a promise of the achieved rate. Constant/variable mode configures the real
encoder. FLAC remains lossless PCM16 and does not claim a constant bitrate mode. Opus recording
retains the existing 960-frame capture packet, eight-session, and 16 MiB pending
Blob bounds; complete encoded output is additionally capped at 8 MiB and
16,384 packets. With 20 ms packets, the packet cap limits a recording to about
5.46 minutes, with encoder delay included; byte limits can end it earlier.
Admission reserves the final flush before accepting new PCM. Exhaustion can
seal the already-admitted prefix with EOS instead of handing out truncated
output. A native encoding failure is terminal, not successful partial audio.

## Dependency provenance and native policy

Pinned `opus` 0.4.0 is MIT OR Apache-2.0. `opusic-sys` 0.7.5 builds bundled
libopus 1.6.1 (BSD-3-Clause); `cmake` 0.1.58 is MIT OR Apache-2.0. Published
checksums are locked. [Third-party notices](../THIRD_PARTY_NOTICES.md) record
the complete Windows graph. No codec implementation or handwritten FFI is
copied into Breeze; no system codec DLL is downloaded.

Bundled native files were compared to the official libopus 1.6.1 release. The
crate's small CMake patch changes MSVC runtime selection and an optional switch,
not the decoder. Production codec execution remains in contained document
audio jobs or the restricted media worker. Containment is additional defense,
not proof that native code cannot contain defects.

Windows policy verifies the actual Cargo-selected compiler configuration, not
just requested CMake options: stack protection, runtime hardening, thread-safe
stack allocation, and runtime CPU dispatch must remain enabled. A global
non-thread-safe pseudostack or globally presumed AVX2 is forbidden; separately
compiled runtime-dispatched SIMD routines are permitted. Independent document
jobs can execute concurrently, so a per-decoder lock would not protect a
native global pseudostack.

The published `opusic-sys/LICENSE` is the complete native `opus/COPYING` notice.
Packaging checks byte equality and retains it beside both wrapper licenses.
The notices preserve upstream terms; they do not relicense third-party code.

## Regression contract and remaining scope

Owned mono and distinct-channel stereo tones test actual output, exact
19,200-frame presentation, gain, resampling, and full seek-suffix equivalence.
CRC-valid remuxes exercise page grouping, nonzero origins, cross-packet pre-skip,
and EOS trimming. Negative tests cover every byte truncation, CRC damage,
chaining/trailing data, malformed headers/packets, budgets, cancellation, and
deadlines. Consumer and hidden renderer tests cover playback/source fallback,
format metadata, and document retirement. Fixture hashes and generation
commands are in [the media inventory](../tests/fixtures/media/README.md).

The companion [WebM/Opus path](webm-opus.md) now implements its separate
`CodecDelay`, signed timestamp and `DiscardPadding` presentation semantics.
Complete Ogg/WebM decoding does not imply SourceBuffer, WebRTC, DRM, or
hardware-efficiency support.

## Primary references

- [RFC 7845: Ogg Opus](https://www.rfc-editor.org/rfc/rfc7845.html)
- [Official libopus decoder API](https://opus-codec.org/docs/opus_api-1.6/group__opus__decoder.html)
- [Official libopus encoder API](https://opus-codec.org/docs/opus_api-1.6/group__opus__encoder.html)
- [Official libopus 1.6.1 release](https://github.com/xiph/opus/releases/tag/v1.6.1)
- [Pinned safe wrapper API](https://docs.rs/opus/0.4.0/opus/struct.Decoder.html)
- [Web Audio decodeAudioData](https://www.w3.org/TR/webaudio/#dom-baseaudiocontext-decodeaudiodata)
- [MediaStream Recording](https://www.w3.org/TR/mediastream-recording/)
