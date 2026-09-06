# Media presentation and playback permission

Video decoding and presentation must not wait for the document's JavaScript callbacks. The
restricted media worker owns H.264/AAC decoding and the audio clock. A renderer-side video producer
polls that clock independently of the document thread and sends only the newest due video frame.
DOM events, media-element state changes, intrinsic-size changes, and layout remain document-owned.

## Bounded delivery

- The media client serializes access to its worker. Decode/append operations and video polling
  cannot issue overlapping requests; the video producer skips a busy client instead of queuing work.
- Each video transfer identifies its document, committed presentation revision, node, frame number,
  and dimensions. Contiguous chunks are at most 1 MiB; complete frames must fit the existing decoded
  frame limit. Only one frame is assembled at a time.
- The browser event queue retains the newest video frame, not a playback backlog. The UI accepts
  pixels only for its current document/revision and an existing image with matching dimensions.
  Geometry and clipping come from the retained document scene, never from the video message.
- Source/seek epochs discard stale in-flight results. Document retirement clears video registration
  and queues a bounded, source-specific pause, including when an append currently owns the worker.
- Video traffic does **not** acknowledge document progress or reset the JavaScript watchdog.
  A regression test deliberately hangs the document while video advances and verifies containment.

The document retains a shared copy of the latest pixels for later scene snapshots. Video-only
changes do not require a complete display-list transfer. A changed intrinsic size still requires
document layout before the browser accepts the new dimensions.

## Measurement

`media.frames_submitted` is a producer count, not proof that every frame reached a paint. The media
`dropped_frames` counter records overdue frames skipped by that producer. Browser-side queue
coalescing may discard additional submissions.

Hidden benchmarks paint accepted video updates through the regular offscreen painter and report
`video_paint_cadence`: completed paints, elapsed span, FPS, interval p95 (capped at 1,000 ms), the
uncapped maximum interval, and bounded playback-state transitions. These are software-paint
measurements, not display-vsync measurements. Use them alongside the 500 ms filmstrip; navigation
"page ready" and first useful visible content are different milestones.

## Playback permission

Breeze permits silent playback or audible playback following sticky activation in the owning
document. Activation comes from validated browser input, not a script-dispatched DOM event.
Disallowed `play()` requests reject with `NotAllowedError`; unmuting an unactivated, silently playing
element pauses it. A new document starts without the old document's activation.

This is a user-agent policy under HTML's [allowed-to-play contract](https://html.spec.whatwg.org/multipage/media.html#allowed-to-play).
It does not imply complete HTML media or Media Source Extensions conformance. Automated audio
suppression is separate from permission: hidden Breeze runs use silent output, and Chromium
references use unified headless mode with `--mute-audio`.

## Current limitations

Live YouTube startup, memory use, controls styling, and broader layout fidelity remain validation
targets. Passing owned media fixtures or decoding a frame alone is not evidence of a usable watch
page. DRM and codecs outside the negotiated H.264/AAC path are not supported.

MSE duration is distinct from the currently decoded buffered extent: initial decode and append
must not shorten an author's declared timeline. Duration reduction conservatively rejects values
below a buffered range end until native coded-frame presentation timestamps are exposed. This is
not complete [MSE duration-change](https://w3c.github.io/media-source/#duration-change-algorithm)
support. Unbuffered seeking and full-length playback still require end-to-end validation.

## September 6 validation checkpoint (not completion)

| Check | Observed result |
| --- | --- |
| Uninterrupted 35-second watch capture | 30.08 completed paints/s; 47 ms p95 interval |
| Live keyboard control capture | Pause/resume transitions observed; no JS errors or renderer exits |
| Longer watch capture | Media connection failed after about 38 seconds of playback |
| Worker memory trace | Private memory rose from 107 MiB to 456 MiB before termination |

Short playback passes do not establish stability. The longer-run failure blocks completion.
The decoder-lifetime change must be remeasured against that failure before claiming a memory fix.
