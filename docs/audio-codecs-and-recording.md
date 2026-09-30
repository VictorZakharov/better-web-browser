# Encoded audio and recording contracts

Breeze decodes and records actual audio samples. MIME and constructor exposure
alone are not used as evidence that a format works. The page-facing APIs follow
the [Web Audio](https://www.w3.org/TR/webaudio-1.0/) and
[MediaStream Recording](https://www.w3.org/TR/mediastream-recording/)
models; FLAC framing follows [RFC 9639](https://www.rfc-editor.org/rfc/rfc9639).

## `decodeAudioData`

`BaseAudioContext.decodeAudioData` accepts complete, bounded encoded files:
PCM/IEEE-float RIFF/WAVE, native FLAC, Ogg/Vorbis, MP3, AAC-in-M4A,
ADTS AAC-LC, audio-only WebM/Vorbis, and Ogg/FLAC. A
document-owned native worker decodes every admitted sample to planar `Float32`
PCM, linearly resamples to the context sample rate, and resolves an
`AudioBuffer` through the media task queue. It rejects malformed or unsupported
formats with `EncodingError`; the caller's `ArrayBuffer` is detached before the
method returns. Navigation cancels document-owned decode jobs.

The encoded input cap is 8 MiB, the decoded buffer cap is 16 MiB, and at most
two jobs can be active per document. Codec metadata, channels, rate, decoded
sample growth, and requested resampled output are checked against those bounds
before accepting output. Ogg/Vorbis requires a complete single logical stream.
Container/codec agreement is checked for MP3 and M4A, and a declared duration
that materially exceeds decoded PCM is rejected as truncation. An MP3 stream
with no duration header cut exactly at a valid frame boundary is inherently
indistinguishable from a shorter valid stream. Opus, WebM video or multiple
tracks, non-LC ADTS AAC, encrypted media, and arbitrary MP4 tracks are not
claimed by `decodeAudioData`. The added containers use shared cancellation-aware
complete-file admission before the upstream demuxer runs; see the
[container policy and limits](encoded-audio-containers.md).

For ordinary single-track AAC/M4A with one nonempty unity-rate edit, the
shared ISO BMFF parser applies the edit's exact sample-frame presentation
window before resampling. This removes encoder priming and tail padding from
both the decoded `AudioBuffer` duration and its PCM. Multi-track files,
multiple or empty edits, fragmented MP4, and edit boundaries between sample
frames fail closed; the decoder does not present raw priming as if it were
audible content. Files without an edit list retain their unedited media
timeline because no presentation trim is specified by the container.

The decoder implementations use existing RustAudio `lewton`/`ogg`, Apache-2.0
`claxon`, and MPL-2.0 Symphonia rather than copied codec implementations.
The exact linked packages and licenses are recorded in
[third-party notices](../THIRD_PARTY_NOTICES.md). Test fixtures are self-authored
synthetic tones with generation commands and hashes in
[fixture provenance](../tests/fixtures/media/README.md).

## Contained playback

The contained media worker pulls bounded PCM16 chunks for ordinary complete
FLAC, Ogg/Vorbis, MP3, AAC-in-M4A, ADTS AAC-LC, audio-only WebM/Vorbis,
and Ogg/FLAC resources. Seek restarts a verified
decoder and discards samples until the requested point; it does not present a
stale pre-seek chunk. Other supported containers, including H.264/AAC video,
retain their separate media paths. No host-installed FLAC decoder is required.
Decoder reports and playback share bounded PCM decoding; output-device format
constraints apply separately. Capability queries stay within the verified
[playback limits](encoded-audio-containers.md), not the wider IPC metadata ceilings.
The format-specific direct tests, silent media-worker tests, and hidden
renderer `<source>` tests use actual PCM and malformed-input cases.
Ordinary AAC/M4A edit-list timing follows the same sample window as Web Audio;
unsupported MP4 shapes remain on the existing media path rather than being
misreported as a contained-decoder success.

## `MediaRecorder`

`MediaRecorder` currently supports one live, browser-granted audio track and
the `audio/flac` MIME type. It converts captured PCM to signed 16-bit samples
and writes a FLAC STREAMINFO header followed by independently decodable
variable-block frames. The STREAMINFO total-sample count and MD5 remain
unknown while recording, as permitted by RFC 9639. A `dataavailable` Blob may
contain only part of a FLAC stream; concatenating its bytes in event order
produces the complete stream. `requestData()`, timeslice, pause/resume, stop,
track-change errors, and event ordering are tested against real capture packets.

The present capture contract is 8–48 kHz, one or two channels, at most 960
frames per native callback, eight active recorder sessions, and 16 MiB of
pending Blob bytes per recorder segment. Video, multiple audio tracks, other
MIME types, and codecs are explicitly rejected rather than silently recorded
as FLAC. FLAC is lossless relative to the signed 16-bit captured PCM, not
relative to a microphone's analog signal. Regression tests concatenate
browser-emitted Blob chunks and independently decode them with Claxon,
including terminal frames containing only 1–15 samples.

These tests establish the stated paths and bounds, not full interoperability
or a score claim from HTML5test. The measured score, if any, is recorded only
after a fresh hidden release run under the README's fixed benchmark settings.
