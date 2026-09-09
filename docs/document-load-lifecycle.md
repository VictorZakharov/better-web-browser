# Document readiness and load-delaying resources

Updated 2026-09-09. This is a bounded loading-pipeline contract, not full HTML navigation
conformance or a claim that YouTube startup now matches Chromium.

## Implemented contract

The runtime owns explicit loading, interactive, DOMContentLoaded-dispatched, and complete
states. The renderer publishes resource readiness; it cannot synchronously dispatch document
completion from a network response. DOMContentLoaded and window load are separate tasks,
with promise jobs and resource discovery between them.

- Primary documents start `loading`; detached documents start `complete`. `readyState` is
  read-only. Transitions fire trusted, non-bubbling, non-cancelable `readystatechange`.
- Interactive is observable before the currently scheduled deferred/module invocations.
  DOMContentLoaded follows those invocations, bubbles from Document to Window, and does not
  wait for eager images, async classic scripts, or pending top-level-await promises.
- Window load waits for admitted eager resources and pending parser-async/dynamic classic
  execution. A resource introduced by a completion listener or its promise job participates
  before the later load task is selected. Success, HTTP/network failure, rejected content and
  obsolete-owner responses all discharge the corresponding admitted obligation.
- In-flight byte sharing does not erase an eager owner's obligation. An admitted eager image
  remains a blocker if its owner is removed, including removal by an initial script. A lazy
  image is excluded unless an eager image or rendered CSS background/mask shares its URL.
- Fetch/XHR and worker/MSE traffic are not document-load blockers. Pending downloads alone
  do not cause timer polling. Cancellation destroys the realm and queued document work.
- Window load follows `complete` and the readiness listener's promise jobs. Its target is
  Document, currentTarget is Window, and the propagation path contains only Window: HTML's
  legacy target override. It is trusted, non-bubbling and non-cancelable, and fires once.
- Pending module evaluation continues to report fulfillment/errors without holding document
  readiness. The existing additional-module element-event path fires after invocation rather
  than after a top-level-await promise settles.

Primary contracts: HTML's [end-of-parsing algorithm](https://html.spec.whatwg.org/multipage/parsing.html#the-end),
[document readiness](https://html.spec.whatwg.org/multipage/dom.html#current-document-readiness),
[script execution](https://html.spec.whatwg.org/multipage/scripting.html#execute-the-script-element),
and [image loading modes](https://html.spec.whatwg.org/multipage/embedded-content.html#attr-img-loading).
The tests are authored for this repository; no external test code or dependencies were copied.

## Owned comparison

`benchmarks/alpha/fixtures/document-load-lifecycle.html` deliberately includes a 500 ms eager
image, a 1000 ms async script which introduces another 1500 ms image, a 3500 ms top-level
await, and unrelated 5000 ms Fetch/lazy-image requests. Its pass condition checks event flags,
targets, readiness and ordering, not a timing threshold. The same fixture runs in a live-runtime
regression test and the headless Chromium harness.

Three serial fresh-profile rounds per browser, no concurrent builds/tests, on the same machine
and loopback server. Merged baseline `5d1c425` uses the browser source built at `4ee9ce0`;
Chromium is `152.0.7977.83`. Breeze window 1520 x 1000 reports 1505.6 x 828 CSS pixels;
Chromium uses 1506 x 828 at the same 1.25 scale. Settle 5500 ms; screenshots every 500 ms
for 6000 ms. Times below are relative to the fixture's bootstrap, not navigation start.

| Milestone/check | Merged baseline | This change | Chromium |
| --- | --- | --- | --- |
| DOMContentLoaded, median (range) | 3545 ms (3529-3546) | 22 ms (18-28) | 4 ms (3-4) |
| Window load, median (range) | 3545 ms (3530-3547) | 2539 ms (2529-2556) | 2527 ms (2522-2538) |
| Complete contract | 0/3 pass | 3/3 pass | 3/3 pass |
| First screenshot showing passing DCL panel | Never | 0.5 s, all runs | 0.5 s, all runs |
| First screenshot showing both passing panels | Never | 3.0 s, all runs | 3.0 s, all runs |

Raw DCL / load samples in round order:

- Baseline: 3545/3545, 3546/3547, 3529/3530 ms.
- Candidate: 18/2529, 22/2556, 28/2539 ms.
- Chromium: 3/2538, 4/2527, 4/2522 ms.

The candidate and Chromium share the trace
`interactive|module|DCL|DCL-microtask|eager|async|async-load|late|complete|LOAD`.
The baseline incorrectly waits for `awaited`, lacks readiness events, and dispatches DCL/load
together. All Breeze runs have zero JavaScript errors, including the failing baseline: absence
of exceptions alone is not a compatibility test. The 0.5 s and completion screenshots were
visually inspected; every filmstrip was checked for the fixture's passing panel colors.

This removes the erroneous 3.5-second promise dependency. It is not a 160x whole-page speedup.
DCL still has 18 ms additional median task/presentation latency versus Chromium (22 vs 4 ms);
the 12 ms window-load difference is mostly obscured by deliberate fixture delays. Neither
number demonstrates the project's whole-page 10% performance target. The two harnesses'
generic `page_ready_ms` values represent different milestones and are not compared here.

### Reproduce

Start `scripts/serve-alpha-fixtures.ps1` in a hidden process with a `-ReadyFile`. Load the
fixture using `scripts/run-hidden-benchmark.ps1`, `-FreshProfile`, `-SettleMs 5500`,
`-WindowWidth 1520`, `-WindowHeight 1000`, `-FilmstripIntervalMs 500`, and
`-FilmstripDurationMs 6000`. Select `#dcl`, `#load`, `#timings`, and `#trace` for diagnostics.
Use `-Browser` for the saved baseline. Chromium's existing launcher must retain `--headless`,
`--mute-audio` and `CreateNoWindow`, with the viewport/scale above. `#load` data attributes
and Breeze's `DOCUMENT_LIFECYCLE` console record expose equivalent fixture timestamps.
Keep reports, profiles and captures in ignored output directories, not commits.

## Explicit remaining boundaries

This gate consumes the renderer's existing admitted stylesheet/image/font/media fetches;
it does not implement the missing fetch algorithms behind them. Whole-document parsing,
defer's current first-presentation barrier, blocking module graph discovery, stylesheet
applicability/import rules, detached `new Image()` fetching, viewport-driven lazy fetching,
nested browsing-context load propagation, detailed media load-delay release, and `pageshow`
remain separate work. Existing listener dispatch also does not model every HTML
callback-cleanup microtask checkpoint within a single event dispatch. The implemented
checkpoints here separate document lifecycle tasks and readiness changes from window load.

Do not infer full script-element error classification or HTML event-handler content attributes
from this change. Continue with the [loading standards sequence](loading-standards.md).
