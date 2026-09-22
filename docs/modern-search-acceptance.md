# Modern search acceptance

Issue [#89](https://github.com/VictorZakharov/better-web-browser/issues/89)
retires the built-in DuckDuckGo HTML fallback only after the modern application
passes owned, cross-browser contracts and a hidden live interaction flow. This is
an integration decision, not a claim that every DuckDuckGo feature or every web
application is supported.

## Standards-based scope

The prerequisite slices implement the general platform behavior that the modern
application exercises: live DOM collections and attributes, URL and request
resolution, DOMParser, native microtasks, CSSOM View geometry, media queries,
IntersectionObserver and ResizeObserver delivery, form submission, child browsing
contexts, cross-context messaging, and history navigation. Their implementation
and evidence live in the linked subsystem documents; no production path checks a
DuckDuckGo host name, endpoint, selector, or response body.

The final integration adds no provider-specific production behavior. It supplies
an owned composition fixture, strengthens the existing hidden live acceptance
runner, and changes the browser default only after those general engine slices are
present. Primary contracts are linked from the subsystem documents and include
the [DOM node-tree model](https://dom.spec.whatwg.org/#concept-node-tree),
[HTML form submission](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#form-submission-algorithm),
and [child navigables](https://html.spec.whatwg.org/multipage/document-sequences.html#child-navigable).

## Deterministic acceptance

`benchmarks/alpha/fixtures/modern-search-app.html` is original project material; it
contains no provider code or assets. The fixture combines the application patterns
that previously failed only when composed:

- asynchronous Promise bootstrap and detached DOMParser import;
- responsive `matchMedia` state and IntersectionObserver delivery;
- named form controls, `requestSubmit()`, and repeated result rendering;
- same-origin `srcdoc` child messaging and result activation; and
- `history.pushState()` after the child relay.

It joins the normal paired alpha matrix as `modern-search-app`. Breeze and
unified-headless Chromium receive identical fixture bytes, viewport and fresh
profiles. Both must expose the ready marker, render populated primary content,
report no script errors, stay within the existing two-times readiness ceiling, and
remain under a 0.12 visual-difference ceiling.

## Live acceptance

`scripts/test-ddg-entrypoints.ps1` keeps the HTML routes as regressions while adding
the modern route:

1. load the homepage;
2. search from the homepage;
3. load a modern results URL, activate a visible Wikipedia result, go Back, edit
   the retained search control, and submit a second Unicode query;
4. load HTML results; and
5. submit another HTML query.

Every run is hidden and muted through the fail-closed benchmark launcher and uses a
fresh profile. Acceptance requires a non-error title, a visible result link, the
expected final query and title, and no JavaScript or error-console entries. An HTTP
200, a challenge page, or a blank shell is not a pass. The live URL is also recorded
on the alpha fixture so `run-alpha.ps1 -Live -Fixture modern-search-app` captures the
same URL in Breeze and unified-headless Chromium. Live third-party content remains
diagnostic because service responses and anti-automation challenges can differ.

## Decision and boundaries

Address-bar searches now use `https://duckduckgo.com/?q=…&ia=web`. Direct HTML
routes remain supported and tested, but are no longer the browser default.

The acceptance covers mounting, usable results, native search input, result
activation, Back, repeat search, and bounded document failure. It does not
establish account features, every rich-result module,
pixel-perfect provider styling, anti-bot bypasses, or universal application
compatibility. Deliberately incomplete progressive enhancements remain documented
with their owning subsystem rather than hidden by provider-specific code.

The live interaction deliberately selects the stable Wikipedia result used by the
earlier child-context acceptance. A provider can rank an exceptionally large
document first; the WHATWG single-page HTML Standard currently exceeds Breeze's
separate 100,000-node document safety limit. That large-document boundary is not
reclassified as a modern-search failure, and the harness does not ignore errors
from the selected navigation path.

## Evidence — September 22, 2026

Three fresh-profile owned pairs pass. The median visual difference is 0.043 against
the unchanged 0.12 ceiling. Breeze page-ready is 125.9 ms and Chromium load is
390.4 ms; those clocks have different definitions and are gate inputs, not a
universal speed comparison. Median working sets are 32.9 MiB and 544.5 MiB,
respectively, under the controlled fixture conditions.

The final library suite passes 1,355 tests with one existing ignore. The isolated
renderer suite passes 138 tests, and live runtime passes 82 tests with three
existing live-service ignores. Three complete fresh-profile live runs pass all five
entrypoints: 15/15 cases, including three clean result → Back → Unicode re-search
flows with the correct final title, ten visible result links, and no renderer exit.

One same-URL hidden live pair returned HTTP 200 and nonblank content in both
browsers. Breeze reported zero JavaScript/console errors and 216 retained draw
items. The live visual difference was 0.374; it is diagnostic, not a threshold,
because the service can vary content between requests. The paired run recorded
471.5 ms Breeze ready versus 1,171.8 ms Chromium load and 127.6/637.8 MiB working
sets, but the different readiness clocks and sequential third-party responses make
those values observations rather than a browser-speed claim.

## Reproduction

```powershell
./scripts/prepare-v8.ps1 -Profile release
cargo build --release --locked --bin better-web-browser
./benchmarks/run-alpha.ps1 -SkipBuild -Fixture modern-search-app -Iterations 3
./benchmarks/run-alpha.ps1 -SkipBuild -Live -Fixture modern-search-app -Iterations 1
./scripts/test-ddg-entrypoints.ps1 -Browser ./target/release/better-web-browser.exe
cargo test --locked --lib
cargo test --locked --test renderer_process
cargo test --locked --test live_runtime
```

Generated live reports and captures stay under ignored output directories and are
not committed.
