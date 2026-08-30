# ADR 0007: Isolate media decoding and playback in a dedicated process

- Status: Accepted and implemented for the tested non-DRM H.264/AAC MP4 path
- Date: 2026-08-29
- Issue: [#125](https://github.com/VictorZakharov/better-web-browser/issues/125)
- Decode data plane: [#127](https://github.com/VictorZakharov/better-web-browser/issues/127)
- Decoded-frame bridge: [#129](https://github.com/VictorZakharov/better-web-browser/issues/129)
- Parent epic: [#121](https://github.com/VictorZakharov/better-web-browser/issues/121)

## Context

At the start of this decision Breeze parsed `video`, `audio`, and `source` as ordinary HTML elements
but had no media lifecycle, demuxer, decoder, audio sink, presentation clock, frame compositor,
object URLs, or Media Source Extensions. Those capabilities now exist for the tested non-DRM
H.264/AAC MP4 path behind the boundary defined here. Keeping this history explains why the worker,
protocol limits, and fail-closed capability policy exist.

The HTML Standard requires potentially-playing media to advance against a media timeline and keep
audio synchronized with the current playback position. Media Source Extensions adds mutable,
bounded encoded-byte queues with append, eviction, and end-of-stream semantics. Microsoft's Source
Reader can demux and decode compressed input, but explicitly does not render, manage a presentation
clock, or synchronize audio and video. It is therefore a decoder building block, not Breeze's
playback architecture.

Primary sources:

- [HTML media elements](https://html.spec.whatwg.org/multipage/media.html)
- [Media Source Extensions](https://w3c.github.io/media-source/)
- [Media Foundation Source Reader](https://learn.microsoft.com/windows/win32/medfound/source-reader)
- [Media Foundation platform APIs](https://learn.microsoft.com/windows/win32/medfound/media-foundation-platform-apis)
- [Media Foundation transform enumeration](https://learn.microsoft.com/windows/win32/api/mfapi/nf-mfapi-mftenumex)
- [Media Foundation buffer locking](https://learn.microsoft.com/windows/win32/api/mfobjects/nf-mfobjects-imfmediabuffer-lock)
- [NV12 format](https://learn.microsoft.com/windows/win32/medfound/recommended-8-bit-yuv-formats-for-video-rendering#nv12)

## Decision

Create a dedicated media worker with its own capability-free AppContainer identity, process, Job
Object, nonce, session identity, protocol magic, version, sequence space, timeouts, and memory
ceiling. The worker is a hidden mode of the signed Breeze executable and is selected before browser
UI initialization; ordinary launches cannot enter it accidentally.

The media worker will own:

- container parsing, decoder instantiation, encoded and decoded queues;
- the authoritative presentation clock and audio output;
- seeking, pause/resume, volume, track selection, and end-of-stream state;
- decoded video-frame lifetime until a bounded frame transfer is acknowledged; and
- Media Foundation startup, shutdown, and transform activation.

The page renderer will own standards-facing media objects, event-loop integration, element layout,
controls, and compositing acknowledged video frames. It will never load OS decoders. The privileged
browser will own user activation, autoplay policy, origin/cookie/referrer policy, worker lifecycle,
and the mapping from an active document to its media sessions. It will not parse or retain media
payloads.

The production byte path streams policy-authorized fetch batches through the capability-free page
renderer to the contained media worker. The privileged browser reconstructs network requests and
retains origin, cookie, referrer, and redirect policy; it does not parse media containers or decode
frames. Encoded queues and every cross-process message remain bounded and document/session scoped.

An intermediate, test-only data plane proves the decoder boundary without relaxing that invariant.
It is a dedicated one-way pipe with independent `BRD1` framing, protocol version, bootstrap nonce,
worker-session and source identities, contiguous offsets, chunk bounds, and an explicit end marker.
The `BRM1` control plane declares a source identity and total encoded length before allocation. Only
the hidden test API can write owned fixture bytes; production page and network paths have no
admission method. Because this slice creates a seekable in-memory byte stream, a source must also
fit the smaller resident encoded-queue budget; future incremental input must retain that
resident-memory bound.

A second test-only one-way pipe proves decoded video can leave the worker without sharing decoder
pointers or handles. Its independent `BRV1` framing repeats the bootstrap nonce, worker session,
source generation, frame generation, timing, dimensions, stride, NV12 format, total length, and
contiguous offset on every bounded chunk. The receiver validates all metadata before reserving the
frame and requires an explicit end marker. The worker permits one outstanding frame, retains its
owned copy after transmission, and releases it only after an exact source/frame acknowledgement on
the `BRM1` control plane. Missing, stale, or duplicate acknowledgements fail that worker closed.

Media Foundation buffers are locked only inside the worker, copied while locked, and always
unlocked before their COM ownership can end. The renderer converts acknowledged stride-aware NV12
frames to opaque premultiplied BGRA for page compositing; XAudio2 presents synchronized PCM audio.
The same bounded transport is covered by deterministic fixtures and hidden page-visible playback.

The Windows backend is the OS-provided Media Foundation stack. Capability probing occurs inside the
media worker and asks for web-filtered H.264 video and AAC audio decoder transforms. The complete
MP4 demux/decode/present/audio path is tested end to end, so `canPlayType()`,
`MediaSource.isTypeSupported()`, and `MediaCapabilities.decodingInfo()` advertise that exact path.
Unknown codecs, unsupported containers, WebRTC configurations, and encrypted-media configurations
continue to fail closed. `powerEfficient` remains false because the backend does not prove a
hardware or otherwise power-optimal decode path.

## Resource and failure contract

Central limits bound control payloads, tracks, dimensions, total encoded bytes, resident encoded
queues, decoded-frame bytes/count, decoder candidates, startup/probe/shutdown time, sessions per tab,
and the worker Job Object. Future protocols must validate limits before allocation and must carry
document plus media-session generations before accepting page-owned work.

Each worker has one process and no child-process, network, console, ambient handle, or broad
environment authority. Malformed IPC, stale sessions, timeouts, decoder faults, access violations,
and out-of-memory termination kill only that worker Job Object. Browser state may replace the failed
worker; it must not retry unboundedly or affect sibling sessions.

Diagnostics may expose the worker PID/session, containment flags, Media Foundation HRESULTs,
bounded decoder counts, queue budgets, last progress, and exit reason. They must not expose media
bytes, URLs, credentials, cookies, request headers, or decoder-owned pointers.

## Consequences

- Remote progressive and bounded MSE playback use the contained, document-scoped byte path.
- The renderer/media boundary needs bounded frame transport and acknowledgement rather than a
  native child HWND, so video participates in clipping, scrolling, fullscreen, and repaint.
- Audio synchronization and timing are explicit media-worker responsibilities; Source Reader alone
  does not supply them.
- Media Foundation and its OS codecs are reused without vendoring patented codec implementations.
- Additional formats are advertised only after their complete playback path is proven.
