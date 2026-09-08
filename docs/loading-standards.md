# Loading standards: implementation sequence

Audited 2026-09-08. This is a code-backed loading-pipeline inventory, not a claim
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

This does **not** finish all script loading. In particular, async scripts still start
after the current whole-document parse/startup phase, rather than interrupting an
incremental parser. Module dependency loading is still blocking and needs its own slice.

## Remaining standards slices

| Order | Contract / observed gap | Existing ownership | Acceptance test for the next implementation |
| --- | --- | --- | --- |
| 1 | Dynamic script readiness and the ordered `async = false` list: source completion is currently consumed through a blocking loader. | `script/dynamic_scripts.rs`, renderer `document/dynamic_scripts.rs` | Withhold the first response; force-async scripts progress, explicitly ordered scripts preserve insertion order; errors, removal, and navigation are covered. |
| 2 | Separate parsing completion from document load completion. The current `__finishDocument` emits interactive/DCL/complete/load together, without a complete resource-delay model. | `script/module_lifecycle.rs`, `bootstrap/tasks.js`, `document/load.rs` | Delay images and async scripts independently; DCL is not held by them, load is; lifecycle transitions and event order match Chromium. |
| 3 | Defer/module script queues and rendering opportunities. `blocks_first_paint = !is_async` currently makes deferred scripts part of a first-presentation barrier. Module dependency fetching is synchronous. | `page/resources.rs`, `page/scripts.rs`, `script/module_evaluation.rs` | Slow deferred scripts do not turn into parser-blocking scripts; ordered execution, DCL gates, import failures, cycles and top-level await have explicit tests. |
| 4 | Incremental HTML parsing and parser-blocking scripts. The complete DOM is built before initial scripts run. | `document/load.rs`, `engine/dom/tree_sink.rs`, `script/mutation_host.rs` | Scripts see only preceding parsed nodes; parsing pauses/resumes correctly; `document.write` uses the parser insertion point; speculative requests cannot introduce execution. |
| 5 | Stylesheet script-blocking versus render-blocking state. Every discovered stylesheet currently blocks first paint, regardless of its media applicability. | `page/refresh.rs`, `page/resources.rs`, renderer resource loader | Delayed matching/nonmatching sheets, media changes, errors and imported sheets have separate fetch, cascade, script and render assertions. |
| 6 | HTML event-handler content attributes. JavaScript-assigned handlers work; `onload="..."` in the fixture did not. | `bootstrap/events.js`, DOM attributes/construction | Parse, set, replace, remove, scope, listener ordering and exception behavior for content attributes; not a special-case script `onload` implementation. |

These should be cohesive changes with owned positive and negative fixtures, relevant
WPT-derived contracts, and headless Chromium comparisons. Do not combine unrelated
unsupported APIs into an unreviewable rewrite or advertise a feature based on its name
existing. First presentation, DOMContentLoaded, window load, and visibly usable content
are different milestones.

### Native task delivery follow-up

The browser shell currently polls idle renderer events every 250 ms
(`windows_app/renderer_lifecycle.rs`), then schedules runtime work through a Win32 timer.
This can add latency after a resource becomes ready even when the renderer scheduler
is correct. Replace polling-based delivery with bounded, coalesced event notification;
do not solve it by continuously spinning or shortening idle polling indiscriminately.

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
or YouTube speedup. The new build is still about 2.75 times Chromium on that script
milestone and does not meet the requested 10% startup margin. The filmstrip interval
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
launcher, viewport 1489 x 828, device scale 1.25, the same settling and filmstrip
parameters, and `--diagnostic-selector '#fast' --diagnostic-selector '#result'`.

`ASYNC_FAST_MS` in Breeze's console and `data-first-script-ms` in Chromium's `#fast`
diagnostics measure the same milestone relative to the fixture's first inline script.
They are not end-to-end navigation timings. Inspect the filmstrip independently.
Expected final title is `Async scripts complete`; the fast box must show two executions.
Captured reports and screenshots belong in ignored output directories, not commits.
