# Viewport scrolling compatibility

The document's normal-flow height is not its scrollable overflow. Absolutely positioned
application roots can extend thousands of pixels below a short body without increasing
the body's normal-flow height. Limiting native scrolling to that height makes these
documents appear unscrollable.

The layout pass now includes descendant border-box overflow in the vertical viewport
extent, including absolute and relative positioning. It excludes viewport-fixed subtrees
and clips descendant overflow at supported overflow-clipping ancestors. Paint-only ink,
such as shadows, is not used to create document scroll range.

This follows the distinction in [CSS Overflow](https://www.w3.org/TR/css-overflow-3/#scrollable).
It is not a claim of complete CSS Overflow support: nested `overflow:auto`/`scroll`
containers, axis-specific overflow, and transformed fixed-position containing blocks
need their own end-to-end implementations.

## Script-visible viewport offsets

`Document.scrollingElement`, `Element.scrollTop`, and `Element.scrollLeft` now expose
the viewport's current offsets, including native scroll updates. Standards mode maps
the root element to the viewport; quirks mode handles the body's potentially-scrollable
test. Detached/inactive elements and elements without implemented scrolling boxes
return zero. Overflow values retain separate axes and their computed-value interaction
instead of serializing every value as either `hidden` or `visible`.

Vertical root/body setters and `window.scroll`, `scrollTo`, and `scrollBy` reach the
native viewport through a bounded, document-owned renderer report. Requests coalesce
to the latest offset, including zero. The host normalizes non-finite values and clamps
to the same scrollable overflow extent used by retained layout; synchronous geometry
therefore retains that extent even though it omits paint output. Script getters update
synchronously and scroll events are coalesced asynchronously. Native acknowledgements
of an unchanged offset do not deliver duplicate events.

This is a vertical viewport slice of [CSSOM View](https://drafts.csswg.org/cssom-view/#dom-element-scrolltop),
not complete element scrolling. Horizontal native scrolling, nested scroll containers,
independent per-axis paint clipping, scroll snapping, and animated script scrolling
remain unsupported. A request for smooth scrolling currently uses instant movement,
which CSSOM View permits a user agent to do. No per-element JavaScript offset cache
pretends that content moved when it did not.

Tests cover numeric reads during custom-element activation, standards/quirks modes,
fractional offsets, native input, clamping, conversion errors, request/event coalescing,
and checked protocol serialization. A hidden native integration fixture checks pixels
after a script scrolls an absolutely positioned document beyond its short body.

### September 8 controller failure

Temporary live tracing found that the watch page's `active` property and Redux state
were both true, but its active-module list remained empty. Its first module read a
missing `scrollTop` property and dispatched `SET_WATCH_SCROLL_TOP` with `undefined`.
The site's reducer rejected that value before the title-update and comments modules
could be constructed. The site caught the exception, explaining why ordinary uncaught
JavaScript-error counters were empty. The temporary site-specific probes were removed;
the fix is the general viewport interface above.

Fresh-profile, hidden release captures at 1520 x 1000 then produced these results:

| Observation | Before this slice | After this slice |
| --- | --- | --- |
| Watch-page controllers | Activation stopped in the scroll-state reducer | Initialization proceeds; comments mount |
| Comments after scrolling | Zero comment threads; comments element remained hidden | 18 threads on the initial page, 15 on the next video; comments visible in captures |
| Video-to-video navigation | Browser/document title remained from the first video | Address, both titles, visible heading, channel, and description match the new video |
| Remaining visible defects | Incomplete initialization obscured downstream failures | Comment text is misplaced; counters and some controls are malformed |

The navigation capture changed from `cvkDcrbLP2s` to `HQ91OXIK34I`. It still reported
template-stamping errors (`__dataHost` on an undefined node). Media counters advanced,
but the screenshot was scrolled to the description/comments, so it is not evidence of
smooth visible playback. These captures are compatibility observations, not a controlled
speed comparison or completion of the controls, layout, or playback acceptance criteria.

Native viewport scrolling updates the script-visible window offsets and dispatches a
trusted `scroll` event at `Document` that bubbles to `Window`, as required by
[CSSOM View](https://www.w3.org/TR/cssom-view/#scrolling-events). Element scroll events
have different bubbling semantics; this change does not implement element scrollers.

Regression coverage includes positioned overflow, constrained visible/clipped content,
fixed content, and a hidden native-wheel integration test. The integration test verifies
that a window-level listener observes an offset greater than 500 pixels on an absolutely
positioned application fixture.

## September 7 live diagnosis

A YouTube watch-page capture had a 680-pixel body and an absolutely positioned application
root approximately 4461 pixels tall. The old body-based extent explained the zero native
scroll range. This is a standards-level layout defect, not a reason to special-case YouTube.

Playback is a separate, unresolved acceptance condition. A longer silent capture painted
approximately 36.8 video frames per second before the site's handled-error path called
`stopVideo()` and reset the media element at approximately 49.8 seconds, while buffered
media remained. This identifies the reset path, not the underlying error or a proven
missing media standard. Startup-to-audio latency and sustained playback must still be
measured and fixed independently; passing scrolling tests does not complete issue #121.

The corrected live watch-page scroll trace reached nonzero offsets (up to 362 native
pixels in that response), with 7.5 ms p95 input-to-paint latency during its six-second
sampling window. The owned fixture independently tests native wheel-message delivery;
the live trace exercises the shared scroll-position/presentation path directly.

The final live screenshot still had a blank player despite active media and completed
offscreen paint calls. These counters are therefore not proof of visible playback.
Opt-in image diagnostics now include `clipped_paint_rects`, bounded to eight rectangles,
to distinguish decoded image geometry from geometry surviving retained overflow clips.
They do not assert visibility through opacity layers or later opaque paint occlusion.
A subsequent capture retained the full video rectangle after clipping, so overflow
clipping alone does not explain that blank-player reproduction.
