# Bounded native ImageBitmap resampling

The RGBA8 ImageBitmap resize path reuses Breeze's existing native drawImage
sampler. It replaces per-pixel JavaScript arrays without changing pixel-center
coordinates, edge clamping, nearest half-way selection, associated-alpha
bilinear interpolation, or Uint8ClampedArray round-to-even quantization.
Transparent RGB is not allowed to bleed into visible interpolated samples.

This is an execution/ownership change, not new format or quality support.
The existing higher-precision normalized-word path remains separate. The
[HTML ImageBitmap resize algorithm](https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html#dom-createimagebitmap)
still controls geometry and options, including pixelated's integer-nearest
intermediate followed by bilinear resampling to the requested dimensions.

Window and dedicated Worker bootstraps use the same stateless host operation.
Both source and destination dimensions are validated before allocation: each
must be nonzero and neither image may exceed four Mi pixels. Thin images retain
the existing pixel budget rather than receiving a new arbitrary axis limit.
The wire record rejects extra fields, malformed dimensions and missing
members. The source must have exactly four bytes per pixel. Every invocation
has at most sixteen Mi bilinear taps and 32 MiB of owned source/output bytes;
existing JS snapshots and bridge transport have their separate budgets.

The bridge may consume its already-owned source copy but never modifies the
author's ImageData or retains a JS handle. The output has independent storage.
Private typed-array accessors and constructors are captured during bootstrap;
author prototype hooks do not receive private samples. Origin-clean ownership,
closing, transfer and promise rejection stay in the existing ImageBitmap layer.

The implementation adds no dependency and imports no third-party source. It
shares this project's existing sampler rather than introducing a second resize
kernel or substituting a differently weighted library filter. Native tests use
an independent transcription of the previous scalar contract over a bounded
dimension grid. End-to-end tests cover transparent edges, immediate mutation
of the original source, close, Worker parity, and replaced pixel accessors.

`tests/canvas/bitmap-resize-throughput.html` is an owned completed-work fixture.
Its measurement includes promise completion, drawing, and pixel readback. The
whole-bitmap hash must remain unchanged against the merged Breeze baseline;
timings and allowed reference-browser kernel differences are reported separately.

## Intermediate completed-work measurements

Three fresh alternating runs used merged #229, the native-resize build, and
Chrome 154, with CPU sampling disabled. Each workload resizes an owned 64×64
ImageData twelve times and includes drawing and full final readback in timing.
These are intermediate build measurements, not final-PR-head or whole-browser
rankings. Chrome's kernel/alpha-quantization output differs in every tested case.

| Completed workload | Merged #229 | Native resize | Chrome 154 |
| --- | ---: | ---: | ---: |
| Bilinear enlargement to 256×256 | 133.4 ms | 64.3 ms | 321.5 ms |
| Pixelated resize to 150×150 | 54.4 ms | 25.6 ms | 99.8 ms |
| Bilinear reduction to 19×23 | 1.4 ms | 1.2 ms | 3.5 ms |

Breeze's before/after whole-bitmap hashes are identical across all three runs:
55,090,629 for enlargement, 3,186,927,121 for pixelated resize, and 1,573,032,206
for reduction. Chrome's hashes differ; timing numbers do not establish identical
rendering quality or a general advantage over Chrome.

## Final-source release replay

Three fresh alternating runs against the complete October 8 source release use
the same fixture and include final readback. No builds/tests run concurrently,
CPU sampling is off, and the viewport/device scale match across browsers.

| Completed workload | Merged #229 | October 8 release | Chrome 154 |
| --- | ---: | ---: | ---: |
| Bilinear enlargement to 256×256 | 132.0 ms | 67.7 ms | 116.4 ms |
| Pixelated resize to 150×150 | 54.1 ms | 26.7 ms | 107.8 ms |
| Bilinear reduction to 19×23 | 1.4 ms | 1.1 ms | 2.3 ms |

All three Breeze before/after hashes remain the exact values above. Chrome's
three hashes also remain stable but different. The materially different Chrome
enlargement time in this later replay reinforces why these small samples are
fixture observations, not a universal browser-speed ranking.
