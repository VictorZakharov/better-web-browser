# Dynamic external classic script readiness

Implemented 2026-09-09. This slice replaces synchronous dynamic-source consumption
in the isolated renderer, not the JavaScript engine or the network security policy.

## Compatibility contract

- A newly created HTML script element is force-async even without an `async`
  attribute. The IDL setter clears force-async; adding the content attribute also
  clears it. Removing an absent attribute does not change the flag.
- Preparation captures the element, source, fetch policy, and execution mode.
  Default/explicit async scripts execute when ready; `async = false` scripts use
  a separate ordered list. Ready ordered successors run together before selecting
  another timer or async task, subject to the existing bounded script-work slice.
- Fetch completion only publishes readiness. It does not execute JavaScript from
  a response callback, synchronously wait for another response, or poll downloads
  through repeated renderer clock advances. New requests can start while earlier
  batches remain incomplete.
- In-flight requests with the same URL, script kind, credentials, and referrer
  policy share bytes, while each element retains its own execution and events.
- Detachment and `src`/`async` changes do not rewrite a prepared operation.
  Adoption into another document prevents execution in the preparation document;
  navigation destroys pending work. Started flags survive cloning and adoption.
- Successful fetching produces a trusted element `load` even if evaluation throws.
  Failed/empty sources produce trusted, non-bubbling, non-cancelable `error` events.
  Failed ordered heads release their successors. Network failures remain visible
  in diagnostics, distinct from JavaScript exceptions.
- Promise jobs, `document.currentScript`, and element load handlers retain the
  existing classic-script cleanup order. Work admitted with a zero task budget
  cannot execute a ready script. Existing script/page byte limits still apply,
  including bytes retained by ready scripts awaiting an ordered predecessor.
- Connecting a subtree prepares its external scripts. Parser-marked inert scripts
  and `nomodule` scripts remain inert; no site-specific script rewriting is used.

The primary contract is HTML's [script processing model](https://html.spec.whatwg.org/multipage/scripting.html#script-processing-model),
including [async reflection](https://html.spec.whatwg.org/multipage/scripting.html#dom-script-async),
[preparation](https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element),
and [execution](https://html.spec.whatwg.org/multipage/scripting.html#execute-the-script-element).

## Scope and verification

The runtime regressions cover default async state, attribute/IDL transitions,
cloning, inert scripts, connection, overtaking, ordered prefixes, exceptions,
element events, promise jobs, empty sources, budgets, adoption, and cancellation.
Isolated-renderer tests withhold a slow response while verifying fast painted
content, absence of idle polling, independent ordered scripts, shared resource
owners, and a newly requested script starting before the old batch completes.

This is **not** full HTML script/loading conformance. Dynamic inline execution,
dynamic modules and dependency graphs, incremental parser integration, and the
document-wide DOMContentLoaded/load resource-delay model remain separate work.
The synchronous source-loader API remains for non-renderer embedders and module
graphs; it is no longer used to wait for dynamic classic sources in the browser.

The owned comparison fixture is
`benchmarks/alpha/fixtures/dynamic-script-readiness.html`. It uses a two-second
slow download, a 100 ms shared fast download, two failed-resource owners, and an
ordered pair whose second response is ready first. `ASYNC_FAST_MS` in Breeze's
console and `#fast`'s `data-first-script-ms` in Chromium measure the same elapsed
time from fixture bootstrap, not navigation or window-load time. The successful
final title is `Dynamic scripts complete`.

Run the existing hidden fixture server and benchmark wrappers with the same
settings documented in [loading standards](loading-standards.md#reproducing-the-owned-comparison),
using this fixture URL. Keep reports and screenshots in ignored `target` output;
inspect 500 ms filmstrip samples separately from the script timestamp.

## Release evidence

Five serial fresh-profile rounds on the same machine and loopback server, with no
concurrent compiler or browser tests: merged baseline `38e53f4` (same browser source
as `ae3dd55`), this implementation, and Chromium `152.0.7977.83`. Breeze used a
1520 by 1000 window, reporting a 1505.6 by 828 CSS-pixel viewport; Chromium used
1506 by 828 at the same 1.25 device scale. Settling was 2800 ms; filmstrip samples
were taken every 500 ms for 3500 ms.

| Milestone / check | Merged baseline | This change | Chromium |
| --- | --- | --- | --- |
| First fast script, median (range), from fixture bootstrap | 2075 ms (2066–2097) | 131 ms (121–133) | 117 ms (108–121) |
| First filmstrip sample showing fast content, all five runs | 2.5 s | 0.5 s | 0.5 s |
| Fast elements executed from the shared URL | 2 | 2 | 2 |
| Failed-resource element error events | 0 | 2 | 2 |

Raw first-fast samples, in round order:

- Baseline: 2066, 2071, 2080, 2075, 2097 ms.
- This change: 133, 131, 121, 133, 125 ms.
- Chromium: 112, 108, 118, 121, 117 ms.

That is approximately 94% less delay on this controlled readiness milestone.
Breeze remains about 12% slower than Chromium here, outside the requested 10%
margin. The measurement includes the fixture's intentional 100 ms fast-response
delay; it does not establish whole-page or YouTube startup parity, CPU savings,
or memory savings. Filmstrip times are observation bounds, not exact paint times.
All five candidate runs reached `Dynamic scripts complete`, with two fast
executions, ordered-first before ordered-second, both expected element errors,
and no JavaScript exceptions. The baseline stayed pending because it did not
dispatch the failed-resource element events. The 0.5-second screenshots were
visually inspected against Chromium; all five filmstrips were checked for the
fixture's green fast-content state.

Verification also exposed two harness issues: the hidden fullscreen Escape test
waited for a transient initial title rather than confirmed entry, and Chromium's
process-tree guard could attribute an older process through a recycled parent
PID. The tests now wait for the fullscreen entry signal and validate creation
times when attributing processes. The visible-window guard remains enabled.
The rejected Chromium startup sample was excluded; the five reported rounds
were captured after that attribution fix.
