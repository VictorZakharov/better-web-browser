# Iframe browsing contexts — implementation in progress

The branch starts replacing the shared synthetic iframe window with real, separate
initial child documents and V8 contexts. **This is not complete iframe support and
does not fix modern DuckDuckGo result activation yet.** The URL-backed relay in
[the form/navigation acceptance fixture](form-submission-and-navigation.md) remains
an outstanding acceptance requirement, not an expected pass.

**Not merge-ready:** the full curated upstream run currently has one regression:
`dom/nodes/Node-isConnected.html` fails its iframe adoption case. The bindings use
same-realm `instanceof Node` checks and document-local numeric node handles. Moving
a parent-created node into a child document therefore requires a general
cross-realm DOM binding/ownership refactor, not relaxing the assertion or casting
an unrelated document's numeric handle. The existing test remains enabled and
failing; it must pass before release.

## Initial-document contract implemented

- Each connected HTML iframe owns an initial `about:blank` document, global,
  intrinsic constructors, DOM wrappers, and event-listener storage. Siblings do
  not share globals or documents with each other or with their parent.
- The initial document inherits its parent's base URL, while its document URL and
  location remain `about:blank`. Its initial readiness is `complete`.
- Same-origin children expose their own `Document` constructor and `defaultView`,
  and the correct `parent`, `top`, and `frameElement` relationships.
- Removal destroys the navigable, including nested child navigables. The removed
  element's `contentWindow` and `contentDocument` become null. Reconnection creates
  a new context; retained references to the old document remain usable.
- An iframe whose sandbox omits `allow-same-origin` has a distinct V8 security
  token. `contentDocument` returns null and direct DOM access is denied. This is
  only an initial-document access boundary, **not** full sandbox enforcement.
- Native bookkeeping limits active child contexts to 256 per document tree.

These rules follow HTML's [child navigables and browsing-context creation](https://html.spec.whatwg.org/multipage/document-sequences.html#child-navigable)
and [iframe element connection/removal steps](https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-iframe-element).
There are no site names or endpoint exceptions in the implementation.

## Evidence, September 19, 2026

The same owned fixture runs unmodified in both browsers:

| Initial-document checks | Merged PR #169 release | Working implementation | Headless Chrome 153 |
|---|---:|---:|---:|
| Passed | 6 / 20 | 20 / 20 | 20 / 20 |

The working measurement uses the optimized `performance` development profile,
not a final release build. These are functional assertions, **not performance
measurements**. The fixture covers constructor/global/DOM/event isolation,
identity, base URLs, removal, reconnection, ordinary DOM moves, shadow-tree
iframes, and opaque-sandbox DOM denial. A separate Rust regression covers nested
navigable destruction and retained inactive documents.

Broader validation at this checkpoint: 129 renderer-process tests pass; 82
live-runtime tests pass with three existing live-service ignores. The curated WPT
run is **384/385 files and 3,379/3,380 assertions**, with the adoption regression
above. No manifest entry or acceptance threshold was weakened.

```powershell
dotnet build benchmarks/chromium -c Release
./scripts/test-iframe-initial.ps1 -Chrome -OutputDirectory target/iframe-chrome
./scripts/test-iframe-initial.ps1 -Browser target/performance/better-web-browser.exe -OutputDirectory target/iframe-breeze
cargo test --locked --lib engine::script::tests::embedded_elements
```

The browser harnesses are headless and muted with fresh profiles. Evidence stays
under ignored `target/iframe-contexts-proof`; the assertions and fixture are
checked in. Passing this narrow fixture must not be reported as full iframe
conformance, a passing URL-backed relay, or live-search acceptance.

## Still required before the planned PR is ready

1. Correct cross-realm DOM argument branding, node-handle translation, wrapper
   identity and ownership on adoption; restore the complete curated WPT baseline.
2. Integrate child-document loading with the existing fetch/parser pipeline,
   including `src`/`srcdoc`, redirects, cancellation, and navigation replacement.
3. Give child scripts, timers, microtasks, fetches, module jobs and lifecycle
   events proper scheduling and effect routing. Exposing child APIs alone does
   not mean asynchronous child work is currently serviced.
4. Preserve `WindowProxy` identity across navigation and enforce origin checks
   on every permitted cross-origin operation; retain safe old-document references.
5. Implement message routing with the correct incumbent source, structured clone,
   target-origin delivery checks, and sandbox navigation/script restrictions.
   The existing window-message helper does not establish cross-realm correctness.
6. Add owned and unmodified upstream tests for these behaviors, make the existing
   URL-backed relay pass, and retry live search → result → Back → another search.
   Keep the HTML fallback until that workflow is reliable.

Visual iframe embedding, out-of-process frame isolation, and complete sandbox,
navigation-history, storage, messaging and Web Platform Test conformance must not
be inferred from the initial-document implementation.
