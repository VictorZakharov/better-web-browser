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

## Prepare fixtures once

Choose a location outside this repository:

```powershell
.\scripts\checkout-wpt.ps1 -Destination ..\wpt
```

The setup script sparsely checks out only `resources/testharness.js` and the paths in the manifest.
It refuses to place upstream fixtures inside the Breeze worktree or overwrite a dirty checkout.
Network access is needed only to create or update this external checkout.

## Run the suite

After the fixtures exist, all 83 curated cases run with one offline command:

```powershell
.\scripts\run-wpt.ps1 -WptRoot ..\wpt
```

Use `-Filter "DOM events"` for a single area or a path substring. Use `-SkipBuild` only after both
release binaries are current. `-Jobs 4` runs four isolated hidden processes concurrently. The
command starts a loopback-only static server and launches a fresh hidden Breeze benchmark process
per case. JavaScript `.any.js` and `.window.js` files run in a generated Window test page. This first
curated set intentionally avoids WPT server substitutions, special hostnames, and support files;
expanding beyond that contract should use the official WPT server rather than ad-hoc emulation.

CI passes `-BuildProfile debug -Jobs 8`: conformance is not a performance measurement, and benchmark
processes launched by this runner request an explicitly sized headless UI-thread stack so large
standards harnesses remain safe in unoptimized builds. Normal local runs remain sequential release
builds, and unrelated performance benchmarks keep the browser's ordinary main-thread path.

The runner asks hidden Breeze instances to finish as soon as testharness emits the reporter's
completion marker. The configured settle period is therefore a fail-safe deadline for a harness
that never completes, rather than a fixed delay paid by every passing case.

The console reports each case and `target/wpt/report.json` records the pinned revision, harness
subtests, JavaScript diagnostics, durations, and one of four actual outcomes: `pass`, `fail`,
`timeout`, or `crash`. Expected non-passes require a reason in the manifest. A matching expected
failure is successful in a discovery manifest, while an unexpected pass, changed failure mode,
regression, or crash makes the command fail. The curated manifest forbids every non-pass
expectation and enforces a floor of 200 harness subtests. Its current baseline is 83 passing files
and 595 passing harness subtests with no failure, skip, timeout, or crash allowance. This forces the
manifest to be updated deliberately when compatibility changes.

## Selection contract

The feature clusters were chosen before expanding the gate: parser and DOM ownership, mutation and
event dispatch, task ordering, URL handling, network-facing objects, browser-owned cookies, form
bindings, and the style/layout surfaces used by the alpha fixtures. The 83 files are distributed as
follows:

| Cluster | Files | Why it is gated |
|---|---:|---|
| HTML parsing | 4 | Tree construction and malformed-input recovery |
| DOM and mutation | 6 | Owned nodes, lookup, names, and live mutation |
| Events, Abort API, and event loop | 16 | Dispatch semantics, cancellation, listeners, and microtasks |
| URLs | 5 | URL and URLSearchParams bindings |
| Fetch and XMLHttpRequest | 31 | Headers, request/response objects, bodies, progress, guards, and CORS-facing behavior |
| Cookies | 1 | Document-cookie interaction with forbidden meta delivery |
| CSS cascade, selectors, and layout | 10 | Cascade, `:first-child`, flex display, and CSSOM geometry |
| Forms | 2 | Form collections and select value behavior |
| JavaScript modules and Web IDL | 7 | Script scheduling and platform exception bindings |
| Custom Elements | 1 | Registry isolation, definition lookup, and when-defined promises |

The revision `f9ecd8a4a9c6e9865ea4aee4741e4b02f75fd476` (2026-08-14 upstream commit)
is pinned by full hash so sparse preparation and later offline runs are reproducible. Selection is
limited to upstream testharness files that are deterministic in a plain loopback Window context and
exercise implemented alpha surfaces. Tests needing special WPT hosts, server substitutions,
testdriver, HTTPS, or additional support servers remain outside this focused gate; they are not
silently copied or rewritten.

## Discovery sample

`discovery.json` is a separate, deliberately failing compatibility sample. It covers nearby DOM,
forms, selectors, flex, float, and geometry behavior that is not in the green gate. At the pinned
revision its eight files contain 19 harness subtests: 3 pass and 16 fail. The known file-level
failure mode and reason are explicit, so an unexpected pass tells maintainers to promote newly
supported coverage instead of masking it.

```powershell
.\scripts\checkout-wpt.ps1 -Destination ..\wpt -Manifest .\tests\wpt\discovery.json
.\scripts\run-wpt.ps1 -WptRoot ..\wpt -Manifest .\tests\wpt\discovery.json `
  -Output .\target\wpt\discovery-report.json
```

The discovery result is an inventory, not a whole-platform pass rate. Restore the curated sparse
selection by running the normal checkout command before the gated suite.

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
