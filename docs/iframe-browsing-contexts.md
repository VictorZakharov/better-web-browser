# Iframe browsing contexts — implementation in progress

The branch replaces the shared synthetic iframe window with separate child
Documents and V8 contexts. The same-origin URL-backed relay in the
[form/navigation fixture](form-submission-and-navigation.md) now passes in Breeze
and Chrome. **This is not complete iframe support or verified live DuckDuckGo
result activation. The planned PR is not merge-ready.**

The proposed cross-origin V8 access-check adapter was blocked by the safety
review and has not been applied; scoped approval was requested. Existing V8
origin checks remain intact. Other remaining work is listed below.

## Implemented contracts

- Connecting an HTML iframe synchronously creates an initial `about:blank`
  Document and a separate global, intrinsic constructors, and event storage.
  The initial Document inherits its parent's base URL and origin unless its
  sandbox requires a unique opaque origin. Its initial readiness is `complete`.
- Native private node brands translate document-local handles across same-origin
  realms. Adopting a node preserves its wrapper identity and original prototype
  while updating its owner Document. The upstream iframe adoption regression is
  fixed without changing its assertion or excluding its test.
- Removal destroys nested navigables and cancels their pending Fetch work.
  Reconnection creates a new context; retained old DOM objects remain usable.
  Document-registry roots are weak so the shared registry does not itself keep
  every replaced Document alive indefinitely.
- `srcdoc` takes precedence over `src`. URL-backed HTML uses the existing Fetch
  transport, redirects, response bounds, decoder, and HTML parser. Response bytes
  are currently buffered before parsing; this is not streaming child navigation.
  Changing a relative `src` resolves it against the embedding Document, not the
  previous child URL. Superseded responses cannot commit a stale navigation.
- Navigation uses V8's public `DetachGlobal`/global-reuse boundary: the
  WindowProxy identity survives, but the Document, global properties, and realm
  are replaced. Old Document objects remain attached to their original host.
- Child parser scripts share the top-level parser-script queue and module-graph
  preparation code. Blocking external scripts pause the parser; `document.write`
  uses the existing parser insertion point rather than reparsing an HTML string.
- A related group of contexts shares one isolate/watchdog. Child timers, promise
  jobs, Fetch completions, and parser work are scheduled through the retained
  runtime. Request identifiers are unique within that group and routed to their
  owning Document. Child load precedes iframe-element load and parent Window load.
- Same-origin `postMessage` uses V8's incumbent context for `event.source` and
  the sender origin, queues asynchronous delivery, checks `targetOrigin`, and
  deserializes into the receiving realm. ArrayBuffer transfer detaches only after
  successful serialization and queue admission. Limits are 16 MiB per message,
  32 MiB queued, and 1,024 queued messages. Transferred buffer bytes count toward
  these limits. Messages targeting a destroyed Document are discarded.
- Native bookkeeping limits active child contexts to 256 per document tree.
  Opaque sandboxed children deny direct DOM access; a sandbox without
  `allow-scripts` prevents loaded scripts from executing. This is **not** full
  sandbox-policy implementation.

The implementation follows HTML's [child navigables](https://html.spec.whatwg.org/multipage/document-sequences.html#child-navigable),
[iframe processing](https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-iframe-element),
[posted messages](https://html.spec.whatwg.org/multipage/web-messaging.html#posting-messages),
and DOM's [adoption algorithm](https://dom.spec.whatwg.org/#concept-node-adopt).
There are no site names or endpoint exceptions in the implementation.

## Evidence, September 19, 2026

| Check | Merged PR #169 release | Working branch | Headless Chrome 153 |
|---|---:|---:|---:|
| Initial-document fixture | 6 / 20 | 20 / 20 | 20 / 20 |
| Same-origin URL/message/navigation relay | Fails | Passes | Passes |

The latest working runs use the `debug` development profile, not a final release
build. The earlier initial-document comparison used `performance`. These are
functional assertions, **not performance measurements**.

At this checkpoint:

- 1,192 library tests pass; one existing test is ignored.
- 129 renderer-process tests pass.
- 82 live-runtime tests pass; three existing live-service tests are ignored.
- Curated upstream WPT: **385/385 files, 3,380/3,380 assertions pass**.
- Clippy with warnings denied, formatting, and source-size checks pass.

No WPT manifest entry, assertion, or acceptance threshold was weakened. The
previous checkpoint's iframe adoption failure is now covered and passing.

```powershell
dotnet build benchmarks/chromium -c Release
./scripts/test-iframe-initial.ps1 -Chrome -OutputDirectory target/iframe-chrome
./scripts/test-iframe-initial.ps1 -Browser target/debug/better-web-browser.exe -OutputDirectory target/iframe-breeze
./scripts/test-form-navigation.ps1 -Browser target/debug/better-web-browser.exe -Cases iframe -OutputDirectory target/iframe-relay
./scripts/test-form-navigation.ps1 -Chrome -Cases iframe -OutputDirectory target/iframe-relay-chrome
cargo test --locked --lib engine::script::runtime::tests::frames
cargo test --locked --lib frame_navigation
```

Browser harnesses remain headless and muted with fresh profiles. Reports stay
under ignored `target/iframe-contexts-proof`; assertions and fixtures are checked
in. The stable release executable remains unchanged at this checkpoint.

## Native API adapter and provenance

`src/engine/script/engine/v8_api.cc` is a small, original adapter compiled against
public headers from the already-pinned `v8 = 152.2.0` Cargo dependency. It exposes
`GetIncumbentContext` and `DetachGlobal`, absent from that binding's Rust API.
It does not copy V8 layouts, use private/mangled symbols, or vendor V8 sources.
The isolate is obtained with V8's public `Isolate::GetCurrent`; Rust's Isolate
wrapper is not passed as a raw C++ isolate pointer.

The build-only `cc` dependency is MIT OR Apache-2.0; rusty_v8 is MIT and its bundled
V8 carries its upstream BSD license. No existing project dependency supplied a
C++ build helper. The adapter requires a C++20 toolchain and resolves the exact
152.2.0 headers in Cargo's registry; vendored/custom registries must set
`BREEZE_V8_SOURCE_DIR` to the resolved crate directory. Ambiguous registries and
header/library version mismatches fail the build.

## Still required before the planned PR is ready

1. Implement and test permitted cross-origin Window/Location capabilities while
   retaining DOM and arbitrary-property denial. Currently V8 rejects cross-origin
   Window access, including `postMessage`; the access-check change awaits approval.
2. Complete sandbox policy inheritance and navigation restrictions. Same-origin
   sandboxed script execution must not gain unauthorized top navigation.
3. Complete child dynamic-script/import, worker, storage, and other effect routing;
   stylesheet-dependent parser blocking and remaining resource lifecycle; nested
   navigation, cancellation, and cache/observer interoperability coverage.
4. Complete messaging platform-object/MessagePort transfer coverage and the
   relevant unmodified upstream messaging tests. Native serialization of built-in
   values and ArrayBuffers alone is not the complete HTML structured-clone contract.
5. Replace buffered child navigation with the shared streaming/encoding-restart
   path and transfer enforceable response policies. X-Frame-Options is checked;
   responses with CSP currently fail closed rather than silently ignoring it.
6. Retry live search → result → Back → another search and record comparable
   evidence. Keep the HTML fallback until that workflow is reliable.
7. Re-run all gates and build the final release head before opening a review-ready PR.

Visual iframe embedding, out-of-process frame isolation, complete history/storage
integration, and full Web Platform Test conformance are not implied by this slice.
