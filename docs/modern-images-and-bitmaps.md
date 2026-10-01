# Modern images and ImageBitmap contracts

This slice implements real AVIF and JPEG XL pixel decoding through a shared
Rust boundary used by page images, Canvas, detached `Image` resources and
dedicated workers. It does not add a format-name stub or modify HTML5test.

## Decoder ownership

`src/engine/image_decode.rs` owns source, dimension, pixel-product and output
allocation checks. The format modules return tightly packed straight-alpha
RGBA. Page presentation converts that once to premultiplied BGRA; Canvas
keeps straight samples and performs alpha-aware sampling/compositing.

| Layer | Implementation | Ownership and policy |
|---|---|---|
| AVIF item extraction | avif-parse 2.0.0 | Bounded encoded input; upstream owns item references and extraction |
| AV1 still decoding | rav1d 1.1.0 | Owned context/data/picture guards; one decoder thread; strict conformance; frame-size limit |
| YUV conversion | yuv 0.8.19 | Explicit matrix, range, bit depth and chroma layout; owned packed copies of padded decoder planes |
| JPEG XL | jxl-oxide 0.12.6 | No Rayon pool; tracked decoder grids; upstream header/container parsing |
| ICC and CICP RGB conversion | moxcms 0.8.1 | Existing project dependency; bounded profiles and row scratch; alpha is never color-transformed |
| Other raster formats | image 0.25.10 | Existing PNG/JPEG/WebP/BMP/GIF/ICO decoder paths, now sharing orientation/ICC handling |
| SVG | resvg 0.48.1 | Existing rasterizer; Canvas pixel-product check now precedes pixmap allocation |

AVIF supports 8/10/12-bit identity RGB, monochrome and 4:2:0/4:2:2/4:4:4
YUV, auxiliary alpha, ICC/nclx metadata, primary-item rotation/mirroring and
integral clean apertures. Metadata is resolved by item association, not by
the first matching property in a file. Unknown essential properties reject
explicitly. Alpha items require matching dimensions and monochrome pixels;
their coverage is full-range independently of color metadata.

JPEG XL supports raw codestreams, single-box and segmented codestream
containers, lossless/lossy RGB/grayscale, associated or straight alpha and
8/16-bit source fixtures. Only codestream events reach the pixel decoder.
Optional Exif/XML/JPEG-reconstruction/Brotli auxiliary metadata is ignored:
it is not needed to paint and must not expand into an unbounded metadata
buffer. Orientation and color metadata are normative codestream fields.

Preflight uses the already-locked JPEG XL parser crates, not a second codec.
Dimensions are checked before image decoding. Encoded and reconstructed
ICC lengths are bounded to 4 MiB before profile expansion. Preview frames
are rejected explicitly until a bounded preview-header interface is available.

The `yuv` scalar 16-to-8 path in this pin can wrap saturated channels after
clamping to the source depth. Breeze uses the library's saturated 16-to-16
transform, then narrows with bounded two-row scratch. An owned 10-bit
saturated-red fixture prevents reintroducing the dark-channel regression.

## ImageBitmap behavior

The [HTML algorithms](https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html)
and [Web IDL dictionary conversion](https://webidl.spec.whatwg.org/#es-dictionary)
are the compatibility contract, rather than a particular website's script.

- Dictionaries are converted synchronously, with one getter read per member
  in lexicographic order. Numeric/enum conversion errors throw immediately.
- Crop arguments use signed-long conversion; negative extents normalize the
  rectangle without reversing pixels. Out-of-source samples are transparent.
- Zero crop dimensions reject with `RangeError`; zero resize dimensions
  reject with `InvalidStateError`. Resize dimensions use `[EnforceRange]`,
  not integer wrapping, and inferred aspect dimensions use a ceiling.
- Canvas/ImageData/ImageBitmap sources are snapshotted before Promise jobs.
  Blob/File bytes are read through an internal capability, not author-overridable
  `bytes`, `arrayBuffer`, `stream`, `size` or `type` members.
- `from-image` applies metadata orientation; `flipY` disregards orientation
  metadata, crops/scales and flips the output vertically. Chromium's legacy
  `imageOrientation: "none"` is retained as an explicit compatibility extension.
- `colorSpaceConversion: "none"` leaves encoded RGB unchanged. Normal decoding
  converts profiles to sRGB. Explicit premultiplication changes actual storage,
  and cloning/worker transfer carries its association flag.
- Pixelated resizing uses the HTML two-stage nearest/integer then bilinear
  algorithm. The other quality preferences currently use the shared bilinear
  filter in premultiplied space, avoiding transparent-color halos.
- Members use private brands. Closing is idempotent; closed source use fails;
  author `close` overrides do not intercept internal transfer detachment.

## Bitmap renderer

[ImageBitmapRenderingContext](https://html.spec.whatwg.org/multipage/canvas.html#the-imagebitmaprenderingcontext-interface)
takes ownership of a bitmap at its natural dimensions without resampling it
to unchanged Canvas content attributes. Null transfer restores an
attribute-sized blank bitmap. Attribute resizing resets the backing store.
`alpha: false` composites over opaque black, including blank/reset states.

The receiver and transferred-image brands are checked, settings are read only
when creating a context, failed settings conversion does not lock the context
mode, and context identifiers are case-sensitive. Offscreen transfer preserves
attribute dimensions, natural bitmap dimensions and the renderer alpha policy.

Presentation exports both the natural bitmap and a content-attribute size
stamp. Native asset keys include that stamp: a resize cannot paint an old
bitmap, but differing natural dimensions no longer discard a valid transferred
image. The same representation survives root and child-document image caches.

## Bounds and deliberate limits

Page raster output is limited to 32 Mi pixels, 32768 on either axis and
128 MiB of output. Canvas/worker decoding is limited to 4 Mi pixels, 8192
on either axis, 24 MiB encoded input and 64 MiB tracked decoder working grids.
Those working limits are not a total-process memory guarantee: other codec
metadata, AV1 internal allocations and caller-owned input coexist. The existing
renderer Job memory ceiling and watchdog remain the outer containment boundary.

This is an SDR, first-displayable-frame baseline, not a claim of complete AVIF
or JPEG XL conformance. AVIF sequences/tiled grids, non-square pixel aspects,
fractional clean apertures, unsupported essential properties and some CICP
matrix encodings reject. There is no animated-frame scheduling, HDR display
pipeline, modern-format encoder, ImageDecoder, VideoFrame, WebGL or WebGPU
implementation in this slice. CMYK JPEG XL can render when the CMS converts
it; unconverted CMYK is not silently presented as RGB.

## Provenance and license audit

All decoder code is consumed from pinned crates.io packages. No upstream
implementation was copied, patched in Cargo's cache, or vendored into the
repository. The existing dependency notices are regenerated from the locked
Windows graph. Optional assembler and Rayon features are disabled.

rav1d is BSD-2-Clause; avif-parse is MPL-2.0; JPEG XL parser/decoder crates
are MIT/Apache-2.0; yuv and moxcms are BSD-3-Clause/Apache-2.0. The transitive
`to_method` 1.1.0 micro-crate uses CC0-1.0. Its complete no_std conversion-trait
source was inspected (no dependencies, build script or unsafe code). The
license policy admits CC0 only for that exact crate/version, not globally.
The [CC0 dedication](https://creativecommons.org/publicdomain/zero/1.0/legalcode.en)
allows reuse; it does not provide trademark or patent rights.

The inspected decoder/build entry points contained normal implementation and
documentation, not execution instructions for this task. Dependency data was
treated as untrusted source material. Cargo's advisory/source/license checks
are run locally before pushing; no advisory is suppressed by this change.

Owned 3×2 fixtures and their regeneration instructions live in
`tests/image-fixtures/README.md`. They are generated from project-owned pixels
with local libaom/libjxl tools; no third-party image is redistributed. Tests
check pixel values, dimensions, metadata ordering, alpha, errors, ownership,
cloning and window/worker behavior instead of only testing API presence.

## Matched release observations — 2026-10-01

| Measurement | Main before this batch | This batch | Chrome 154.0.8037.92 |
|---|---:|---:|---:|
| HTML5test rendered score, three fresh profiles | 487 / 588 | 487 / 588 | Not remeasured for this slice |
| Owned AVIF Canvas readback fixtures | 0 / 8 decoded | 8 / 8 decoded | 8 / 8 decoded |
| Owned JPEG XL Canvas readback fixtures | 0 / 7 decoded | 7 / 7 decoded | 0 / 7 decoded |

HTML5test settings were the default Breeze identity, 1280×720 window,
125% device scale, `en-US`, 4.5-second settle and separate fresh profiles.
All six captures returned HTTP 200 without script errors or renderer exits.
The score is reported as observed, not extrapolated from format support.

The local image probe uses identical project-owned encoded bytes and records
actual 3×2 Canvas RGBA samples. Exact RGB and auxiliary-alpha AVIF samples match
Chrome; the 12-bit fixture has an exact Chromium oracle. Other YUV fixtures can
differ because Chrome interpolates subsampled chroma while the baseline yuv
conversion uses nearest chroma. One gray10 white sample differs by one byte.
Native FFmpeg reference comparisons keep small explicit rounding tolerances;
the 12-bit subsampled case uses Chrome instead because FFmpeg interpolates it
differently. There is no claim of pixel-perfect color equivalence for all files,
and no speed or memory improvement is inferred from this codec coverage.
