# Owned modern-image fixtures

These small images are generated from six project-owned RGBA pixels in
`scripts/generate-image-fixtures.ps1`. No photographs, upstream source code, or
third-party binary assets are copied into this directory. They are covered by
the repository's MIT license.

The generator runs the locally installed FFmpeg's libaom and libjxl encoders,
then decodes each image with that tool's native decoders to create an independent
RGBA reference. FFmpeg is a fixture-generation tool only: its executable and
libraries are not vendored, linked into Breeze, or distributed here. The local
tool used for this set is `N-111280-gd51b0580e4-20230625`.

Regenerate on this Windows development machine:

```powershell
./scripts/generate-image-fixtures.ps1 -FixtureDirectory tests/image-fixtures
```

Both intermediate binary outputs and committed Base64 text must stay on G:.
The script bounds encoder runs to 30 seconds, uses one encoder thread, removes
unnecessary metadata, and launches without a console window. Regeneration is
not required to run tests and does not run in CI.

The `3×2` rectangular pattern contains red, green, blue, white, black and
`(128,64,32)` pixels. Alpha variants have coverage values
`255,128,64,0,255,192`. Tests compare actual decoded pixels, not signatures or
API presence. AVIF fixtures cover identity RGB, 4:2:0, 4:2:2, 4:4:4, full/limited
range, monochrome and 8/10/12-bit samples. JPEG XL fixtures cover lossless/lossy,
grayscale/RGB, alpha and 8/16-bit samples. YUV rounding differences between
independent conversion implementations have a small explicitly declared byte
tolerance; identity RGB and lossless RGBA have exact comparisons.

The auxiliary-alpha AVIF fixture uses two lossless AV1 streams (identity RGB
and full-range monochrome coverage). Its expected RGBA is the owned input
pattern, since the older FFmpeg demuxer exposes those streams separately.

`probe.html` is an owned offline pixel oracle: it fetches these Base64 files,
creates ImageBitmaps, draws them to Canvas and records `getImageData` bytes in
diagnostic attributes. It has no external requests, credentials, analytics or
third-party source. Serve it with the existing loopback fixture server and
run both browser harnesses headlessly.

Chrome 154.0.8037.92 decodes the seven initial AVIF variants, but rejects the
JPEG XL variants. The 12-bit 4:2:0 AVIF oracle is Chromium's exact Canvas
readback, recorded in `modern_tests.rs`: FFmpeg interpolates its chroma while
Chromium reconstructs nearest samples. Do not widen a byte tolerance to hide
that policy difference. JPEG XL remains checked against the independent
native libjxl decoder, not against an unsupported Chrome path.
