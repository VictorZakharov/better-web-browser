# WebCodecs AV1 decoding

This implements real `VideoDecoder` and immutable `EncodedVideoChunk` resources
in Window and dedicated Worker realms. It uses the existing locked rav1d 1.1.0
decoder (BSD-2-Clause), existing YUV/color conversion and existing VideoFrame
ownership, cloning and Canvas paths. No new dependency or third-party source
was added. There is no VideoEncoder or placeholder software codec.

Contracts follow the [WebCodecs September 21, 2026 working draft](https://www.w3.org/TR/webcodecs/)
and [AV1 codec registration](https://www.w3.org/TR/webcodecs-av1-codec-registration/).
The registration uses low-overhead AV1 OBUs, not IVF files or Annex B framing;
`description` is unused for AV1. Test-only IVF extraction does not add a public
container demuxer.

## Admitted baseline

- Profile zero, main tier, 8-bit AV1 codec strings, with the registered short
  spelling or complete optional SDR color fields.
- Software decoding, one codec thread, up to 1,920×1,080 pixels. Maximum width
  or height is 1,920. Actual output dimensions come from decoded pictures.
- Required inter-frame references persist across packets. Flush drains outputs;
  it does not replace the context. Configuration, reset and close replace or
  cancel the context and require a new key chunk.
- A declared key chunk must contain a frame header and decoded key-frame type.
  OBU envelope lengths, reserved bits and packet limits are checked before
  handing codec syntax to rav1d.
- Picture color encoding is converted through the existing bounded native color
  path to sRGB RGBA. Output VideoFrame metadata describes that converted format.
  Author color-space overrides, HDR and higher-bit-depth/profile configurations
  are not advertised by this baseline.
- Display aspect-ratio adjustment enlarges a display dimension before rotation.
  Rotation retains the author's converted double in the support-query snapshot;
  decoded frames use the existing clockwise quarter-turn convention. Flip and
  frame orientation remain metadata consumed by Canvas rendering.

`codedWidth`/`codedHeight` are admission/selection hints, not fabricated output
dimensions. They do not rescale pictures. Configuration dictionaries are
snapshotted synchronously, including description view extents; support-query
results settle on later tasks and omit unrecognized members.

## Resources and scheduling

Encoded chunks own an immutable snapshot and copy only into accepted destination
views. Constructor transfers validate the whole list before detaching buffers.
Chunks are serializable but not transferable. Native and Worker message cloning
preserve video branding, aliases, metadata and bytes without author getters or
constructor replacements. Decoded VideoFrames use the existing transferable
resource implementation; cloning/transfer does not keep a decoder alive.

Each realm may retain four native video workers; a process may retain sixteen.
A lifetime permit remains held until the actual worker exits, including after
cancellation. One outstanding command and one bounded result per session keep
the JavaScript thread nonblocking. Session IDs increase monotonically.

Input chunks are capped at four MiB, queued commands at 32/eight MiB, and a native
output batch at eight MiB. OBU and picture loops have explicit progress/count
bounds. Existing renderer containment and process memory limits still apply.
This is a deliberately bounded software baseline, not universal AV1 playback.

Reset/close invalidate generations, reject pending flushes and suppress stale
callbacks. Native errors close only the failed codec and deliver the same
exception to its error callback and pending flush. Callback exceptions are
reported as script errors without being mistaken for decode failures.
Document and Worker teardown cancel their sessions.

## Verification

The original eight-frame 16×16 video changes its pixels and uses inter-frame
references. Independent FFmpeg decode produces the pixel reference; tests allow
at most three byte values of YUV/color rounding difference. Fixture provenance
and regeneration instructions are in `tests/video-codec-fixtures/README.md`.
No upstream video or source code was copied.

Native tests verify persistent references, actual pixels and timing, malformed
packets, cancellation, admission and worker permits. V8 tests verify chunks,
native clone branding, Window/Worker decoding, aspect/rotation, Canvas painting,
reset, key-frame admission and contained error recovery. AppContainer integration
executes actual decoding and frame transfer inside the hidden renderer.

Run the browser comparison without visible UI:

```powershell
./scripts/test-video-codecs.ps1 -Browser ./target/release/better-web-browser.exe `
  -OutputDirectory target/video-proof -ExpectedPassed 7
./scripts/test-video-codecs.ps1 -Chrome -OutputDirectory target/video-chrome-proof
```

Both runs use the exact original packet bytes. The script delegates to the
existing guarded hidden Breeze and unified-headless Chromium launchers; all
captures, profiles and packet staging stay on G:. Failed contracts and reference
differences are reported rather than widened or treated as successful support.

The October 2, 2026 controlled comparison completed **0/7** contracts on merged
#218, **7/7** on this release and **7/7** on Chrome 154.0.8037.97. All eight
reference frames matched the independent RGBA fixture within three byte values.
The pinned WPT subset also includes EncodedVideoChunk construction/copy tests;
the full curated run passed 550 cases and 5,991 assertions. Dedicated Worker
coverage is supplied by original realm/ownership tests, not inferred from the
Window WPT harness. HTML5test remains 487/588 and does not establish codec
conformance. No live-site playback or performance improvement is claimed.
