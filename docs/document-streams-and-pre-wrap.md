# Document streams and preserved wrapping

Updated 2026-09-16. This is one combined change with two separate contracts:
script-created document input streams and CSS `white-space: pre-wrap`. It does not
claim complete browser compatibility or a measured page-loading speedup.

## Script-created document streams

The implementation follows HTML's [dynamic markup insertion](https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html)
and [end-of-parsing lifecycle](https://html.spec.whatwg.org/multipage/parsing.html#the-end).

- `document.open()` reuses the Document and JavaScript realm, removes the old tree,
  clears listeners/handlers on its shadow-including descendants and Window, resets
  readiness and compatibility mode, and creates a retained html5ever parser.
  Globals, timers and unrelated detached-node listeners survive.
- `write()` and `writeln()` without an insertion point perform this replacement
  implicitly. They no longer append a parsed fragment to a completed document.
  Split tokens, immediate DOM reads and nested writes share the real tokenizer.
- `close()` supplies EOF only to a script-created parser. Closing inside a nested
  script cannot skip the remainder of the outer written input.
- Written external scripts and stylesheets use the existing asynchronous resource
  and parser-blocking gates. Both successful and failed scripts resume parsing.
  Replacing a page discards old parser-script owners and old load blockers.
- Active parser-script calls to `open()`/`close()` do not replace the network stream.
  XML documents and parser-created custom-element construction reject dynamic
  insertion. External classic and module evaluation suppress destructive writes
  when there is no insertion point; writes at an active insertion point still work.
- Inert HTML documents have independent parsers and compatibility modes and do not
  execute their written scripts. They cannot replace the primary document.

Parser execution and host borrowing remain separate: author callbacks run after
native borrows end. Existing parser/node/depth/write/script budgets still apply.
The renderer collects replacement state before selecting subsequent tasks, avoiding
stale parser, stylesheet and focus state without destroying the realm.

### Explicit Chromium timing difference

Breeze keeps HTML's queued `DOMContentLoaded` and window `load` tasks. In the owned
fixture, headless Chrome 153 instead fires DCL and reaches `complete` inside
`close()`; `load` follows later, or is suppressed when already dispatching the
original load event. Chromium's [Document::close implementation](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/third_party/blink/renderer/core/dom/document.cc)
explicitly notes that its completion handling should follow the specification more
closely. This is a documented timing difference, not claimed event-for-event parity.
The browser comparison verifies the shared lifecycle ordering and displays the
`close returned` marker; native integration tests assert Breeze's specified task order.

## CSS preserved wrapping

`pre-wrap` is distinct from `pre`: both preserve spaces and explicit line breaks,
but only `pre-wrap` allows soft wrapping. Collection, wrap opportunities, intrinsic
sizing and painting use the same distinction. Preserved trailing spaces hang at
soft wraps; at a forced or final line end they hang only when they do not fit.
Long words remain unbroken unless another supported breaking rule applies.

The source is [CSS Text 3, white-space processing and line breaking](https://www.w3.org/TR/css-text-3/#white-space-property).
Regression tests cover preservation, empty lines, leading spaces, trailing-space
hanging/alignment, inline boundaries, resizing, long words and intrinsic sizing.
This does not add `pre-line`, `break-spaces`, configurable tab stops, full Unicode
line breaking or bidi layout; those remain separate text-layout work.

## Verification

- Twelve hidden live-renderer tests cover replacement, inert/XML guards, readiness,
  split input, nested close, external success/failure, stylesheet blocking,
  destructive-write suppression, old parser cancellation and readiness re-entry.
- Three unchanged upstream WPT files at the existing pinned revision add eleven
  assertions: `opening-the-input-stream/002.html`, `custom-element.window.js` and
  `type-argument.window.js`. Upstream files and their BSD license stay in the
  external WPT checkout; no copied fixtures or expected failures are added.
- `benchmarks/alpha/fixtures/document-streams.html` is an original combined fixture.
  Serve it with the hidden fixture server, run Breeze through
  `scripts/run-hidden-benchmark.ps1 -FreshProfile -DeviceScaleFactor 1`, and compare
  with the unified-headless, muted Chromium harness at the same content viewport.
  `#result[data-contract=pass]`, the rendered trace and the wrapped text are required.
  The earlier `synchronous-parser-writes.html` fixture remains a regression check.

## Remaining boundaries

Nested browsing-context/realm navigation, unload/aborted-parser handling, CSP and
Trusted Types are not completed here. The three-argument `document.open()` overload
requires auxiliary browsing contexts and explicitly reports `NotSupportedError`
(or `InvalidAccessError` without a Window); it is not a document-replacement call.
Parser-originated MutationObserver records, per-token custom-element reaction timing
and asynchronous module continuation policy still require dedicated coverage.
