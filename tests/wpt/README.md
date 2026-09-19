# Curated Web Platform Tests

This directory defines Breeze's small upstream conformance regression suite. Test files are never
copied into this repository. The runner requires a separate sparse checkout of the official
[web-platform-tests/wpt](https://github.com/web-platform-tests/wpt) repository at the exact revision
recorded in `manifest.json`.

WPT is distributed under the
[3-Clause BSD License](https://github.com/web-platform-tests/wpt/blob/f9ecd8a4a9c6e9865ea4aee4741e4b02f75fd476/LICENSE.md). The
external checkout retains the upstream source, history, metadata, and license. Breeze's
`reporter.js` is an original adapter which uses testharness.js's documented result and completion
callbacks; it replaces `/resources/testharnessreport.js` only for these hidden local runs.

Event-handler tests also use WPT's externally checked-in WebIDL2 build
(`e6d8ab852ec4e76596f6e308eb7f2efc8b613bfd`,
[MIT license](https://github.com/w3c/webidl2.js/blob/e6d8ab852ec4e76596f6e308eb7f2efc8b613bfd/LICENSE)).
The local server mirrors upstream's `/resources/WebIDLParser.js` URL alias; no dependency
or third-party source is added to Breeze's production build.

## Prepare fixtures once

Choose a location outside this repository:

```powershell
.\scripts\checkout-wpt.ps1 -Destination ..\wpt
```

The setup script sparsely checks out only `resources/testharness.js` and the paths in the manifest.
It refuses to place upstream fixtures inside the Breeze worktree or overwrite a dirty checkout.
Network access is needed only to create or update this external checkout.

## Run the suite

After the fixtures exist, all curated cases run with one offline command:

```powershell
.\scripts\run-wpt.ps1 -WptRoot ..\wpt
```

Use `-Filter "DOM events"` for a single area or a path substring. Use `-SkipBuild` only after both
release binaries are current. `-Jobs 4` runs four isolated hidden processes concurrently. The
command starts a loopback-only static server and launches a fresh hidden Breeze benchmark process
per case. JavaScript `.any.js` and `.window.js` files run in a generated Window test page. Upstream
`// META: script=` dependencies load unchanged, in declared order, resolved against the test's URL;
external-origin dependencies are refused. These wrappers do not run Worker variants. This first
curated set intentionally avoids WPT server substitutions and special hostnames. A manifest can
list immutable relative support files from the same pinned upstream checkout; expanding into tests
that need WPT server behavior should use the official WPT server rather than ad-hoc emulation.

The runner explicitly requests 100% device scale and rejects a report at another scale. Several
upstream assertions assume that environment: at 125%, Chrome also snaps a 1 CSS-pixel border to
0.8 CSS pixels and produces different classic-scrollbar geometry. This is test-environment control,
not a change to normal browsing, which continues to use the monitor's DPI. Other hidden comparisons
can request `-DeviceScaleFactor` (1–4) with `scripts/run-hidden-benchmark.ps1`; omitting it keeps native DPI.

CI passes `-BuildProfile debug -Jobs 8`: conformance is not a performance measurement, and benchmark
processes launched by this runner request an explicitly sized headless UI-thread stack so large
standards harnesses remain safe in unoptimized builds. Normal local runs remain sequential release
builds, and unrelated performance benchmarks keep the browser's ordinary main-thread path.

The runner asks hidden Breeze instances to finish as soon as testharness emits the reporter's
completion marker. The configured settle period is therefore a fail-safe deadline for a harness
that never completes, rather than a fixed delay paid by every passing case.

The original callback adapter chunks large reports across delayed tasks to respect
the renderer's bounded diagnostic reports, including when a wakeup batches ready
tasks. Upstream tests are unchanged. The reader requires every ordered chunk and
the final completion marker; truncation cannot silently turn a partial report green.

The console reports each case and `target/wpt/report.json` records the pinned revision, harness
subtests, JavaScript diagnostics, durations, and one of four actual outcomes: `pass`, `fail`,
`timeout`, or `crash`. Expected non-passes require a reason in the manifest. A matching expected
failure is successful in a discovery manifest, while an unexpected pass, changed failure mode,
regression, or crash makes the command fail. The curated manifest forbids every non-pass
expectation and enforces a floor of 200 harness subtests. Its current baseline is
383 passing files / 3,372 passing harness subtests (2026-09-19) with no failure, skip, timeout,
or crash allowance. This forces the
manifest to be updated deliberately when compatibility changes.

## Selection contract

The feature clusters were chosen before expanding the gate: parser and DOM ownership, mutation and
event dispatch, task ordering, URL handling, network-facing objects, browser-owned cookies, form
bindings, and the style/layout surfaces used by the alpha fixtures. The selected files are distributed as
follows:

| Cluster | Files | Why it is gated |
|---|---:|---|
| HTML parsing | 4 | Tree construction and malformed-input recovery |
| Active-parser writes | 60 | Same-call DOM visibility, token/character boundaries, nested classic execution and external-script pause/resumption |
| Document streams | 3 | Document open/close, replacement, and parser state |
| HTML event handlers | 12 | Compilation, scope, ordering, forwarding, mutation and errors |
| Stylesheet loading | 1 | MIME rejection for nonempty and empty stylesheet responses |
| DOM and mutation | 10 | Owned nodes, lookup, names, nested-document connectivity, hierarchy validation, atomic live mutation, and observer records |
| Dynamic inline classic scripts | 15 | Synchronous insertion, empty/nonempty preparation, direct child text, movement, type/source changes, and nested/fragment execution order |
| Events, Abort API, event loop, and animation frames | 23 | Dispatch semantics, cancellation, listeners, microtasks, and rendering callbacks |
| Idle callbacks | 12 | Deadline caps, timeout races while busy, cancellation, exceptions, and FIFO/repost fairness |
| Resize observers | 18 | Content/border boxes, padding, box selection, inline transitions, callback lifetime, depth and error handling, and observer ordering |
| Performance Timeline and User Timing | 39 | Asynchronous/buffered observation, filtering, buffer lifetime, mark/measure dictionaries, structured details, and monotonic clocks |
| URLs | 16 | Explicit-base statics, setter stripping, forgiving form decoding, and live URLSearchParams methods |
| Detached document parsing | 8 | HTML/XML parser documents, encodings, doctypes, namespaces, and inert stylesheets |
| Intersection observers | 36 | Geometry, clipping, thresholds, lifecycle, and asynchronous delivery |
| Fetch and XMLHttpRequest | 31 | Headers, request/response objects, bodies, progress, guards, and CORS-facing behavior |
| Readable streams | 2 | Demand-driven tee, cancellation, error ordering, and floating-point queue-size accounting |
| Cookies | 1 | Document-cookie interaction with forbidden meta delivery |
| Web Storage | 25 | Lossless strings, method/named access, conversions, quotas, enumeration, independent areas, and storage-event construction |
| CSS cascade, selectors, and layout | 29 | Cascade, structural selectors, generated content, flex display, and CSSOM geometry |
| CSSOM fragment geometry | 8 | Snapshot lists, inline fragments, selected text, display:contents, and UTF-16 source offsets |
| CSSOM stylesheet ownership | 3 | Preferred-set insertion order and per-occurrence imported-sheet identity |
| Forms | 4 | Form collections, button types, datalist options/validation, and select values |
| JavaScript modules and Web IDL | 7 | Script scheduling and platform exception bindings |
| Dynamic modules | 2 | Native import promises and parse/specifier/link/evaluation error identity |
| Custom Elements | 8 | Registry isolation, definition lookup, when-defined promises, parser construction, and reaction ordering |
| Shadow DOM | 3 | Root connectivity, detached slot assignment, and composed event retargeting |

The revision `f9ecd8a4a9c6e9865ea4aee4741e4b02f75fd476` (2026-08-14 upstream commit)
is pinned by full hash so sparse preparation and later offline runs are reproducible. Selection is
limited to upstream testharness files that are deterministic in a plain loopback Window context and
exercise implemented alpha surfaces. Tests needing special WPT hosts, server substitutions,
testdriver, HTTPS, or additional support servers remain outside this focused gate; they are not
silently copied or rewritten.

Every case uses an isolated temporary profile. Large testharness completion reports are split into
bounded, ordered console chunks and reassembled without omitting subtests; malformed or missing
chunks fail the case. The ordinary renderer IPC text limits and upstream test timeouts are unchanged.

## Discovery sample

`url-parser-discovery.json` separately runs the complete upstream URL constructor,
origin, and setter corpora. It intentionally keeps passing expectations and exits
nonzero while parser-library gaps remain; those failures are not part of the green
curated gate or a claimed whole-URL pass rate. See the
[URL/request contract](../../docs/url-request-resolution.md) for the exact command
and remaining file-URL, IDNA, and unusual-path boundaries. No upstream assertion is
rewritten or skipped within either manifest.

The active-parser write selection is `001.html`–`046.html`, `051.html` and
`script_001.html`–`script_013.html` under `html/webappapis/dynamic-markup-insertion/document-write/`.
That selection does not cover popup/frame realms, XML or module destructive-write policy.
Three separate document-stream tests cover part of document open/close and replacement;
see the [stream contract](../../docs/document-streams-and-pre-wrap.md) and
[parser observation contract](../../docs/parser-observation-and-cssom.md).

The broader ResizeObserver inventory is documented in
[ResizeObserver delivery](../../docs/resize-observer-delivery.md#remaining-boundaries), including
the cases intentionally outside this green gate. The selected 18 files are not a whole-spec pass rate.

Both former discovery cases, `MutationObserver-document.html` and `table-scroll-props.html`,
now pass and are part of the strict gate. The obsolete failure manifest is retired. The table
case was already green on merged main; only the dynamic inline-script case needed new execution
support. Fifteen execution-timing files (`013`, `016`, `037`, `048`, `056`–`060`, `089`, `090`,
`124`, and `127`–`129`) extend that script contract; their shared helper and support script remain
in the external licensed WPT checkout.

See [inline scripts and table geometry](../../docs/inline-scripts-and-table-geometry.md) for the
measured Chrome differences, including Chrome's four failing border-offset assertions in the
pinned `table-client-props.html`. Existing passing coverage is not weakened to match that result.
Future discovery manifests must document their actual observed non-pass and reason before use;
there is currently no active expected-failure sample. None of these selections is a whole-platform
pass rate.

## Runner decision

The official [wptrunner](https://web-platform-tests.org/tools/wptrunner/README.html) remains the
right long-term integration point for broad WPT execution. Its documented
[product/executor design](https://web-platform-tests.org/tools/wptrunner/docs/design.html) normally
drives browsers through a remote protocol such as WebDriver and uses the full WPT environment.
Breeze does not expose such a protocol or have an upstream wptrunner product. Adopting it for this
alpha slice would therefore add a browser-control layer and full server integration rather than
reduce maintenance. The focused runner is retained because it already provides hidden fresh-process
isolation, bounded timeouts, crash detection, parallel execution, pinned offline fixtures, and a
structured report without creating a visible UI or WebDriver detour. Re-evaluate wptrunner when
coverage requires special hosts, substitutions, testdriver, or cross-context orchestration.

The runner follows the official
[testharness.js API](https://web-platform-tests.org/writing-tests/testharness-api.html). It is a
focused regression gate, not a replacement for WPT's full
[local runner and server](https://web-platform-tests.org/running-tests/from-local-system.html).
