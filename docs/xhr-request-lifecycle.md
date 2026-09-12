# XMLHttpRequest cancellation and reuse

`open()` may replace an in-flight request on the same `XMLHttpRequest`. Its old Fetch
promise, body reader, and timeout must not complete or fail the replacement. Event
handlers may also reenter `open()`, `send()`, or `abort()` while a response is delivered.

Breeze previously checked only a shared `send` boolean. A replacement set that boolean
back to true before the previous abort rejection or body-reader continuation ran. The
old callback then reset the new request. Aborting from `loadstart` also dereferenced a
cleared controller, and `abort()` unconditionally reset a replacement created in its
own abort handler to UNSENT.

The request's unique AbortController now identifies callback ownership. Response
processing checks that identity after awaited reads and author callbacks. Canceling or
reopening invalidates ownership before aborting Fetch. Initial load events may cancel
the operation before Fetch starts. Terminal error state is established before exposing
reentrant events; `abort()` returns to UNSENT only when the object is still DONE.
The response consumer is a separate shared module used by Window and Worker bootstraps.

LOADING now starts when bytes arrive, not when headers arrive, and its readystatechange
notifications accompany incremental progress. An empty response skips LOADING. This
change does not implement synchronous XHR or claim complete XHR/Web IDL conformance.

## Sources and tests

The contracts come from WHATWG's [open](https://xhr.spec.whatwg.org/#the-open()-method),
[send/response processing](https://xhr.spec.whatwg.org/#the-send()-method),
[abort](https://xhr.spec.whatwg.org/#the-abort()-method), and
[request error steps](https://xhr.spec.whatwg.org/#request-error-steps).

Five initial script regressions failed before the fix: immediate replacement, restart
inside an abort listener, replacement from headers/progress, and abort from loadstart.
The final tests additionally cover loadstart replacement, empty-body state ordering,
Worker reuse, upload cancellation/retry, and two hidden AppContainer renderer tests. The latter verify actual
Fetch cancellation IDs and successful replacement response delivery through IPC.

`tests/fixtures/xhr-reuse.html` and its sibling script/text files are an owned, loopback-
served browser comparison, not a live-site test. On 2026-09-12, hidden Breeze passed all
six cases with no script errors or renderer exits. Hidden Chrome 152.0.7977.83 passed
five of six with no browser or profile-cleanup error, but failed the fixture's overall
readiness assertion: immediate reopen/send inside HEADERS_RECEIVED returned `old\nnew`
instead of `new`. This also occurred when the old HTTP body was delayed 75 ms after
flushing headers. The stricter isolation assertion is retained, and that reference run
is explicitly **not** reported as an all-passing compatibility comparison.

This defect was found while examining media-request cancellation. Its regression fix
does not establish the cause of the intermittent live YouTube refill failure. See the
[seeking investigation](media-source-seeking.md) for that separate acceptance test.

The first hidden/silent release retest after this correction still failed the reported
10:01 seek: 296 painted frames over 9.827 s were all before the seek, then the site reset
the player. There were no JavaScript errors or renderer exits, and screenshot inspection
confirmed the site's error screen. The browser harness completing is not a playback pass.
