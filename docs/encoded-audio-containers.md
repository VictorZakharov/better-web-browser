# Complete-file audio containers

This slice extends real PCM decoding and contained audio-element playback, not
just format detection. A shared playback support matrix backs `<source type>`
and `HTMLMediaElement.canPlayType()`. File `MediaCapabilities` queries use that
matrix after stricter one-track MIME/dictionary validation and output limits.

| Container / codec | `decodeAudioData` | Contained playback | SourceBuffer |
| --- | --- | --- | --- |
| ADTS / AAC-LC | Yes | Yes | No |
| Audio-only WebM / Vorbis | Yes | Yes | No |
| Ogg / FLAC | Yes | Yes | No |
| Ogg / Opus, mapping family 0 mono/stereo | Yes | Yes | No |
| MP3, ordinary AAC/M4A, WAV, native FLAC, Ogg/Vorbis | Existing paths preserved | Existing paths preserved | Only the existing supported ISO-BMFF segment paths |
| WebM / Opus or video | No | No | No |

ADTS playback previously used the Windows Media Foundation fallback. This batch
adds the bundled, bounded AAC-LC path to both consumers and brings ADTS to Web
Audio; it does not describe ADTS playback itself as previously unavailable.

WebM is admitted only as an audio-only, single Vorbis track. Ogg/FLAC must be a
complete supported logical stream. AAC admission is restricted to the supported
ADTS AAC-LC configuration. An unsupported or corrupt recognized source fails
instead of silently falling through to another decoder. A child `<source>` may
then advance through the ordinary HTML source-selection lifecycle; stale
responses cannot revive the failed candidate.

## Existing open-source implementations

The pinned, pure-Rust Symphonia 0.6.1 framework supplies AAC decoding and ADTS,
WebM and Ogg demuxing, Vorbis decoding for WebM, and FLAC decoding for Ogg. Its
newly enabled packages use MPL-2.0. The existing Ogg packet reader validates Ogg
packet/page integrity. Native FLAC continues to use claxon; Ogg/Vorbis continues
to use lewton. No codec implementation is copied or rewritten here.
Ogg/Opus uses the existing Ogg reader and vetted bundled libopus through a safe
wrapper; its [presentation, provenance, and build policy](ogg-opus.md) are
shared by Web Audio and contained playback.
The added packages are official Symphonia 0.6.1 publications from upstream
commit `ee35874b571a35a9a6e15d3bc9a3aaf8f11fbeee`; their MPL-2.0 license,
absence of package build scripts, and `forbid(unsafe_code)` were inspected.
Their source is not vendored or modified by this batch.

Browser-owned admission adds bounded complete-file, header and metadata checks
around those parsers. Those checks are not an alternate packet demuxer. Existing
Vorbis setup-allocation guards are reused for Xiph-laced WebM headers. The
fixture generator creates an owned tone; fixture provenance and generation
commands are recorded in [the media fixture inventory](../tests/fixtures/media/README.md).

## Behavior and limits

- Web Audio retains its 8 MiB encoded-input and 16 MiB decoded/resampled-buffer
  budgets, worker-thread cancellation, input detachment, and asynchronous promise
  and legacy callback completion. It resamples actual decoded channel samples.
- Contained playback retains its encoded, packet, PCM, sample, duration and command
  budgets. Seeking restarts bounded decoding and discards PCM to the requested
  sample boundary. This favors exact results over fast seeking in large files.
- `MediaCapabilities.decodingInfo()` validates a single codec per audio/video
  configuration. Multi-codec container MIME types require exactly one `codecs`
  parameter; native FLAC, MP3 and ADTS AAC MIME types allow no parameters.
  This is distinct from `canPlayType()` compatibility queries: a codec-less
  admitted container can return `maybe` there without being a valid capability
  configuration. Malformed configurations reject; valid unsupported ones resolve
  with `supported: false`.
- Explicit capability channels are limited to numeric mono/stereo counts, not
  inferred layout names. Explicit source rates stay within 8–192 kHz; MP3 uses
  its supported indexed rates up to 48 kHz and ADTS AAC-LC up to 96 kHz.
  Native FLAC's decoder-only metadata ceiling does not imply that XAudio2 can
  render a 384 kHz source. Capability reports keep both `smooth` and
  `powerEfficient` false because this matrix does not measure frame delivery or
  device power efficiency. They do not advertise WebRTC or encrypted decoding.
- MediaSource AAC queries use the [Media Foundation AAC decoder's documented
  indexed 8–48 kHz rate contract](https://learn.microsoft.com/en-us/windows/win32/medfound/aac-decoder#format-constraints),
  not the complete-file Symphonia decoder's 8–192 kHz explicit-rate allowance.
- Single supported audio tracks are required. A supported track does not authorize
  ignoring an unsupported video or another stream in the same source.
- WebM timing must fit the upstream signed timestamp arithmetic before any PCM
  is decoded. The guard checks a conservative whole-file extrema envelope, so
  independently valid but near-64-bit-limit timestamp/relative-offset/duration
  combinations can be rejected. This bounded policy prevents an upstream timing
  overflow from masquerading as clean EOF after a partial decode; it is not a
  claim to admit every Matroska timeline. Empty `TrackTimestampScale` uses its
  schema default of 1.0, as required by [EBML's empty-element rule](https://www.rfc-editor.org/rfc/rfc8794.html#section-6.1).
- ADTS complete framing and decoded packets are checked, but the pinned upstream
  parser skips rather than verifies an optional ADTS CRC. Ogg page CRCs are
  verified. This is a documented decoder limitation, not a claim of equivalent
  checksum validation across the two containers.
- `MediaSource.isTypeSupported()` stays limited to the implemented ISO-BMFF
  segment parser. Complete-file AAC support does not imply raw ADTS append support.
  Recording has separate encoder admission: the default remains FLAC, with
  [bounded Ogg/Opus microphone recording](ogg-opus.md#incremental-microphone-recording)
  now supported. Complete-file decoder support does not imply AAC, MP3, Vorbis,
  or WebM recording. Hardware efficiency is not claimed.

MIME parsing follows the MIME Sniffing algorithm for HTTP whitespace, quoted
values, escaped characters and the first valid duplicate parameter. A quoted
unrelated value cannot inject a `codecs` parameter. An explicit unsupported codec
returns the empty string; an admitted container without an explicit codec remains
`maybe`. Native FLAC has no defined codec parameter.

Tests cover real PCM and resampling, presentation duration, exact post-seek sample
suffixes, malformed/truncated input, unsupported profiles/tracks, bounded decoder
resources, asynchronous decoding and document retirement. Hidden renderer tests
exercise the ordinary media and Web Audio paths. HTML5test is an inventory only;
the measured score is reported separately and is not proof of conformance.

## Primary references

- [Web Audio decodeAudioData](https://www.w3.org/TR/webaudio/#dom-baseaudiocontext-decodeaudiodata)
- [HTML media source selection and canPlayType](https://html.spec.whatwg.org/multipage/media.html)
- [MIME parsing](https://mimesniff.spec.whatwg.org/#parse-a-mime-type)
- [MediaCapabilities configuration validity](https://w3c.github.io/media-capabilities/)
- [XAudio2 source-voice format limits](https://learn.microsoft.com/en-us/windows/win32/api/xaudio2/nf-xaudio2-ixaudio2-createsourcevoice)
- [WebM container guidelines](https://www.webmproject.org/docs/container/)
- [Ogg FLAC mapping, RFC 9639](https://www.rfc-editor.org/rfc/rfc9639.html#section-10.1)
- [Symphonia 0.6.1 supported formats and codecs](https://docs.rs/symphonia/0.6.1/symphonia/)
