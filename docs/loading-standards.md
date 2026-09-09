# Loading standards: implementation sequence

Updated 2026-09-09. This is a code-backed loading-pipeline inventory, not a claim
that HTML loading is conformant or a percentage of the web platform implemented.
YouTube is a supplementary compatibility check, not the definition of correctness.

## First slice: parser-prepared external async classic scripts

The renderer now owns a set of prepared **elements**, independent of the shared
resource cache. Fetch completion makes their execution tasks ready. A missing
earlier response cannot block a later ready classic script. One element executes
per renderer checkpoint, with promise jobs completed before selecting the next.

The implemented contract includes:

- Each element executes once, including multiple owners of identical fetched bytes.
- Resource identity retains script kind, credentials mode, and referrer policy.
- Prepared source/element identity survives removal and `src` changes. Moving an
  element into a different document prevents execution in the former document.
- Failed fetches deliver one trusted `error` event to each prepared owner. A
  successfully fetched classic script fires trusted `load` even if evaluation throws.
- Script cleanup drains microtasks before restoring `document.currentScript`;
  the subsequent load handler observes the restored value. The owned fixture was
  checked against Chromium for this ordering, not only against hand-written expectations.
- Pending downloads do not cause repeated renderer clock polling. Completion wakes
  pending work. Newly discovered stylesheets/images can start while another resource
  batch is outstanding; the renderer does not synchronously wait for async classic sources.
- Existing Fetch validation, script byte limits, resource budgets, cancellation and
  process isolation remain in force.

The source contracts are HTML's [script preparation and execution](https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element)
and [cleanup after running script](https://html.spec.whatwg.org/multipage/webappapis.html#clean-up-after-running-script).
Tests cover the queue, Fetch completion order, isolated renderer execution, a withheld
slow response, shared successes/failures, preparation-time identity, and resources
discovered while the original batch is still pending.

This does **not** finish all script loading. Parser-prepared module dependency
loading is now nonblocking; see [deferred scripts and module readiness](deferred-script-readiness.md).
The isolated renderer now retains the tokenizer/tree builder at script boundaries:
prepared async elements can execute while an external classic script pauses parsing.
See [parser suspension, measured evidence, and remaining limits](incremental-html-parsing.md).

## Remaining standards slices

Dynamic external classic readiness and the explicit ordered list are now implemented;
see the [contract, scope, and verification](dynamic-script-readiness.md). Document
lifecycle tasks now have separate readiness and resource gates; see the
[contract, measured evidence, and explicit limits](document-load-lifecycle.md).
Parser-prepared defer/module readiness and rendering opportunities are implemented
in the [next slice](deferred-script-readiness.md), including import failures, shared
cycles, and top-level await. These slices do not establish whole-page parity.

| Order | Contract / observed gap | Existing ownership | Acceptance test for the next implementation |
| --- | --- | --- | --- |
| 4 | Remaining parser work: streaming main-response decoding and synchronous, re-entrant insertion. Script-boundary pause/resume is implemented; the response body is still complete at parser startup, and buffered writes enter the tokenizer after the caller returns. | `document/parsing.rs`, `dom/incremental.rs`, `mutation_host/document_write.rs` | A withheld response tail cannot delay prefix/script progress; encoding restart is correct; same-script reads and nested written-script execution match the insertion-point algorithm. |
| 5 | Stylesheet script-blocking versus render-blocking state. Parser scripts currently wait on the conservative admitted stylesheet set; partial presentation has no distinct standards-based render-blocking gate. | `page/refresh.rs`, `page/resources.rs`, renderer resource loader | Delayed matching/nonmatching sheets, media changes, errors and imported sheets have separate fetch, cascade, script and render assertions. |
| 6 | HTML event-handler content attributes. JavaScript-assigned handlers work; `onload="..."` in the fixture did not. | `bootstrap/events.js`, DOM attributes/construction | Parse, set, replace, remove, scope, listener ordering and exception behavior for content attributes; not a special-case script `onload` implementation. |

These should be cohesive changes with owned positive and negative fixtures, relevant
WPT-derived contracts, and headless Chromium comparisons. Do not combine unrelated
unsupported APIs into an unreviewable rewrite or advertise a feature based on its name
existing. First presentation, DOMContentLoaded, window load, and visibly usable content
are different milestones.

### Native task delivery

The shell now receives coalesced renderer-ready notifications instead of waiting for
its 250 ms idle monitor to discover queued events. Drains are bounded, and remaining
batches continue at low timer priority. Health/deadline polling remains as a fallback;
idle polling was not shortened. See [event delivery and measured evidence](renderer-event-delivery.md).
Retained parser ownership now removes the whole-DOM-before-script startup barrier.
Streaming response parsing, re-entrant writes and precise stylesheet blocking remain
the next boundaries. Deferred/module readiness and document lifecycle separation are
implemented for the currently admitted resource paths; this is not a complete HTML
navigation implementation.

## Reproducing the owned comparison

### Release evidence, 2026-09-08

Three fresh-profile runs per browser, same machine/server, release Breeze baseline
`70b151b` and the async-readiness change; Chromium `152.0.7977.83`.

| Milestone / correctness check | Merged baseline | Async-readiness change | Chromium |
| --- | --- | --- | --- |
| First fast script, median (range), relative to fixture bootstrap | 2,122 ms (2,080–2,146) | 319 ms (318–358) | 116 ms (113–116) |
| Fast executions from two elements sharing one URL | 1 (incorrect) | 2 | 2 |
| Failed-resource element error events | 0 (incorrect) | 2 | 2 |
| Inspected first filmstrip sample with fast content | 2.5 s | 1.0 s | 0.5 s |

The controlled fast-script delay fell approximately 85%; this is not a whole-page
or YouTube speedup. At this revision, Breeze was still about 2.75 times Chromium on that
script milestone and did not meet the requested 10% startup margin. The filmstrip interval
is 500 ms, so its values are observation bounds, not exact paint timestamps. The
baseline never reaches the fixture's successful final state because it skips one
script element and fails to dispatch the two element errors. No CPU/memory improvement
is claimed from these short runs.

`benchmarks/alpha/fixtures/async-script-readiness.html` uses a two-second first response,
a 100 ms shared fast response, and two owners of a failed response. The fixture server
accepts bounded `?delay_ms=0..10000` requests without blocking unrelated responses.
The fixture intentionally uses JavaScript-assigned event handlers; content attributes
remain the separate gap listed above.

Start `scripts/serve-alpha-fixtures.ps1` with a `-ReadyFile` from a hidden process.
Run Breeze only through `scripts/run-hidden-benchmark.ps1` with `-FreshProfile`,
`-SettleMs 2800`, `-FilmstripIntervalMs 500`, `-FilmstripDurationMs 3500`,
`-WindowWidth 1520`, and `-WindowHeight 1000`. Use `-Browser` for the saved baseline.
Run the existing Chromium harness with its verified `--headless`/`--mute-audio`
launcher, device scale 1.25 and the viewport reported by Breeze (rounded to CSS pixels),
the same settling and filmstrip
parameters, and `--diagnostic-selector '#fast' --diagnostic-selector '#result'`.

`ASYNC_FAST_MS` in Breeze's console and `data-first-script-ms` in Chromium's `#fast`
diagnostics measure the same milestone relative to the fixture's first inline script.
They are not end-to-end navigation timings. Inspect the filmstrip independently.
Expected final title is `Async scripts complete`; the fast box must show two executions.
Captured reports and screenshots belong in ignored output directories, not commits.
