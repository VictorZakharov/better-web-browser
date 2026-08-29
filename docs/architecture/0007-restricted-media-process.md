# ADR 0007: Isolate media decoding and playback in a dedicated process

- Status: Accepted; boundary, capability probe, bounded byte pipe, and owned-fixture decode implemented
- Date: 2026-08-29
- Issue: [#125](https://github.com/VictorZakharov/better-web-browser/issues/125)
- Decode data plane: [#127](https://github.com/VictorZakharov/better-web-browser/issues/127)
- Parent epic: [#121](https://github.com/VictorZakharov/better-web-browser/issues/121)

## Context

Breeze parses `video`, `audio`, and `source` as ordinary HTML elements but has no media lifecycle,
demuxer, decoder, audio sink, presentation clock, frame compositor, object URLs, or Media Source
Extensions. Adding those pieces to either the privileged browser process or the page renderer would
make an expensive boundary permanent before hostile-media ownership is explicit.

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

The target byte path is a separately contained network service streaming policy-authorized bytes
directly to a media worker. Until that service exists, this first slice carries no URLs, cookies,
headers, encoded bytes, or decoded frames. Reusing the current browser-owned Fetch response body for
media would violate the target invariant and is not an allowed temporary shortcut.

An intermediate, test-only data plane proves the decoder boundary without relaxing that invariant.
It is a dedicated one-way pipe with independent `BRD1` framing, protocol version, bootstrap nonce,
worker-session and source identities, contiguous offsets, chunk bounds, and an explicit end marker. The `BRM1` control
plane declares a source identity and total encoded length before allocation. Only the hidden test
API can write owned fixture bytes; production page and network paths have no admission method.
Because this slice creates a seekable in-memory byte stream, a source must also fit the smaller
resident encoded-queue budget; future incremental input must retain that resident-memory bound.

The initial Windows backend is the OS-provided Media Foundation stack. Capability probing occurs
inside the media worker and asks for web-filtered H.264 video and AAC audio decoder transforms.
Probe results remain internal diagnostics. `canPlayType()`, `MediaSource.isTypeSupported()`, and
MediaCapabilities must continue to report no support until the complete demux/decode/present/audio
path for that exact type is tested end to end.

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

- Progressive playback requires a contained network-byte path before remote media is admitted.
- The renderer/media boundary needs bounded frame transport and acknowledgement rather than a
  native child HWND, so video participates in clipping, scrolling, fullscreen, and repaint.
- Audio synchronization and timing are explicit media-worker responsibilities; Source Reader alone
  does not supply them.
- Media Foundation and its OS codecs are reused without vendoring patented codec implementations.
- Additional formats are advertised only after their complete playback path is proven.
