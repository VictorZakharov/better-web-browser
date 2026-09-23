# Child browsing contexts and message-driven navigation

This slice replaces the shared synthetic iframe window with child Documents and
V8 contexts. It enables URL-backed frame/message relays without running child code
in the parent realm or bypassing canceled links. It also fixes Back after a loaded
document navigates with `Location.assign()` or the `href` setter.

**This is a scripting/navigation slice, not complete iframe support.** Visual frame
embedding, child persistent state, and several policy/lifecycle features remain
unsupported. The later [modern-search acceptance](modern-search-acceptance.md)
records the integration decision after this slice.

## Implemented contracts

- Connecting an HTML iframe synchronously creates an initial `about:blank`
  Document, global, intrinsic constructors, and event storage. It inherits the
  parent's base URL and origin unless sandboxing requires an opaque origin.
- Private native node brands translate document-local handles across same-origin
  realms. Adoption preserves wrapper identity and its original prototype while
  changing the owner Document. Document registry roots are weak.
- `srcdoc` takes precedence over `src`. Relative `src` changes resolve against the
  embedding Document. A changed URL or removal invalidates old navigation epochs,
  aborts pending fetches, and terminates owned workers. Reconnection makes a new
  context. Superseded responses cannot replace its Document.
- HTML response heads commit the new Document before body EOF. The existing
  incremental parser and decoder handle split bytes, HTTP charset precedence,
  and meta-encoding replay. A restart replaces the Document but preserves the
  WindowProxy. Load waits for the parser and its resource obligations to finish.
- V8's public `DetachGlobal`/global-reuse boundary preserves WindowProxy identity
  across navigation while replacing the realm. Retained old DOM objects remain
  associated with their original Document, not the replacement host.
- Parser scripts use the existing blocking/async/deferred and module queues.
  `document.write` uses the parser insertion point. Linked stylesheets and imports
  use shared dependency discovery; blocking scripts wait for applicable CSS, and
  loaded stylesheet text is available to CSSOM. Failure releases the blocker and
  reports the element error. Inserted scripts and `import()` use child-owned fetches.
- Related contexts share one isolate/watchdog and a scheduled task loop. Timers,
  promise jobs, message queues, fetches, and dedicated workers retain their Document
  ownership. Native identifier checks reject attempts to abort another Document's
  fetch or terminate its worker. Child load precedes iframe load and parent load.
- Cross-origin Window access has a native allowlist for the HTML Window/Location
  surface. DOM/arbitrary-property access and foreign prototype access stay denied.
  Permitted functions and descriptors are created in the caller's realm. Foreign
  Location reads are denied; writes obey sandbox/ancestor checks. Sandboxed script
  execution, forms, opaque origins, and top-navigation restrictions are inherited.
- `postMessage` uses the incumbent context for sender origin and source, checks
  target origin at delivery, and deserializes into the receiving realm. V8 handles
  built-in cloneable values and ArrayBuffers; native side tables handle MessagePorts,
  Blob, and File. Transfers commit only after serialization and queue admission.
  Native MessagePort endpoints retain entanglement/queued messages across transfers;
  start/close and discarded-transfer cleanup are tested. Window and port tasks take
  turns, so a recursive window-message stream cannot starve ports.
- FileReader provides asynchronous text, binary-string, ArrayBuffer, and data-URL
  reads over private Blob bytes, progress/load/error/abort events, encoding selection,
  cancellation, and read chaining. Author-defined Blob getters cannot replace its
  underlying bytes.
- Browser-owned Fetch clients are established from final response heads, not
  renderer-supplied origins. Redirected child requests retain the appropriate origin
  and referrer. The embedding client's frame policy controls child self-navigation
  as well as attribute navigation, including redirects.
- Supported response CSP source lists are intersected and enforced for script,
  connection, stylesheet, frame, base, and form requests; eval/code generation and
  inline script checks use the child policy. `frame-ancestors` and X-Frame-Options
  are checked against ancestors. Unsupported CSP expressions/directives refuse the
  child response rather than silently treating that policy as absent.
- Loaded-document Location assignment pushes session history; initial redirects
  without activation, explicit replacement, and reload replace the current entry.
  Existing bounded scripted-navigation guards remain in place.

Primary contracts: HTML [child navigables](https://html.spec.whatwg.org/multipage/document-sequences.html#child-navigable),
[iframe processing](https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-iframe-element),
[cross-origin Window](https://html.spec.whatwg.org/multipage/nav-history-apis.html#crossoriginproperties-(-o-)),
[Location navigation](https://html.spec.whatwg.org/multipage/nav-history-apis.html#location-object-navigate),
[messaging](https://html.spec.whatwg.org/multipage/web-messaging.html),
[structured serialization](https://html.spec.whatwg.org/multipage/structured-data.html#structuredserializewithtransfer),
DOM [adoption](https://dom.spec.whatwg.org/#concept-node-adopt),
[File API](https://w3c.github.io/FileAPI/), and [CSP 3](https://www.w3.org/TR/CSP3/).
No implementation code contains site-specific endpoint or host exceptions.

## Evidence — September 19, 2026

The following evidence describes the initial child-context head. Subsequent user
testing exposed homepage and HTML-results failures outside those direct-results
runs; see the [entrypoint correction and follow-up](ddg-entrypoint-regressions.md).

| Check | Merged PR #169 release | This slice | Unified-headless Chrome 153 |
|---|---:|---:|---:|
| Initial-document fixture | 6 / 20 | 20 / 20 | 20 / 20 |
| Same-origin URL/message/navigation relay | Fails | Passes | Passes |
| External-script relay with response CSP | Not supported | Passes | Passes |
| Relay → destination → Back → second Unicode query | No functioning relay | Passes | Passes |
| Live modern DDG → Wikipedia result → Back → second search | Result activation blocked | 3 / 3 fresh release runs pass | Requested result link not presented |
| Curated WPT files / assertions | 385 / 3,380 pass | 397 / 3,393 pass | Not run by this runner |

The three live Breeze release runs used fresh state, native result activation, Back,
native input, and Enter. Each ended at `?q=CSS+Grid+specification&ia=web` with ten
result-title links and no reported JavaScript or console errors. CSS-variable
warnings remain; this is not a claim of error-free visual compatibility.
The Chrome run did not present the requested Wikipedia result
link, so there is **no equal-content live timing comparison**. The functional
harness deliberately waits between interactions: its elapsed time is not load speed.
Owned fixtures provide the deterministic cross-browser comparison.

The final local suites pass 1,225 library tests (one existing ignore), 118 browser
unit tests, 17 WPT-runner tests, 130 renderer integration tests, and 82 live-runtime
tests (three existing live-service ignores). The four additional Windows integration
suites pass all 22 tests. All twelve owned form/navigation cases pass in release
Breeze and headless Chrome, as do the twenty initial-document checks. One fresh
pair per existing alpha fixture passes all sixteen visual/readiness/scroll gates
with unchanged thresholds. This smoke comparison is not a new performance assessment.
Clippy with warnings denied, formatting, source-size checks, and generated notices
validation pass. No file-size ceiling was raised.

No upstream
assertion, existing manifest entry, or acceptance threshold was weakened. Twelve
unmodified upstream web-messaging files add thirteen assertions to the curated
suite. The WPT checkout remains external and pinned; its BSD-licensed source is
not copied into this repository.

```powershell
./scripts/test-iframe-initial.ps1 -OutputDirectory target/iframe-initial
./scripts/test-iframe-initial.ps1 -Chrome -OutputDirectory target/iframe-initial-chrome
./scripts/test-form-navigation.ps1 -Cases flow,iframe,iframe-external -OutputDirectory target/iframe-relay
./scripts/test-form-navigation.ps1 -Chrome -Cases flow,iframe,iframe-external -OutputDirectory target/iframe-relay-chrome
cargo test --locked --lib frame_navigation
./scripts/run-wpt.ps1 -WptRoot ../wpt -Jobs 4
```

All automated browsers remain hidden and muted. Breeze uses its fail-closed
benchmark wrapper; Chromium uses unified `--headless`, `--mute-audio`, and
`CreateNoWindow`. Captures/logs stay under ignored `target/iframe-contexts-proof`.

## Deliberate limits and remaining work

- No visual child-document compositing, child viewport/input/observer geometry,
  embedded media playback, or embedded fullscreen. Media/fullscreen requests reject
  instead of remaining pending or acting on the parent's browser identity.
- Child cookie/storage projections are document-local and do not persist to the
  browser profile or synchronize with other contexts. Their updates are never
  applied under the parent's identity. Full storage/event/cookie routing remains
  required before claiming embedded applications have persistent state.
- Child form POST, named/ancestor form targets, joint nested session history, and
  history restoration are incomplete. Unsupported embedded POST/target requests
  report a diagnostic instead of silently becoming a GET or parent navigation.
- The CSP implementation is a **child response-header subset**, not general CSP
  conformance. Nonced parser scripts, CSP3 `strict-dynamic` script loading, and
  external dedicated-worker entry/response policies are covered by the
  [script/worker policy slice](csp-script-workers.md). Meta policies, top-level
  response-policy integration, hash sources, violation reporting, Trusted Types,
  and general Worker/MessagePort interoperability remain work. Full sandbox
  token coverage and transient-activation lifetime propagation are not claimed.
- Stylesheet discovery/MIME/import ordering is covered; child image/font/media load
  obligations, full CSS encoding rules, and complete resource/observer interoperability
  are not. There is no separate out-of-process frame/site isolation.
- Native Window/MessagePort cloning does not make the separate global
  `structuredClone` or worker messaging implementation fully HTML-conformant.
  Supported transferred endpoints are cleaned up on failed delivery and document
  removal; general garbage collection of live unreferenced ports remains limited.
- Safety bounds: 256 active child contexts, 4,096 lifetime broker client records,
  4,096 MessagePort endpoints, 16 MiB per structured message, 32 MiB queued bytes,
  1,024 window messages, and 256 messages per port. Existing HTML/script/CSS limits
  also apply. Exceeding a bound fails explicitly; it is not a successful benchmark.
- At this slice, longer navigation/responsiveness runs and broader endpoint coverage
  still needed acceptance before removing the HTML search fallback. The later
  [modern-search acceptance](modern-search-acceptance.md) addresses that integration
  decision; it does not turn three successful runs into proof of general iframe
  conformance.

## Native adapter and dependency provenance

The small original C++ adapters in `src/engine/script/engine/v8_api.cc` and
`window_access.cc` compile against public headers from the pinned `v8 = 152.2.0`
dependency. They expose incumbent-context lookup, global detachment/reuse, and
access-checked object templates missing from the Rust binding. They do not copy
V8 layouts, use private/mangled symbols, or vendor V8 sources. Origin-token checks
remain enabled; the scoped adapter permits only the tested HTML cross-origin surface.

The build-only `cc` dependency is MIT OR Apache-2.0; rusty_v8 is MIT and bundled V8
has its upstream BSD license. Locked transitive build-tool licenses are in
`THIRD_PARTY_NOTICES.md`. No existing project dependency supplied a C++ build helper.
The adapter requires C++20 and resolves exact-version Cargo registry headers.
Vendored/custom registries must set `BREEZE_V8_SOURCE_DIR` to that crate directory.
Ambiguous registries or header/library version mismatches fail the build. Browser
and renderer must be rebuilt together: this slice uses IPC major version 14.
