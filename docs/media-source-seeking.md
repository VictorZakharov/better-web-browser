# Seeking outside appended MediaSource data

A MediaSource presentation's duration is not the duration of the bytes currently owned by
the native decoder. A forward seek must not be clamped to the end of those bytes.

## Observed failure

The 2026-09-09 live watch-page reproduction requested **517.813 seconds** while audio was
buffered only through **49.923 seconds**. Breeze acknowledged `seeked` at **49.923 seconds**,
overwriting the requested position, then stopped advancing. This reproduces the same
unbuffered-seek failure class as the reported jump to seven minutes; it is not a page-scroll
test and does not establish the exact original interaction.

## Implemented contract

- Preserve the requested `currentTime` and `seeking` state while the active-track buffered
  intersection does not contain the target. Set `readyState` to `HAVE_METADATA` and emit
  `waiting` when playing becomes blocked.
- Suspend native playback without changing the author's `paused` state or emitting a
  spurious `pause`. Ignore old-position clock/EOF notifications during the pending seek.
- After both tracks cover the target, seek the native decoders and wait for their response
  before `seeked`. Resume only if the author has not paused; deferred play promises remain
  pending until playback is acknowledged.
- Give each seek its own request identity. A superseded seek's completion cannot rewind the
  new seek. Pausing rejects deferred play promises; `load()` and media errors cancel seeking.
- Reset the SourceBuffer parser on `abort()` even between append calls: discard partial
  input, retain accepted ranges and initialization state, and reset the append window.
  Aborting from `updatestart` prevents that operation from subsequently parsing its bytes.
- Scope `load()` retirement to the element owning the decoder. Loading an unrelated audio
  element must not pause another element's video and audio clock.
- Convert fragmented-MP4 presentation start and end timestamps to the native timebase, then
  derive duration from their difference. Truncating duration separately invented 100 ns
  gaps in contiguous video and triggered unnecessary player recovery seeks.

This implements the waiting portion of [MSE seeking](https://www.w3.org/TR/media-source-2/#mediasource-seeking)
and [HTML seeking](https://html.spec.whatwg.org/multipage/media.html#seeking). It does not claim
complete MSE conformance, gapless playback across arbitrary discontinuities, or support for
new codecs. No site-specific seek behavior, larger byte budget, or increased decoder timeout
is used.

## Regression evidence

- The 420-second script regression fails before the change because a native seek is sent
  while only the first second is buffered. It passes after the change, including separate
  audio/video arrivals, stale clock notifications, and completion while paused.
- Additional script tests cover superseded seek acknowledgements and pause during a pending
  play promise.
- The existing licensed one-second fragmented MP4 fixtures are retimestamped in memory to
  seven minutes. Native Media Foundation tests decode actual video and PCM after seeking
  into that disjoint segment.
- A hidden, silent isolated-renderer test seeks to **420.5 seconds**, appends video and audio
  separately, verifies `HAVE_METADATA` while audio is missing, then observes `seeked`, clock
  advancement, and subsequent video-pixel delivery.
- A second isolated-renderer test loads an unrelated audio element from the resumed video's
  `playing` listener. Before ownership isolation both frames and the clock stop; afterward
  both continue.
- Parser-reset and fractional-timebase regressions fail before their respective fixes and
  pass afterward. No buffer-limit increase or speculative gap tolerance is involved.

## Request-copy compatibility

The live trace also exposed a Fetch bug. A Request internally addressed to a `data:` URL
had author-overridden public URL/method properties. Copying those getters together with the
internal empty body incorrectly sent a bodyless POST to the apparent network URL, producing
HTTP 400. Request construction, cloning, and serialization now use the stored request state,
not shadowed public attributes. Explicit RequestInit overrides still apply. Regressions cover
throwing public getters, body/header retention, abort propagation, and explicit overrides.
This follows the [Fetch Request constructor](https://fetch.spec.whatwg.org/#dom-request),
not a rule for a particular origin. The erroneous player-API HTTP 400 disappeared in the
clean live retest.

## Live evidence and remaining failure

Hidden, silent release tests on 2026-09-09 used the reported video. Starting directly at
420 seconds reached **444.953 seconds**, with **864 painted frames over 24.724 seconds**,
active playback, and no native media failure. This is real frame advancement, but it is not
the same operation as seeking after playback has started. The silent worker clock is not
proof of physical audio output; the native fixture separately verifies decoded PCM.

Seeking after initial playback still encountered HTTP 403 and, on some runs, DNS failures
from media hosts, followed by the site's own player reset. That case remains unresolved.
Headless Chrome advanced from 420 to about 440 seconds with both its usual identity and
Breeze's user-agent string. Those captures reported a temporary-profile cleanup failure,
so their playback observations are retained without counting the complete harness runs as
passes. No request replay or changes to filtering, TLS validation, or access policy are
included in this change.

An HTTP 200 document or a successful benchmark process alone is not evidence that video
playback succeeded. The complete seek-after-playback scenario must pass before this incident
can be called resolved.

Additional contracts: [MSE reset parser state](https://www.w3.org/TR/media-source-2/#sourcebuffer-reset-parser-state),
[SourceBuffer.abort](https://www.w3.org/TR/media-source-2/#dom-sourcebuffer-abort), and
[HTML media load](https://html.spec.whatwg.org/multipage/media.html#dom-media-load).
