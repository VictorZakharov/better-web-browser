# Original AV1 video fixture

`eight-frames.ivf.base64` encodes eight 16×16 project-authored RGBA patterns at
four frames per second. Red and blue values divide the frame into quadrants;
green increases by sixteen between frames. There are no photographs, downloaded
videos, third-party test vectors or copied codec implementations. The fixture is
covered by the repository's MIT license.

`scripts/generate-av1-video-fixture.ps1` creates the raw source on G:, encodes
with the locally installed FFmpeg/libaom and decodes an independent RGBA reference
with FFmpeg. Both processes use one thread, a 30-second deadline and
`CreateNoWindow`. FFmpeg is a generation/reference tool only; none of its source,
executables or libraries are distributed or linked into Breeze.

The IVF container is test storage only. Tests extract each elementary AV1 packet
and do not expose an IVF demuxer through WebCodecs. The sequence has an initial key
frame followed by inter-frame references, so recreating the decoder for every
packet cannot satisfy the test. Actual decoded dimensions, samples, timestamps
and nullable durations are checked separately from constructor presence.

Run the generator explicitly to regenerate; normal tests and CI do not require
FFmpeg. Its outputs and scratch files must remain on G:.
