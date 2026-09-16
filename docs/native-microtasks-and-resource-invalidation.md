# Native microtasks and nonvisual resource completion

## Bootstrap diagnosis

A hidden release run of modern DuckDuckGo reached the two-second script watchdog
in its main module and again during DOMContentLoaded. Native bridge timings did
not account for those stalls. Temporary V8 safepoint stack sampling repeatedly
found the cycle `author Promise.then -> window.queueMicrotask -> author Promise.then`.
The sampler was removed after diagnosis; no watchdog policy was changed.

The old bootstrap implemented `queueMicrotask` using the page's global
`Promise.resolve().then`. A page can replace that constructor or its methods,
including with a scheduler that itself calls `queueMicrotask`. Browser-internal
scheduling must not invoke those replaceable author APIs.

The [HTML microtask-queuing contract](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#microtask-queuing)
now goes directly to V8's microtask queue in both Window and dedicated workers.
Validation remains synchronous; callbacks run asynchronously, receive no arguments,
and have their return values ignored. Exceptions are reported as cancelable global
error events, not converted into rejected promises. Existing Promise jobs and
MutationObserver delivery share the native queue. Internal mutation and slot-change
notifications no longer depend on author-replaceable Promise methods either.

Original unit regressions exercise Promise replacement, patched `then`, FIFO/nested
jobs, callback receiver/arguments, ignored thenables, and exception identity in both
Window and worker contexts. Another regression replaces both public scheduling APIs
while observing mutation and slot delivery. The original `native-microtasks` visual
fixture uses a minimal author scheduler and requires a visibly populated ready state
in Breeze and headless Chrome. The alpha gate now explicitly checks the fixture's
ready marker in both browsers; nonblank content without completion is insufficient.
Existing upstream microtask WPT cases remain unmodified;
the curated `.any.js` adapter runs the Window variant, with workers covered by the
owned runtime tests, not an implied worker-WPT run.

This fixes a browser bug, **not all modern DuckDuckGo compatibility**. The live app
then encounters a caught `ReferenceError: DOMParser is not defined` and renders only
a partial shell. Console errors and captures must be checked even when the report's
uncaught-error list is empty. [Issue #89](https://github.com/VictorZakharov/better-web-browser/issues/89)
remains open; the HTML search fallback is unchanged. DOMParser requires its own
standards-based implementation and acceptance, not a DuckDuckGo-specific patch.

## Resource completion is not automatically a visual change

Downloaded script source was treated as a presentational resource change before
execution. Merely dispatching a resource load/error event also requested a layout,
even when its handler only logged a message or queued another request.

Source installation and event delivery now wake runnable work separately from
visual invalidation. Real script/handler DOM mutations retain their normal
invalidation; images, stylesheets, fonts, and media retain their visual path.
Document-load and parser readiness still advance after nonvisual completions.

Renderer-process regressions require execution and load notification without a
presentation for an otherwise unchanged document, a later presentation for actual
DOM changes, and recovery-script fetching/execution after an error handler.
The full async/deferred/parser, stylesheet, document-lifecycle, and live-runtime
suites exercise the surrounding readiness and resource-event contracts.

This removes one confirmed unnecessary-layout path. It is not a claim that every
zero-style-change layout can be skipped: intrinsic image/font geometry and DOM
structure can change independently of computed-style differences. Remaining
Wikipedia work must continue to be profiled with those distinctions intact.
