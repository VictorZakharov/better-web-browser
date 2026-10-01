# ImageDecoder and VideoFrame

This is a CPU-backed image-frame slice, not a claim that WebCodecs video or
audio encoders are implemented. `ImageDecoder` decodes real encoded images;
`VideoFrame` owns real pixel planes that can be copied, cloned, transferred,
and painted. No URL, hostname, HTML5test detector, or score-specific branch is
involved.

## Scope

| Surface | Implemented contract | Deliberate limits |
| --- | --- | --- |
| ImageDecoder | PNG/APNG, JPEG, GIF, WebP, BMP, ICO; actual frame pixels; random access; stream input; stable `completed` and `tracks.ready` promises | No AVIF/JXL ImageDecoder admission yet; their page/ImageBitmap decoders currently expose a still image rather than the complete track contract |
| Animation | GIF/APNG/WebP frame composition, disposal, cumulative microsecond timing, repetition metadata | Complete input and all admitted frames are decoded before tracks become ready; no incremental/progressive output |
| Image tracks | Read-only indexed list, exclusive selection, separate APNG poster track, animation preference, selection cancellation | Track order is not an application contract; select by `animated`, not a hard-coded index |
| VideoFrame | RGBA, RGBX, BGRA, BGRX, I420, I420A, I422, I422A, I444, I444A, NV12; visible crop, strides, display size, rotation/flip, timestamps, color metadata | 8-bit SDR conversion; unsupported higher-bit-depth, HDR, and wide-gamut rendering reject explicitly |
| Ownership | Private immutable plane snapshots, independent clones, atomic transfer, accepted copy surviving close | Copies are CPU-backed; GPU surfaces and zero-copy cross-process resources are not supplied |
| Rendering | Canvas `drawImage`, `createPattern`, `createImageBitmap`, OffscreenCanvas | Existing Canvas limits and rendering capabilities still apply |
| Workers | Dedicated-worker decoder jobs, streamed bytes, frame messages and transfers, OffscreenCanvas painting | Secure creator and secure worker origin are required for ImageDecoder |

`ImageDecoder`, `ImageTrack`, and `ImageTrackList` are exposed only in secure
contexts, including trustworthy loopback origins. VideoFrame itself does not
carry WebCodecs' SecureContext annotation. Embedded-context exposure fails
closed until the existing realm host can prove ancestor trust.

Calling `isTypeSupported()` is a capability query, not an execution probe. A
valid unsupported image MIME resolves false. A malformed or non-image MIME
rejects with TypeError. Constructing a decoder for a valid unsupported codec
does not synchronously throw: its decode/metadata promises reject with
NotSupportedError. Successfully admitted codecs still reject malformed data.

## Resource lifetime

The renderer's JavaScript control thread does not execute the image codec.
Each realm owns a registry of bounded background jobs. Encoded input is
snapshotted before its ArrayBuffer transfers are detached. A worker publishes
either complete frame metadata/pixels or an actionable error; partial output
is never advertised as a successfully decoded track.

`reset()` aborts pending output requests without deleting the encoded resource
or established tracks. Track selection changes also abort pending requests.
`close()` removes the native session, cancels pending copies/polling and stream
reads, and empties the track list. Already returned frames retain their own
resources and remain usable. Navigation/realm teardown cancels every session;
worker `close()` and termination do the same even while the runtime object is
still retained by its owner.

A VideoFrame clone shares only private immutable planes. Closing or transferring
one reference does not invalidate another. Transfer validation and graph
serialization finish before any sender resources are detached. Author-written
`close()` or `colorSpace.toJSON()` replacements cannot intercept the internal
transfer steps. Frame resources cannot be serialized for persistent IndexedDB
storage, as required by WebCodecs; ordinary structuredClone/postMessage remains
available.

## Limits and scheduling

- Encoded input: 24 MiB; dimensions: 8192 per axis; decoded image: 4 Mi pixels.
- Retained decoded animation/poster pixels: 64 MiB and 256 animation frames.
- Each realm: two live decoder sessions and two codec workers. Closing a
  session does not release its worker permit until that thread actually exits.
- Each decoder: 32 pending output requests. Excess requests reject without
  cancelling already accepted work.
- Frame layout/destination allocation: 64 MiB. Stride arithmetic, plane count,
  plane overlap, chroma-origin alignment, and detached buffers are validated.
- Pixel delivery to JavaScript uses 64 KiB host chunks with event-loop yields.
  CPU color conversion and Canvas operations retain their existing bounded
  synchronous behavior; this is not a compositor/GPU performance claim.

Codec cancellation is checked before decode and between animation frames.
A single codec operation is not forcibly interrupted mid-call. Its allocation
limits and the renderer watchdog remain necessary safeguards. Complete-input
eager decoding also means a long animation can delay `tracks.ready`; lazy
per-frame decoding is future work, not hidden behind a successful probe.

For custom layouts, allocation includes the entire final stride. Copying only
writes each row's sample bytes, leaving padding untouched. copyTo defaults to
the source format; explicit output format conversion is limited to the RGB
formats. Raw YUV copies ignore the output color-space option, while unsupported
RGB color targets reject. Rendering applies crop, clockwise rotation, horizontal
flip, and display scaling in that order. Orientation metadata is not baked into
the coded image returned by copyTo.

## Existing open-source reuse

No new production package is introduced. Animation composition and disposal use
the already pinned `image` decoders. The native 8-bit YUV path calls the existing
`yuv` library; it does not duplicate its conversion kernels. ICC profile handling
reuses the project's bounded `moxcms` adapter.

The only new direct dependency declarations are test-only references to `png`
0.18.1 and `gif` 0.14.2, both already resolved through image in Cargo.lock and
listed in THIRD_PARTY_NOTICES.md. Their MIT/Apache-2.0 provenance and manifests
were reviewed. They generate tiny original fixtures; no scraped site content,
third-party image assets, or copied implementation source is committed.

The WebP test fixture uses the existing image encoder for each VP8L payload and
original container assembly according to the published RIFF specification.
The fixture generator refuses output paths outside G: on this workstation.
Generated browser profiles, benchmark captures, and build outputs stay in
ignored G: directories.

## Evidence and reproduction

The owned `tests/image-frame-fixtures/probe.html` page exercises actual bytes,
timing, cancellation, transfer, stream ownership, and Canvas output. It can be
served by the existing hidden fixture server and compared with unified-headless
Chromium. It contains no browser-name branches or relaxed Breeze assertions.
The main build before this slice has no VideoFrame or ImageDecoder and fails
all these contracts; release results and the measured HTML5test before/after
are recorded in README and the PR body after final verification.

Unit tests run the installed V8 interface and the native session boundary.
Separate worker tests prove isolation, secure exposure, stream binding order,
and messages. Contained-renderer tests prove decoded frame pixels cross the
presentation IPC without applying alpha twice. Native admission tests cover
bounded lazy animations, truncated containers, malformed host offsets, and
owned output surviving session teardown.

Useful local commands (with Cargo/TEMP/TMP already directed to G:):

```powershell
cargo test --locked --lib image_ -- --test-threads=8
cargo test --locked --lib video_frames -- --test-threads=8
cargo test --locked --test renderer_process image_frames -- --test-threads=1
cargo run --locked --example generate_frame_fixtures -- G:\Git\better-web-browser\target\owned-frame-fixtures
```

## Primary behavior references

- [WebCodecs: VideoFrame](https://www.w3.org/TR/webcodecs/#videoframe-interface)
- [WebCodecs: ImageDecoder](https://www.w3.org/TR/webcodecs/#imagedecoder-interface)
- [PNG Third Edition: animation](https://www.w3.org/TR/png-3/)
- [WebP RIFF container](https://developers.google.com/speed/webp/docs/riff_container)
- [image decoder API](https://docs.rs/image/0.25.10/image/trait.AnimationDecoder.html)

Known gaps remain explicit: no VideoDecoder/VideoEncoder integration, no
camera/video-element live frame sourcing, no GPU textures, no full HDR color
pipeline, and no all-format incremental image streaming. This API slice should
not be used to advertise any of those capabilities.
