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

### MediaSource event tasks

MediaSource lifecycle and SourceBuffer events previously used microtasks. That ran
`sourceopen` before promises queued by the attaching script, and ran `update` and
`updateend` without a microtask checkpoint between their listeners. They now use the
renderer event loop's media task source. SourceBuffer `updatestart` and asynchronous
parsing are separate tasks; an abort during the intervening microtask checkpoint
invalidates the pending parser operation. Already queued events remain queued.

The internal queue does not call author-replaceable `setTimeout` or `queueMicrotask`,
and author timer cancellation cannot remove media tasks. The normal bounded task
executor supplies microtask checkpoints and rendering opportunities. Native acceptance
still controls `updateend`: dispatching an event is not proof that the decoder accepted
bytes. Independently processed tracks may produce separate transfers; acknowledgements
release the next transfer without resubmitting already accepted bytes.

Four focused regressions cover attach/promise order, a checkpoint after each buffer
event, abort from an `updatestart` promise, and internal task ownership. Existing
fixtures now explicitly advance queued tasks after worker acknowledgements instead of
assuming synchronous event delivery. This is an MSE event-scheduling correction, not
complete HTML media-event or MSE conformance, and it did not resolve the live failure.

Contracts: [MSE appendBuffer](https://www.w3.org/TR/media-source-2/#dom-sourcebuffer-appendbuffer),
[MSE range removal](https://www.w3.org/TR/media-source-2/#sourcebuffer-range-removal),
and [HTML media tasks](https://html.spec.whatwg.org/multipage/media.html#queue-a-media-element-task).

### MediaSource reset and replacement ownership

The September 12 failed-seek trace exposed a separate recovery defect: after `load()`,
the element reported no resource, but its old MediaSource remained open with its old
audio/video buffers. Reset now detaches that source, clears both SourceBuffer lists and
its duration, invalidates pending buffer work, and releases retained parser data.
Old buffers cannot append again even if the same object URL is subsequently reattached.

Pending play promises are rejected with `AbortError`. Resource-specific queued element
events and late acknowledgements for old native commands cannot change a replacement.
Old clock updates are ignored while the replacement has no metadata. Source assignment
through the property, `setAttribute`, or the null-namespace attribute APIs shares the
same lifecycle path; resetting one element does not detach another element's source.

Three regressions failed before the change: load left the old source usable, replacement
left its play promise unresolved, and an in-flight append remained active after detachment.
Additional cases cover queued events, same-URL reattachment, another element's ownership,
and an old native load acknowledgement arriving after replacement.

The hidden, silent Chrome comparison observed `closed`, empty source/active lists,
`NaN` duration, `updating === false`, and `InvalidStateError` on another append. Its
event sequence was `updatestart`, active-list removal, `abort`, `updateend`, source-list
removal, `sourceclose`; the browser run and profile cleanup completed without errors.
This corrects reset/recovery ownership, **not** the cause of the tiny live refill response.

Contracts: [HTML load](https://html.spec.whatwg.org/multipage/media.html#concept-media-load-algorithm)
and [MSE detachment](https://www.w3.org/TR/media-source-2/#mediasource-detach).

### Unequal track starvation and media callback side effects

The follow-up audio-keeps-playing report exposed two additional engine defects:

- Media-clock event callbacks ran after the renderer had drained side effects. Their
  fetch, worker, storage, and media commands could be discarded when creating the runtime
  report. The checkpoint now admits those effects through the normal input-outcome path.
  Media acknowledgement callbacks are drained before collecting their resulting effects.
- Reaching the video-buffer end only lowered `readyState`; audio could continue through
  its longer buffer. [MSE SourceBuffer Monitoring](https://www.w3.org/TR/media-source-2/#sourcebuffer-monitoring)
  now holds the timeline at the active-track intersection and stops native playback.
  Further audio-only appends cannot resume it. Once both tracks cover the held time,
  generation-tagged recovery restores that position and resumes unless the author paused.
  Buffering does not manufacture `pause`, `seeking`, `seeked`, or `ended` events.

The existing coded-frame fixtures provide a deterministic test around 4:30: one video
segment and over twenty seconds of audio. It observes `waiting`, a stopped native clock,
unchanged author-facing position after an audio-only append, and resumed video pixels after
video arrives. Fetch requests from both the `playing` acknowledgement and `waiting` callback
must also reach the broker. The test failed with the discarded-callback path and passes with
normal side-effect admission. Script regressions separately cover author pause, stale clock
replies, and ended-track range extension without filling internal gaps.

This fixes synchronization when buffered input runs out; it does not repair an HTTP 403,
provide a missing codec, or establish uninterrupted live playback. Clock suspension occurs
at a renderer media checkpoint, not on the audio device's real-time callback.

### Live observations

Hidden, silent release tests on 2026-09-09 used the reported video. Starting directly at
420 seconds reached **444.953 seconds**, with **864 painted frames over 24.724 seconds**,
active playback, and no native media failure. This is real frame advancement, but it is not
the same operation as seeking after playback has started. The silent worker clock is not
proof of physical audio output; the native fixture separately verifies decoded PCM.

Some seek-after-playback runs recorded HTTP 403 or DNS failures as well as the site's own
player reset. Those observations alone do not establish a causal relationship. On
2026-09-12, redacted request timing showed that the six GET/itag-18 HTTP 403 responses
occurred during startup, roughly twenty seconds before the seek. A successful seek
also had those startup failures. The prior approved replay of one such GET in signed-out,
headless Chrome also returned 403, but it did not test the failing post-seek POST.

Two baseline keyboard-seek runs resumed at about 601.536 s and continued to 640.662 s
and 635.730 s. Their post-seek POST responses delivered about 3.7 MB followed by further
refills. With the event-task correction, another run failed: every post-seek POST returned
HTTP 200, but delivered only 168 or 170 bytes. No post-seek frames were presented, and the
site reset the player about twenty seconds later. The screenshot confirms the player's
error screen. Byte counts do not identify the response semantics or prove that the
outbound request was correct. The cause of this intermittent failure remains open.

### Approved replay of the actual post-seek POST

A subsequent hidden, silent run reproduced the failed 601.533-second seek (298 painted
frames, all before the seek). With explicit user approval, a one-shot benchmark-only
probe captured a post-seek media POST and its small response. Cookie, Cookie2,
Authorization, and Proxy-Authorization headers were excluded before writing.

The 3,560-byte request body and signed URL were replayed once from a fresh, signed-out
Chrome 152.0.7977.83 page on the same document origin. Chrome generated its own browser-
controlled headers; Fetch used `credentials: omit`, CORS mode, and no-store cache mode.
The capture was about 25.7 seconds old at comparison time. CDP recorded the actual POST
response, not merely a preflight:

| Observation | Breeze original | Chrome replay |
| --- | --- | --- |
| Method / request-body bytes | POST / 3,560 | POST / 3,560 |
| HTTP status | 200 | 200 |
| Response-body bytes | 168 | 168 |
| Response body comparison | Identical SHA-256 | Identical SHA-256 |

The common response SHA-256 was
`7147E9143A7F830615AB9A66B4AE133ECD4BA95368047FAC6B9CEC19CE0420C0`.
Chrome reported `application/vnd.yt-ump`, HTTP/1.1, and a CORS response. No cookie or
authorization header was replayed. Its hidden-browser run and profile cleanup completed
without errors. The temporary signed request/response capture was deleted immediately
afterward, and all capture/replay probes were removed from the browser and harness.

For this request, Breeze was not truncating a larger server response: Chrome received the
same bytes. This does **not** prove that Breeze constructed the correct request/session
state, identify the meaning of the binary response, or establish full YouTube playback
parity. The next investigation is the state/inputs that lead to the failed refill, not a
speculative decoder timeout or an attribution to startup GET 403s.

### Native Chrome seek controls

Further hidden, silent comparisons on September 12 exercised the normal Chrome player,
not a replay of a Breeze request. Each run used Chrome 152.0.7977.83 with a fresh,
signed-out profile, started playback, waited ten seconds, then sought forward and
observed another twenty-five seconds. The reported `972vfNAiz5M` video stalled at
601.533 s and reset to the site's error screen in these reference runs too.

| Reference condition | Observation |
| --- | --- |
| Media event/buffer trace | Playback before seek; no recovered seek, then player reset |
| Repeat without wrapping media APIs | Same failure at 601.533 s; `seeking` and `waiting` reported readyState 1 |
| Other reported video, `kdtdVTxvjD4` | Same failure at its 50% position, 240.680 s; this is not an exact 270 s scrub |
| Normal Chrome user-agent string override | Same failure; user-agent text alone did not restore playback |
| Normal caching, no injected page instrumentation | Same player reset |
| Progress-bar midpoint pointer press/release, normal caching, no page instrumentation | Same player reset; this was a click, not a drag |

Chrome itself generated the requests. Network observations recorded HTTP/3 and the same
pattern of a 168-byte post-seek response followed by repeated 170-byte responses, each
with HTTP 200 and `application/vnd.yt-ump`. Counts sum CDP `Network.dataReceived.dataLength`,
not encoded transfer totals. The diagnostic retained only media state, byte counts,
method/status/protocol, and host names: no signed queries, headers, or body contents.
All temporary instrumentation and its compiled harness hooks were removed afterward.

Every run above reported a temporary-profile cleanup failure (`Account Web Data` access
denied), including attempts with graceful browser shutdown. Playback observations remain
useful, but none of these complete harness runs is counted as a pass. Screenshots confirm
the player error surface. No user browser session or authenticated profile was used.

This prevents attributing this reproducible failure solely to Breeze's decoder or lack
of HTTP/3. It does **not** identify the response semantics, establish a YouTube outage,
explain the reference-session difference, or prove Breeze's implementation correct.
The user subsequently confirmed that both regular signed-in Chrome and signed-out
Incognito play this video and resume a manual seek to 10:01 in under one second.
That is user-observed latency, not an automated measurement. Sign-in alone therefore
does not explain the discrepancy. A successful equivalent automated reference remains
needed to compare internal state; starting a timestamped URL is not a seek test.

### Approved visible Chrome comparison

On September 12 the user approved one visible, muted Chrome comparison in a fresh
temporary profile. It used installed Chrome 152.0.7977.83 with `--mute-audio`, an isolated
user-data directory, and a console-free launch. No personal profile was opened/copied.
CDP observed native media state without wrapping player/media APIs and sent the normal
`5` player key after ten seconds of playback. Browser visibility was the explicit exception
to normal hidden-only tests, not a new default in the checked-in benchmark harness.

First nonzero frame counts appeared at about 63.73 s after diagnostic launch. The seek
was sent at 75.68 s; all subsequent seeking samples stayed at 601.533 s with 318 frames,
and the site displayed its error screen at 84.26 s. The user independently captured that
same visible error. Post-seek media POSTs returned HTTP 200 over HTTP/3 with only 190/192
encoded transfer bytes; those CDP transfer totals are not the earlier decoded-body counts.
The window viewport changed during the run, so this was not a controlled layout/timing
benchmark. No raw signed URLs, request/response bodies, or authentication headers were saved.

The owned browser closed, but deletion of its temporary profile again reported access
denied for `Account Web Data`. This is a failed playback comparison with incomplete
profile cleanup, not a passing harness result. The executing Windows identity was the
normal, non-elevated user. Headless mode alone therefore does not explain the difference
from the user's successful normal and Incognito Chrome seeks; the reference-environment
difference and Breeze's live seek incident remain unresolved.

### Pressed-button state during scrubbing

A separate isolated-renderer regression reproduced `mousedown.buttons=1`, followed by
`mousemove.buttons=0` while the mouse was still pressed. The renderer previously derived
the mask from the event phase, losing held state during movement and other-button releases.
The shell now carries Win32's post-event mask through versioned IPC. Movement coalescing
preserves button/modifier transitions, and each mouse button retains independent click
ownership. A release observed outside content clears stale ownership on return; focus loss
and document suspension also clear it. This does not deliver a lost release outside the
window or implement pointer capture.

The owned `tests/fixtures/pointer-buttons.html` fixture was compared with silent headless
Chrome 152.0.7977.83 using actual press, move, and release input. Both completed without
browser/profile-cleanup errors. Primary/secondary chords and middle-button movement match:
`buttons` retains all held buttons, unchanged `pointermove.button` is -1, and additional
button transitions use `pointermove` while legacy mouse events report each press/release.
Legacy `mousemove.button` remains 0, as in Chrome and the current
[mouse-event initialization algorithm](https://www.w3.org/TR/pointerevents4/#set-mouseevent-attributes-from-native).
The [chorded-button contract](https://www.w3.org/TR/pointerevents3/#chorded-button-interactions)
and [Win32 message state](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-mousemove)
define the mapping. IPC major version 5 rejects older peers rather than misreading the
added mask byte. This slice covers the shell's three routed mouse buttons, not complete
Pointer Events conformance, touch/pen input, or compatibility-mouse suppression.

This fixes a demonstrated generic drag-input defect. It is not evidence that the
intermittent 168/170-byte media refill or the live YouTube seek failure is resolved.

### Earlier comparison and overall limits

The related API audit found a separate [Web Crypto randomness defect](web-crypto-randomness.md):
integer elements wider than a byte received only 8 random bits, randomness used `Math.random`,
and workers had no crypto API. The OS-backed replacement has cross-engine and isolated-renderer
regressions. This is a demonstrated standards/security correction, not proof that the live
media refill failure is resolved.

A subsequent audit reproduced an independent [XHR cancellation/reuse defect](xhr-request-lifecycle.md):
callbacks from a canceled request could reset a new request on the same object. Ownership
is now checked across promise/reader continuations and reentrant events. Its isolated tests
pass, but a connection to the live YouTube refill failure is not established.

Headless Chrome advanced from 420 to about 440 seconds with both its usual identity and
Breeze's user-agent string. Those captures reported a temporary-profile cleanup failure,
so their playback observations are retained without counting the complete harness runs as
passes. Temporary timing/replay probes and signed request captures are not shipped.
No changes to filtering, TLS validation, or access policy are included in this change.

An HTTP 200 document or a successful benchmark process alone is not evidence that video
playback succeeded. The complete seek-after-playback scenario must pass before this incident
can be called resolved.

Additional contracts: [MSE reset parser state](https://www.w3.org/TR/media-source-2/#sourcebuffer-reset-parser-state),
[SourceBuffer.abort](https://www.w3.org/TR/media-source-2/#dom-sourcebuffer-abort), and
[HTML media load](https://html.spec.whatwg.org/multipage/media.html#dom-media-load).
