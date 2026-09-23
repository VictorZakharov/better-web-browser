# HTML5test capability slice: events and scrolling

This slice targets two missing web-platform APIs, not HTML5test-specific behavior.
The 2026-09-23 fresh-profile hidden release run scores **323 / 588**, up from
[317 / 588 on 2026-09-22](https://github.com/VictorZakharov/better-web-browser/commit/157ee94).
The score increase is six points: five for `EventSource` and one for
`Element.scrollIntoView()`. It is a capability signal, not a claim of full
specification compliance or pixel parity with another browser.

## Server-sent events

The implementation follows the [HTML server-sent events model](https://html.spec.whatwg.org/multipage/server-sent-events.html):

- `EventSource` is exposed on Window and dedicated workers, with EventTarget
  events and the `CONNECTING`, `OPEN`, and `CLOSED` states.
- The transport uses the engine's Fetch stack and incremental `ReadableStream`
  body, so events can arrive before EOF. Requests include `Accept:
  text/event-stream`, use no-store cache mode, and follow Fetch's CORS and
  credential modes.
- An incremental UTF-8 decoder and line parser handle split code points,
  optional BOM, CRLF/CR/LF, comments, `data`, `event`, `id`, and `retry` fields.
  Events expose origin, last event ID, and trusted browser dispatch.
- Disconnects enter the reconnect state. Reconnect requests send
  `Last-Event-ID`; `close()` aborts the transport. Invalid MIME type and HTTP
  204 close the source. Message size and queued-event limits protect the
  browser from unbounded streams.

Tests cover split chunks, malformed and terminal responses, disconnect and
reconnect, worker decoding, and a real loopback HTTP stream that remains open
while the document processes its events.

Remaining work: task-source scheduling currently uses the engine's generic
timer queue; the HTML standard's dedicated remote-event task source and all
possible Fetch redirect/CORS edge cases are not yet independently covered by
Web Platform Tests. Shared-worker exposure is also not part of this slice.

## Scroll into view and scroll spacing

The implementation follows [CSSOM View's scroll-into-view algorithm](https://drafts.csswg.org/cssom-view/#scroll-an-element-into-view)
and [CSS Scroll Snap's scroll-spacing definitions](https://drafts.csswg.org/css-scroll-snap-1/#scroll-padding):

- `Element.scrollIntoView()` accepts the legacy boolean and options dictionary,
  including block/inline alignment and `container: "nearest"`.
- It scrolls nested element boxes from the innermost outward, then the vertical
  viewport, using existing geometry and scroll clamping. A detached or
  unrendered target does not move the viewport.
- Physical `scroll-margin` and `scroll-padding` shorthands/longhands participate
  in cascade, CSS-wide keywords, `CSS.supports()`, computed CSSOM values, and
  scroll-into-view positioning. Percentages for scroll padding use the
  corresponding scrollport dimension.

Tests cover nested two-axis and viewport positioning, legacy options,
already-visible targets, option validation, CSS cascade and computed values,
and interaction with scroll margin/padding.

Remaining work: the host does not expose horizontal viewport scrolling, so
that final alignment is vertical only. The `smooth` behavior option is
currently accepted but moves immediately. Logical scroll-spacing properties,
scroll snapping, and some CSS length forms are outside this slice. Future work
should address these shared platform gaps rather than adding site-specific
positioning rules.

The [README benchmark command](../README.md) reproduces the score using the
release build in a hidden window. The source-size, format, library, renderer
process, and live-runtime suites provide separate regression gates.
