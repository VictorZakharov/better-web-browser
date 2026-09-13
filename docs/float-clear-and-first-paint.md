# Float clearance and stylesheet-blocked first paint

The September 2026 regression work separates two problems: floats collapsing article
text into an overlapping column, and asynchronous stylesheet loading exposing an
unstyled first presentation. Neither fix uses site selectors or loading delays.

## Float layout contract

Physical `clear: none | left | right | both` participates in parsing, CSS-wide
keywords, computed-style serialization, feature queries, and layout invalidation.
Floats are blockified before layout. Their margin-box exclusions belong to the
surrounding block formatting context; ordinary descendants share that context,
while floats, table cells, independent formatting contexts and the root isolate it.

Cleared or non-fitting floats move down without reducing specified widths.
Percentages resolve against the containing block. Auto-width floating wrappers
use shrink-to-fit contributions and respect fixed-width descendants rather than
expanding to their unwrapped text. Normal block boxes retain their width; individual
line boxes avoid floats and recover the space below them.

References: [CSS 2.2 floats and clearance](https://www.w3.org/TR/CSS22/visuren.html#floats),
[floating widths](https://www.w3.org/TR/CSS22/visudet.html#float-width).
This is not complete float/table conformance: margin collapsing with clearance,
mid-line float insertion, complex intrinsic sizing, logical float/clear values and
full table-cell sizing remain separate work.

## First-presentation contract

Pending matching parser-created head stylesheet links block presentation, not the
event loop. Explicit head `blocking=render` links admitted before body insertion
remain blockers after insertion. Failure, removal, disabling, or loss of media
applicability releases the gate. Alternate, disabled, nonmatching and body links
do not implicitly block the first presentation. A 30-second monotonic safety
deadline prevents indefinite blank output.

While blocked, the renderer sends nonvisual runtime updates and remains responsive
to scripts, resource completions and heartbeats. It retains pending presentation
state, image/glyph delivery and geometry-observer work until rendering is allowed.
The browser paints its normal empty background before receiving the first layout.
Empty native windows do not reserve a scrollbar before content creates overflow;
otherwise delaying the first presentation changes CSSOM width and spuriously
redelivers ResizeObserver callbacks when the initial gutter disappears.

This implements a bounded part of the [HTML render-blocking mechanism](https://html.spec.whatwg.org/multipage/dom.html#render-blocking-mechanism)
and [stylesheet processing](https://html.spec.whatwg.org/multipage/links.html#link-type-stylesheet).
Recursive `@import` blocking, complete stylesheet-set selection, render-blocking
scripts and animation-frame/rendering-opportunity alignment remain incomplete.

## Regression evidence

| Reproduction | Before | After |
| --- | --- | --- |
| Cleared right floats | Subsequent boxes squeeze the available width | Boxes stack without collapsing the article column |
| Auto float around a fixed-width table | Unwrapped descendant text inflates the wrapper | Wrapper preserves the fixed-width contribution |
| Head stylesheet delayed 1.5 seconds | Plain content appears before styling | First content frame is styled; scripts and pings remain responsive |

Unit tests cover clearance sides, context boundaries, line recovery, percentages,
intrinsic/replaced sizes, CSS-wide keywords and invalidation. Hidden renderer tests
cover delayed CSS, async scripts, failures, removal, applicability and explicit links.
The owned `float-clear-layout.html` and `render-blocking-stylesheet.html` fixtures
support hidden Chromium comparison and 500 ms filmstrips via
`scripts/serve-alpha-fixtures.ps1` and `scripts/run-hidden-benchmark.ps1`.
Capture artifacts belong in ignored output directories, not source control.
