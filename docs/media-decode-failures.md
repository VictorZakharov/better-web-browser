# Adaptive video decoder state and failure containment

## September 9, 2026 incident

A user reported that scrolling a YouTube watch page stopped the entire document with
`H.264 transform accepted 150 access units but emitted no frame`.
The count is the segment's sample count, not a 150-sample timeout.
The screenshot alone does not establish that scrolling caused the decode failure.

Two independently reproduced decoder-state bugs and the document-fatal error path are fixed:

| Contract | Before | After |
| --- | --- | --- |
| Seek beyond a valid buffered segment | Valid decoded preroll was discarded, then reported as no decoder output | Count decoded output before seek filtering; return buffer exhaustion and allow seeking back |
| Receive a replacement initialization segment | The immediate segment used it, but later media-only appends reused the first initialization bytes | Subsequent appends use the latest initialization segment, including AVC SPS/PPS |
| Codec error while requesting a frame | Worker exited and the presenter/seek error stopped the document | Retire the failed audio/video source, consume its frame identity, preserve the control session, and report a media-element error |
| Deferred retirement after decode rejection | A pause could target a source already removed by the worker | Clear the client's installed source identity when the decoder rejects a frame |

The native decoder still rejects undecodable access units. No timeout, sample budget, codec
validation, or IPC identity check is relaxed. Stale commands, unacknowledged frames, and transport
failures remain protocol failures; they are not disguised as successful decoding.

The renderer records the failure in its media diagnostics and stops advancing that source. It
preserves the document and the last presented image. The page receives `MEDIA_ERR_DECODE` with
`NETWORK_IDLE`, once per failed resource, and can run timers or load another source. Further
SourceBuffer updates on an errored media element are rejected. This is not automatic retry of
corrupt media and does not claim complete Media Source Extensions conformance.

## Regression evidence

- The checked-in, BSD-licensed fragmented fixture reproduced the original error family:
  `H.264 transform accepted 31 access units but emitted no frame`. All frames were valid preroll
  before the requested target. The regression now exhausts normally and can seek back to zero.
- Restoring the old initialization-cache condition makes the reconfiguration regression fail:
  its third transfer contains initialization marker **1** instead of **2**. Tests cover both
  combined initialization/media appends and separately appended replacement initialization.
- A real Media Foundation decoder rejects corrupted fixture sample bytes while its MP4 metadata
  remains valid. The worker consumes the failed frame request and accepts the next valid source.
- Hidden, silent isolated-renderer tests cover failure during both seeking and ordinary video
  presentation. An `error` listener and a subsequent document timer run in both cases, then
  `load()` resets the failed element and a replacement MediaSource decodes on the same worker.
- A client regression verifies that deferred retirement does not send a stale pause after a
  decode failure, and a subsequent source can be installed.

The reported live URL was also exercised using the hidden benchmark, explicit video activation,
and scrolling. The pre-fix executable from parser PR #141 presented 2,298 video frames over
46.377 seconds without reproducing the crash. That is a negative reproduction, not evidence that
the reported problem was absent. The user's exact failing bitstream/quality-switch sequence was
not captured, so the precise live trigger remains unconfirmed. The deterministic regressions
prove the corrected contracts, not universal YouTube playback reliability or a speedup.

## Standards and platform references

- [HTML media data processing](https://html.spec.whatwg.org/multipage/media.html#media-data-processing-steps-list):
  fatal decode errors belong to the media element, not the document.
- [MSE initialization segment receipt](https://www.w3.org/TR/media-source-2/#sourcebuffer-init-segment-received):
  subsequent initialization segments supply updated track descriptions.
- [MSE prepare append](https://www.w3.org/TR/media-source-2/#sourcebuffer-prepare-append):
  an attached media element with an error cannot accept another append.
- [Windows H.264 decoder](https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-decoder):
  Annex-B input must carry valid SPS/PPS; accepting input alone does not prove decoded output.
