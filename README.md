# Breeze (temporary name)

Breeze is a performance-first browser-engine MVP written in Rust. The product name is provisional and isolated in `src/branding.rs` so it can be replaced without touching engine code.

This is not a Chromium, WebView2, Gecko, or operating-system web-view wrapper. The executable owns its HTML DOM, CSS cascade, JavaScript bindings, layout, display list, resource loading, image/SVG/font decoding, form submission, cookie jar, and Win32 painting path.

## Run it

Requirements: Windows 10/11 x64, PowerShell 7, Rust via rustup (the repository pins
Rust 1.95.0), and the Visual Studio C++ build tools/Windows SDK for the MSVC target.
The first build downloads locked dependencies and the checksum-verified V8 library.

```powershell
./scripts/prepare-v8.ps1 -Profile release
./scripts/prepare-angle.ps1
cargo build --release --locked --bin better-web-browser
./target/release/better-web-browser.exe
./target/release/better-web-browser.exe https://example.org/
```

Persistent cookies and `localStorage` use `%LOCALAPPDATA%\Breeze`; `sessionStorage` remains
tab-scoped and is not written to disk. Tests and isolated automation can set
`BREEZE_PROFILE_DIRECTORY` to an absolute profile directory.
This persistence currently covers top-level documents; child-document state routing
is a [documented iframe limitation](docs/iframe-browsing-contexts.md#deliberate-limits-and-remaining-work).

For a faster optimized edit/build loop, use `cargo run --profile performance`. This profile keeps
optimization enabled but trades release LTO and single-unit code generation for incremental,
parallel compilation. Reproducible performance claims and distributable binaries always use the
canonical `release` profile.

The local rebuild and GitHub Actions timing methodology, latest measurements, and remaining CI
critical path are recorded in [docs/build-performance.md](docs/build-performance.md).

The normal page surface is always the default. **Reader** is an explicit optional feature; navigating or reloading returns to the normal page surface.

Current page support includes:

- HTML5 tree construction with an engine-owned DOM, including [declarative Shadow DOM parsing](docs/declarative-shadow-dom.md)
- [Scoped custom-element registries](docs/scoped-custom-elements.md), shadow-root serialization, slot-change delivery, focus retargeting, and selected CSS shadow selectors and part forwarding
- [Live DOM collection iteration and supported CSSOM property exposure](docs/collections-and-capabilities.md), with mutation-aware iterators and authored capability fallbacks
- [Detached HTML/XML DOMParser documents](docs/detached-document-parsing.md), inert parsing, namespace-aware XML nodes, and shared XHR document-response parsing
- [URL and native request resolution](docs/url-request-resolution.md), explicit public bases, live query parameters, and requests independent of author URL replacements
- [Synchronous document streams and replacement](docs/document-streams-and-pre-wrap.md), plus [parser mutation notifications and autonomous custom-element construction](docs/parser-observation-and-cssom.md)
- A growing CSS cascade with custom properties, [typed comparison lengths](docs/css-comparison-lengths.md), [stepped, exponential and trigonometric math](docs/css-math-functions.md), [cascade layers](docs/css-cascade-layers.md), [nested rules and Selectors Level 4 features](docs/css-nesting-selectors.md), [conditional queries and scoped rules](docs/css-conditional-scope.md), block/inline flow, flex, grid, table, float, and positioned layout
- Standards-based layout fixes and their headless Chrome comparisons are tracked in [layout compatibility](docs/layout-standards.md), including explicit remaining gaps.
- External stylesheets with [nested import loading and separate script/paint gates](docs/stylesheet-loading-dependencies.md), CSS background images, raster images, alpha compositing, inline/external SVG geometry and [bounded shaped SVG text](docs/svg-text-rendering.md), and renderer-owned webfont parsing plus Rust text shaping, fallback, and rasterization
- [AVIF and JPEG XL decoding](docs/modern-images-and-bitmaps.md) through shared page/Canvas/worker paths, with real alpha/color/orientation handling and bounded ImageBitmap crop, resize, ownership and bitmaprenderer presentation contracts
- [ImageDecoder and VideoFrame](docs/image-decoder-and-video-frame.md) with animated GIF/APNG/WebP frame decoding, image streams, eleven 8-bit pixel layouts, frame transfer, and Canvas/worker rendering; complete-input CPU decoding with explicit resource and SDR limits
- [Owned and imported CSSOM](docs/parser-observation-and-cssom.md): preferred titled sheets, per-occurrence import identity, rule edits reflected in the cascade, and constructed/adopted sheets
- A bounded V8 JavaScript runtime with browser Annex B syntax, owned DOM bindings, capture/target/bubble events, retained timers, [native microtasks independent of author Promise implementations](docs/native-microtasks-and-resource-invalidation.md), navigation, and browser-authoritative cookie/storage projections
- [HTML event-handler attributes](docs/html-event-handlers.md), with lazy compilation, DOM scope lookup, stable listener ordering, cancellation, and body/window forwarding
- [IntersectionObserver geometry and queued snapshots](docs/intersection-observer-geometry.md), with containing-block overflow clips, nested scroll margins, and callback microtask checkpoints (remaining geometry and v2-visibility gaps are explicit)
- Progressive document/worker Fetch response streams with bounded backpressure, Fetch/XHR body primitives, abort signals, static/dynamic document ECMAScript modules with top-level await and [inline import maps](docs/import-maps.md), and isolated classic/module dedicated workers
- Native text/search/password/select controls and buttons with renderer-owned state and trusted events, plus [bounded GET/POST form submission](docs/form-submission-and-navigation.md), [live values, reset and constraint validation](docs/form-control-validation.md), submitter/validation events, and synthetic link activation. Default widget rendering and specialized collection APIs retain [documented limits](docs/form-control-validation.md#deliberate-limits-and-remaining-work).
- Character-set decoding from BOM, HTTP headers, or HTML metadata
- A typed Fetch/navigation pipeline with tuple origins, guarded headers, redirect modes, persistent RFC-oriented cookies, CORS/preflight checks, bounded backpressured renderer streams, and document-wide cancellation
- One capability-free Windows AppContainer renderer per tab, owning remote-document decoding, HTML/DOM, JavaScript, trusted input dispatch, CSS/layout, image/font decoding, Workers, and immutable presentation output behind bounded IPC, Job limits, crash recovery, hang detection, and Task Manager controls
- Browser-owned multi-tab contexts with independent history, native-event capture, final scrolling/composition, navigation and Fetch brokerage, in-flight completion routing, and isolated renderer lifecycles
- Desktop tab workflows including Ctrl/Shift multi-selection, ordered drag/reorder, detach/redock across windows, searchable open/recent tabs, Ctrl+N/Ctrl+Shift+W, Ctrl+Shift+A, Ctrl+T/W/Shift+T, Ctrl+Tab/PageUp/PageDown, Ctrl+Shift+PageUp/PageDown, Ctrl+1-9, Ctrl+L/R, F5, Alt+Left/Right, middle-click, and Ctrl+click
- Links, history, reload, scrolling, and background networking

## Task manager

Click **Task manager**. Its modeless popup refreshes every second and shows a process tree rooted at
the privileged browser, with one child row per stable tab/renderer context. Rows report CPU, working/private
memory, handles, uptime, lifecycle state, restarts, and exit diagnostics; document-engine activity
is reported separately. Select a live renderer row and click **End process** to exercise the same
browser-owned termination and reload path used for an unresponsive renderer.

## F12 diagnostics

The bottom-right status-bar counter reports completed content frames for the current tab's active
scroll animation. Press **F12** or click the counter to toggle the native incident panel. It combines
rolling performance data with renderer health, browser-authoritative state-lane pressure, activity
counters, and the newest entries from a bounded per-tab navigation/renderer/Fetch/storage/console
timeline. **Copy diagnostics** exports those details, the last live renderer snapshot, and raw frame
intervals as text for a bug report, including after a contained page failure. The recorder retains
metadata and script diagnostics, not document bodies, and has fixed record and message limits. A
250 ms display timer repaints only browser chrome and the panel surface; those updates are excluded
from the content-frame sequence and cannot inflate its FPS.

## Chromium comparison

The repository-owned public-alpha gate runs Breeze and unified-headless Chromium against seventeen deterministic, original fixtures. Every sample uses a fresh hidden profile on the same machine; the harness aligns viewport, Windows scale, locale, fixture bytes, settle period, and cache policy, then records compatibility captures plus timing, scroll, memory, CPU, and process metrics.

```powershell
.\benchmarks\run-alpha.ps1 -Iterations 3
```

CI is a smoke gate on both pull requests and main, not a standards-conformance suite. It checks source/format, dependency/security policy, harness self-tests, and three small hidden-browser checks: startup/shutdown, containment, and real HTML/CSS/JavaScript/native WebGL rendering. Full-target Clippy, full unit/integration, curated WPT/Khronos and visual/performance suites run locally before pushing relevant changes; their test files and thresholds are retained. CI omits only the auxiliary WPT CLI, not any production browser capability. The local visual matrix requires intact major content, nonblank captures, no Breeze script errors, bounded visual difference, Breeze page-ready no slower than two times Chromium load, and stable six-second early scrolling on long-form fixtures. This is an intentional hosted-coverage tradeoff, not proof of standards compliance. Performance claims remain valid only for feature-equivalent controlled paths. See [the benchmark methodology](benchmarks/README.md), [CI policy and timings](docs/build-performance.md), [modern-search acceptance](docs/modern-search-acceptance.md), and [latest alpha evidence](docs/alpha-compatibility.md).

### Last performance assessment — September 19, 2026 (#168)

The [collections/readiness slice](docs/collections-and-capabilities.md) fixes live
attribute/token/child iteration and false CSSOM capability detection, and avoids
building unpresentable display lists while initial rendering is blocked. Three
fresh hidden release trials per Wikipedia page compare merged #167 with this slice.

| Matched before/after measurement | Before | After |
|---|---:|---:|
| Curated WPT assertions | 3,354 pass | 3,372 pass |
| Closed modern-DDG side menu | Covers results | Outside viewport |
| Main Page first presentation | 611 ms | 416 ms (-32%) |
| Coron first presentation | 586 ms | 508 ms (-13%) |
| Main Page layout/paint work | 317 ms | 259 ms |
| Coron layout/paint work | 893 ms | 1,085 ms (increased) |

**Earlier first paint is not complete-page readiness.** Coron's Appearance controls
still populate between 2.0 and 2.5 seconds in both representative Breeze filmstrips;
Chrome shows them in its first 0.5-second sample. Total CPU rose in this live series,
and late script activity produced memory outliers. No general speed or memory win
is claimed. Full-document style/layout and late widget readiness remain priorities.

| Page | Breeze first presentation | Chrome load / FCP | Working set B/C | Private B/C |
|---|---:|---:|---:|---:|
| Main Page | 416 ms | 818 / 324 ms | 146.7 / 645.1 MiB | 113.9 / 374.8 MiB |
| Coron | 508 ms | 717 / 271 ms | 191.6 / 676.3 MiB | 158.3 / 407.8 MiB |

These readiness milestones and feature sets differ; this is not a browser-speed
ratio. Process-tree memory may double-count shared pages. Hidden-window creation
(about 10 ms) is not comparable interactive startup to Chrome debugger readiness
(about 202–216 ms).

All **48 owned Breeze/Chrome fixture pairs** passed in that assessment. Modern DDG's
closed-menu overlap was fixed, but form submission and result activation failed.
The subsequent [form/navigation slice](docs/form-submission-and-navigation.md) adds
`submit()` / `requestSubmit()`, GET/POST transport, and correct input-type reflection.
Live search submission now reaches the new query. The subsequent
[child-context/navigation slice](docs/iframe-browsing-contexts.md) loads child HTML
in separate realms, supports guarded cross-origin messaging and MessagePort transfer,
and fixes Back after script-driven result navigation. Owned relay/Back/search cases
pass in Breeze and headless Chrome; three fresh release Breeze runs also complete
modern DDG → Wikipedia result → Back → another search, each with ten final result
links and no reported JavaScript/console errors. Chrome did not present the
same requested live result link, so no live performance comparison is claimed.
Visual iframe embedding, persistent child state, complete policy support, and broader
live reliability remain unfinished.

User testing then exposed two entrypoints missing from that assessment: the DDG
homepage crashed, and HTML-results dropdown options spilled into the page.
The [entrypoint follow-up](docs/ddg-entrypoint-regressions.md) adds native DOM equality,
blockified-select rendering, framework-compatible user editing, correct empty
button labels, and per-element SVG colour. It records the reproduced failures and
adds a homepage/HTML-results acceptance script. Earlier direct-results evidence
must not be read as a claim that every DDG entrypoint or visual detail worked.

The final [modern-search acceptance](docs/modern-search-acceptance.md) adds an
original composed application fixture and a strict live result → Back → Unicode
re-search case. Address-bar searches now use DuckDuckGo's modern `/?q=…&ia=web`
route; the HTML endpoint remains covered as a compatibility regression, not as the
browser default.

The [full September 19 assessment](docs/browser-performance-readiness-2026-09-19.md)
records the before/after results, populated-UI evidence, validation, outliers, and
remaining standards gaps. [PR #167's assessment](docs/browser-performance-url-2026-09-16.md)
retains the preceding measurements and limitations.

### Historical renderer text cold-path comparison

Three-run hidden release medians on the Wikipedia earthquake fixture compare the historical GDI
control, the first renderer-owned COSMIC Text implementation, and the lean renderer-owned
pipeline:

| Text backend | Page ready | Non-network | Layout/paint | Working set |
|---|---:|---:|---:|---:|
| GDI control | 576.915 ms | 223.193 ms | 105.540 ms | 155.7 MiB |
| COSMIC Text | 714.472 ms | 321.404 ms | 214.886 ms | 179.668 MiB |
| Fontique + HarfRust + Swash | 534.731 ms | 226.830 ms | 131.399 ms | 183.672 MiB |

These are historical measurements from ADR 0003, not timings remeasured for the current HEAD.
The current path keeps hostile font bytes, advanced shaping, and rasterization inside the
AppContainer renderer. It recovers page-ready time but not the original GDI memory footprint; the
full method, scroll results, per-stage profile, and outlier record are in
[ADR 0003](docs/architecture/0003-lean-renderer-text-pipeline.md).
Browser-owned cookie/Web Storage authority, persistence, bounded Fetch streaming, and the
`cookie_store` dependency decision are documented in
[ADR 0004](docs/architecture/0004-browser-state-and-fetch-broker.md).

## Architecture

The browser retains network and OS authority; each tab's AppContainer renderer owns untrusted page
execution and sends back a bounded, immutable presentation:

```text
Browser:  URL/history -> Fetch policy -> WinHTTP -> response bytes
                    `-> origins/CORS/cookies/redirects/cancellation
                                      |
                              bounded typed IPC
                                      v
Renderer: charset decode -> HTML5 DOM -> JavaScript/DOM mutation
                                  |-> brokered Fetch intents
                                  |-> CSS and resource decode
                                  `-> layout -> text shape/raster -> immutable presentation
                                                                       |
                                                               validated typed IPC
                                                                       v
Browser:                            validated pixel composition and native controls
                                      ^                         |
                                      `-- presented/controls ---'
                    native input/lifecycle -- typed IPC --> Renderer DOM dispatch
```

The page and Reader surfaces share navigation and networking, but Reader extraction is never selected automatically.
The networking boundary and its standards/platform ownership are documented in
[docs/fetch-pipeline.md](docs/fetch-pipeline.md).
The accepted renderer isolation boundary, threat model, IPC contract, and staged Windows migration
are documented in
[ADR 0001](docs/architecture/0001-renderer-process-boundary.md).
The renderer-owned font-byte boundary is documented in
[ADR 0002](docs/architecture/0002-renderer-owned-text-stack.md); its measured lean text pipeline
supersession is documented in
[ADR 0003](docs/architecture/0003-lean-renderer-text-pipeline.md).
Central hostile-input budgets, decoder preflights, renderer termination behavior, and the fuzzing
contract are documented in [docs/security-and-fuzzing.md](docs/security-and-fuzzing.md).

## Verification

```powershell
./scripts/prepare-v8.ps1 -Profile debug
./scripts/prepare-angle.ps1
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all -- --check
./scripts/check-source-size.ps1
./scripts/prepare-v8.ps1 -Profile release
./scripts/prepare-angle.ps1
cargo build --release --locked --bin better-web-browser
./scripts/run-fuzz-smoke.ps1

.\scripts\run-hidden-benchmark.ps1 `
  -Url https://example.org/ `
  -Output result.json `
  -Screenshot result.png `
  -ScrollSamples 12 `
  -WindowWidth 1920 `
  -WindowHeight 1080 `
  -DiagnosticSelector '#main' `
  -SettleMs 2000
```

Run renderer and live-runtime integration tests as your normal Windows user, not a restricted
filesystem-sandbox identity: they need access to that user's AppContainer profile. Automated
browser tests must remain hidden, and reference Chromium runs must use unified `--headless` and
`--mute-audio`. The Chromium comparison additionally requires the .NET 8 SDK and installed Chrome.

The hidden-benchmark wrapper verifies the child launch and enforces its automation guard.
`--screenshot` paints an offscreen PNG for visual
verification without putting a browser window on the desktop. Repeatable `--diagnostic-selector`
options add bounded computed-style, resource-decode, and native-control geometry facts to the JSON
report; omit them during normal measurements. `--early-scroll-trace` starts at page-ready and
drives a six-second, 16 ms scroll schedule through the same offscreen paint path. Its JSON report
includes input-to-paint latency plus per-sample script, resource, style, layout, invalidation, and
paint activity plus bounded native host-call timing, making post-load responsiveness regressions
reproducible without visible UI. While scrolling remains active, Breeze gives input priority over
post-load timer and async-script tasks; deferred timer work resumes in batches of at most eight
callbacks after a 100 ms quiet period. Runtime-only progress crosses IPC without rebuilding or
installing an immutable presentation; DOM/style/resource invalidation is required before the
renderer emits new visual output.

For a delayed native document scroll, pass `-ScrollTarget @('800', '0') -NavigationDelayMs 1500`
to `run-hidden-benchmark.ps1`. Targets are integer CSS-pixel y offsets
from 0 to 2147483647, converted to native pixels and clamped to the document's scroll range.
They run in array order after the other action groups, with the navigation delay before each
action, then the usual settle/capture period. The underlying repeatable `--scroll-after-ready`
option preserves command-line ordering with other actions and delivers the ordinary scroll
events; it does not bypass native scrolling or directly mutate the page's JavaScript state.

### Web-platform regression suite

A pinned, curated 546-file Web Platform Test suite covers 5,960 upstream harness subtests across HTML
and detached HTML/XML parsing, DOM and mutation, events, event-loop ordering, URLs, Fetch/XHR, cookies, forms, modules,
Web IDL, window/port messaging, [Web Storage values and persistence](docs/web-storage.md), User Timing/PerformanceObserver,
and CSS cascade/selectors/layout, media and feature queries, scoped rules and imports, CSSOM,
and stylesheet MIME validation. Upstream fixtures stay in a separate sparse WPT checkout;
after preparing that checkout, the suite runs offline with one hidden command. All 5,960 selected
subtests pass at the pinned revision in the 2026-09-26 hidden run, with no expected-failure,
skip, or timeout allowances:

```powershell
.\scripts\checkout-wpt.ps1 -Destination ..\wpt
.\scripts\run-wpt.ps1 -WptRoot ..\wpt
```

The [form/navigation slice](docs/form-submission-and-navigation.md) adds upstream submit/formdata
event-construction coverage plus owned cross-browser navigation checks. The earlier
[URL/request slice](docs/url-request-resolution.md) documents native parsing,
headless browser checks, and remaining parser boundaries. The earlier
[document-stream slice](docs/document-streams-and-pre-wrap.md) records replacement semantics.

The runner emits `target/wpt/report.json`, enforces the 200-subtest minimum, and fails on
regressions, crashes, changed failure modes, and unexpected passes. This is a focused regression
gate, not Breeze's whole-platform pass rate. Both former discovery cases are now strict regressions;
the separate [URL parser discovery suite](docs/url-request-resolution.md) still reports 85 failing
assertions across its three broader parser files, with no expected-failure masking.
[Inline-script and table geometry limits](docs/inline-scripts-and-table-geometry.md) remain explicit. See
[tests/wpt/README.md](tests/wpt/README.md) for the selection rationale, wptrunner evaluation,
provenance, licensing, expectation policy, filtering, and exact execution contract.

Window and dedicated Worker realms deliver real marks and measures through asynchronous
`PerformanceObserver` tasks, including buffered observation and cloned entry details. The
[timing compatibility contract](docs/performance-timeline.md) documents the monotonic clock,
supported entry types, and the remaining Resource/Navigation/Paint/Long Task boundaries.

A second in-repository parser suite runs selected WPT tree-construction fixtures directly against
the engine-owned DOM. It covers implied elements, foster parenting, adoption-agency repair,
templates, foreign namespaces, both `noscript` modes, malformed attributes, fragment contexts, and
deep malformed-input safety:

```powershell
cargo test --test html_parser_conformance
```

See [tests/html-parser/README.md](tests/html-parser/README.md) for the pinned upstream revision,
fixture provenance, structural serialization contract, and intentionally unsupported error-count
comparison.

The hostile-input suite deterministically replays the committed HTML, fragment, CSS, URL, DOM, and
JavaScript-host fuzz corpora on stable Windows. Coverage-guided runs use the separate pinned fuzz
workspace and scheduled Linux workflow; see [fuzz/README.md](fuzz/README.md).

## Support matrix and current limitations

`☑` means the high-level capability is supported, `◩` means useful foundations exist but
important behavior is incomplete, and `☐` means the capability is not implemented.

| Status | Area | Details |
| --- | --- | --- |
| ◩ | Host platforms | The native shell runs on Windows; macOS and Linux shells are not implemented. |
| ◩ | HTML and DOM | The engine owns its DOM and implements substantial HTML5 tree construction, mutation, and event propagation behavior, including named and manual Shadow DOM slot assignment. Web-platform conformance is still incomplete. |
| ◩ | CSS, layout, and painting | The cascade, [cascade layers](docs/css-cascade-layers.md), custom properties, calculated lengths, common block/inline, flex, grid, table, float, and positioned layouts, images, SVG, and webfonts work on selected pages. Selector, layout, invalidation, and painting coverage remain incomplete. |
| ◩ | JavaScript and browser APIs | A bounded retained V8 realm provides owned DOM bindings, capture/target/bubble events, trusted pointer/keyboard/text/focus/scroll/visibility dispatch, timers, microtasks, navigation, browser-authoritative cookie/storage projections, and other early browser APIs. IME/composition and cancelable `beforeinput`, many HTML event-loop sources, and much of the wider browser API surface remain incomplete. |
| ☑ | HTTP navigation policy | Typed navigation and Fetch policy cover tuple origins, guarded headers, redirects, scoped cookies, CORS/preflight checks, bounded bodies, and document-wide cancellation. This is an early implementation rather than a security-audited replacement for a mature browser network stack. |
| ◩ | Cookies and Web Storage | Browser-owned cookies implement RFC-oriented domain/path, expiry, public-suffix, Secure, HttpOnly, SameSite, prefix, ordering, quota, and restart-persistence behavior. Origin-scoped `localStorage` persists; `sessionStorage` is tab-scoped. Named properties preserve UTF-16 values, and same-origin tabs synchronize local storage with ordered `storage` events. Child-frame event scope, partitioned state, and user-facing data controls remain incomplete; see the [storage contract](docs/web-storage.md). |
| ◩ | IndexedDB | Browser-owned origin-scoped databases provide asynchronous transactions, ordered keys/ranges/cursors, structured-cloned values, restart persistence, and secondary indexes with unique, multi-entry, compound-key-path, and cursor operations. Dedicated workers use the same browser-owned store; full cross-tab scheduling and connection blocking/versionchange coordination remain incomplete. See the [standards slice](docs/html5test-websocket-indexeddb-forms.md). |
| ◩ | CacheStorage | Secure top-level pages and dedicated workers can store and query origin-scoped named `Cache` objects through a browser-owned, restart-persistent store. `add`/`addAll` use Fetch policy and batch writes atomically. Secure descendant-frame context calculation and Service Workers remain open; see the [Cache API contract](docs/cache-storage.md). |
| ◩ | StorageManager | Secure top-level pages and dedicated workers expose `navigator.storage.estimate()` with rough browser-owned usage across localStorage, IndexedDB, and CacheStorage. Its fixed logical 37 MiB quota reflects independent backend caps, not reservable disk space. The default bucket is best-effort (`persisted()` is false); `persist()` and persistence promotion are not implemented. See the [StorageManager contract](docs/storage-manager.md). |
| ◩ | JavaScript Fetch and XHR | Document and dedicated-worker Fetch bodies stream progressively through default readers, with byte-based backpressure, cloning, and cancellation. Fetch/XHR and Body primitives are implemented; BYOB, streaming uploads, and complete pipe/transform semantics remain incomplete. See the [streaming contract and measurements](docs/progressive-fetch.md). |
| ◩ | Beacon, Fetch keepalive, and speech synthesis | `navigator.sendBeacon()` queues browser-owned POST delivery across navigation, while bounded `fetch(..., {keepalive: true})` uploads retain their admitted origin and policy after navigation and share the in-flight body budget with Beacon. Windows Web Speech synthesis uses browser-owned SAPI voices and per-document queues after trusted activation. Renderer-side admission cannot yet predict all browser-wide Beacon saturation; speech recognition and boundary events remain open. See the [keepalive contract](docs/fetch-keepalive.md) and [speech batch](docs/html5test-beacon-speech-shadow-prefetch.md). |
| ◩ | WebSocket | Document and dedicated-worker WebSockets use browser-owned network transport, bounded IPC, per-client CSP/mixed-content policy, text/binary frames, and close events. Extensions, compression, and broader network interoperability remain incomplete; see the [standards slice](docs/html5test-websocket-indexeddb-forms.md). |
| ◩ | Notifications | Secure document and child-frame clients can use session-scoped, origin-keyed permission grants and browser-owned Windows notifications. A new permission prompt requires top-level transient activation. Persistent permission, frame-scoped prompts, service-worker notifications, and non-Windows presentation remain open; see the [Notification contract](docs/notifications.md). |
| ◩ | Custom protocol handlers | Secure top-level pages can request a browser-owned handler for safelisted or `web+` schemes. Explicit, gesture-bound consent and a bounded profile store gate subsequent link navigation; Fetch and operating-system defaults are unchanged. Embedded-frame support and a management UI remain open; see the [handler contract](docs/custom-protocol-handlers.md). |
| ◩ | Async Clipboard text | Secure, visible top-level pages can `readText()` and `writeText()` through the actual Windows clipboard after trusted activation and separate per-origin session consent. Bounded IPC and document/session revalidation keep the OS boundary browser-owned. Rich `ClipboardItem` read/write, iframe delegation, and persistent grants remain open; see the [Clipboard contract](docs/async-clipboard.md). |
| ◩ | Screen Wake Lock | Secure, visible top-level pages can request a browser-owned screen wake lock backed by Windows power requests. Locks release on document or visibility loss, tab switch, and shutdown. Permissions-Policy header enforcement, an active-lock indicator, and workers remain open; see the [wake-lock contract](docs/screen-wake-lock.md). |
| ◩ | Permissions API | Window documents expose non-prompting queries for six permission names. Secure top-level clients receive browser-authoritative states and change events; insecure and child clients resolve `denied`. Other names, worker exposure, and broader descendant-frame policy remain open; see the [Permissions API contract](docs/permissions-api.md). |
| ◩ | BroadcastChannel | Same-origin top-level documents can exchange structured-cloned messages across tabs through browser-owned, bounded membership and delivery queues. Embedded documents and workers await storage-key/lifecycle integration; see the [BroadcastChannel contract](docs/broadcast-channel.md). |
| ◩ | File API and uploads | Memory-backed `Blob` slices share private immutable chunks, byte streams pull bounded chunks with BYOB readers, and dedicated workers can use all four `FileReaderSync` read formats. A [browser-owned file picker and bounded upload path](docs/file-upload.md) expose selected `FileList` snapshots without local paths and send byte-exact multipart form bodies, including worker `FormData`. Large-file streaming and complete File API coverage remain open; see also the [worker-read contract](docs/file-api-worker-reads.md). |
| ◩ | Location and media-device discovery | Secure top-level pages can request real Windows location readings after an origin-scoped session permission prompt. `navigator.mediaDevices.enumerateDevices()` reports only the presence of microphones and cameras, without identifying them before a capture grant. Background/minimized requests wait, and navigation retires outstanding work. Child-frame permissions policy, camera/microphone capture, and device-change events remain unavailable; see the [location](docs/geolocation.md) and [device enumeration](docs/media-devices-enumeration.md) contracts. |
| ◩ | URLPattern | A bounded Window `URLPattern` subset matches component dictionaries, absolute and relative constructor strings (including non-special opaque paths), named and repeated path segments, literal-affixed captures, hostname labels, and a terminal wildcard. Custom regular expressions, complete constructor-string grammar, and worker exposure remain open; see the [URLPattern contract](docs/url-pattern.md). |
| ◩ | ECMAScript modules | Static graphs, top-level `await`, [dynamic document JavaScript modules](docs/dynamic-modules.md), and parser-inserted [inline import maps](docs/import-maps.md) are implemented. Script-created maps, map integrity enforcement, import attributes, other module types, and dynamic worker imports remain gaps. |
| ◩ | Web Workers | Isolated classic and module dedicated workers are implemented. Shared Workers and Service Workers are not. |
| ◩ | Web Crypto and Gamepad | Secure Window and dedicated-worker realms expose CNG-backed digests, HMAC, AES, key derivation, NIST-curve ECDH/ECDSA, and RSA-OAEP/PSS/PKCS#1 v1.5 with bounded key import/export. The Windows Gamepad API polls XInput controllers after user interaction. Other algorithms, hardware-backed key storage, non-XInput devices, and haptics remain gaps; see the [current standards slice](docs/html5test-indexes-crypto-media.md). |
| ◩ | Device sensors | Windows WinRT-backed relative/absolute Device Orientation and Motion events plus Generic Accelerometer, Linear Acceleration, Gravity, Gyroscope, Magnetometer, Relative/Absolute Orientation, and Ambient Light sensors deliver permissioned physical readings to visible secure top-level documents through bounded IPC. Screen-reference transforms, iframe delegation, and persistent grants remain open; see the [sensor contract](docs/sensors.md). |
| ◩ | Script scheduling | Streaming parsing, [synchronous writes](docs/synchronous-document-write.md), [document replacement](docs/document-streams-and-pre-wrap.md), [synchronous dynamic inline classics](docs/inline-scripts-and-table-geometry.md), parser mutation notifications, autonomous custom-element construction/reactions, independently ready classic `async` scripts, deferred/module readiness, [dynamic module insertion](docs/dynamic-modules.md), and document load tasks are implemented slices. Stylesheets have separate parser-script and paint gates. Customized built-ins and the complete HTML rendering/event-loop model remain incomplete. |
| ◩ | Images and fonts | Document images, CSS backgrounds, SVG, alpha compositing, detached JavaScript-created `Image` fetch/decode, and webfonts are supported. The sandboxed renderer owns font parsing, advanced shaping, fallback, and glyph rasterization; the browser validates and composites only bounded raster assets and placements, so remote font bytes never enter the privileged process. CSS Fonts coverage, variable-font controls, vertical text, and broader image resource-selection behavior remain incomplete. |
| ◩ | Resource hints and CSP | Connected `dns-prefetch` and `preconnect` links perform bounded origin-only DNS warmup; `preconnect` does not yet establish a reusable connection. Bounded `prefetch` handles same- and cross-origin documents and subresources with potential-CORS credentials, redirect CSP checks, completion events, and compatible private-cache reuse. Response and eligible `head` meta CSP govern resource admission and eval/Wasm generation, including inherited worker policy; inline script hashes use SHA-256/384/512. Embedded-document prefetch, integrity-bearing hints, report-only delivery, style hashes, and full CSP3 remain open; see the [implementation contract](docs/html-media-hints-csp.md). |
| ◩ | Forms and input | Native text, search, password, select, file, and button controls use renderer-owned DOM state and default actions. GET forms and bounded multipart file uploads are supported. Checkboxes/radios have separate checked/default state, activation, grouping, and reset behavior. Date/month/week/time/datetime-local numeric values, applicable UTC date values, color sanitization, and [meter/progress and text-length reflection](docs/form-numeric-reflection.md) have targeted standards coverage. [Text-control selection](docs/text-control-selection.md) supports the applicable input states and textarea with UTF-16 offsets, `setSelectionRange()`, and `setRangeText()`, mirrored to Windows edits. Ordinary native typing and Backspace/Delete dispatch cancelable `beforeinput` with generation-fenced rollback; paste, IME/composition, and accessibility edits retain a narrower legacy path. Control styling, larger uploads, broader form/reset behavior, and document text selection remain incomplete. |
| ◩ | Popover | Connected HTML popovers support auto, manual, and hint modes, top-layer paint and hit testing, invoker activation, bounded light dismiss, focus behavior, and toggle events. Full nested/Shadow DOM interoperability and every Popover API edge case remain open; see the [Popover contract](docs/popover-api.md). |
| ☑ | Tabs and windows | Multiple live tabs, [browser-owned session history](docs/history-traversal.md) with structured-cloned state, per-entry viewport scroll restoration (`history.scrollRestoration`), and same-document Back/Forward, tab search and restoration, keyboard shortcuts, multi-selection, reordering, and detach/redock across windows are supported. Back/forward document caching, restoration of nested scroll containers, and persistent tab sessions across browser restarts are not. |
| ◩ | Canvas, media, and downloads | Bounded software Canvas 2D provides real sRGB pixels, SVG paths, fills/strokes, gradients/patterns, clipping, shadows, filters, compositing, shaped/rasterized text, Geometry Interfaces, `ImageData`, image drawing, `ImageBitmap`, `OffscreenCanvas` (including workers), and PNG/JPEG/WebP export. Dirty document and child-frame Canvas bitmaps participate in live image presentation. The contained media worker plays URL-backed PCM WAV, MP3, ordinary AAC/M4A, Ogg/Vorbis, native FLAC, ADTS AAC-LC, audio-only WebM/Vorbis, Ogg/FLAC, and mapping-family-0 Ogg/WebM Opus with bundled decoders, plus H.264/AAC MP4 and H.264-only MP4 with a video clock; it also supports synchronized XAudio2 output, progressive Media Source input, and play/pause/seek/volume/mute/fullscreen controls. Browser-owned [camera and microphone grants](docs/media-capture.md) provide a bounded `getUserMedia` path, with captured video presented through `<video srcObject>`. One granted microphone track can be recorded as actual FLAC or [Ogg/Opus](docs/ogg-opus.md) / [WebM/Opus](docs/webm-opus.md) with bounded [`MediaRecorder`](docs/audio-codecs-and-recording.md); Opus recording accepts native 8/12/16/24/48 kHz, not 44.1 kHz capture. [Window/Worker encoding and decoding queries](docs/media-capabilities.md) use the implemented codec matrices without claiming hardware efficiency. HTML `<source>` fallback and stale-response rejection are tested. Text tracks can load WebVTT and paint bounded captions; audio/video track lists expose the accepted decoder streams and allow disabling their output. [Container limits](docs/encoded-audio-containers.md) include WebM video/multiple tracks, and streaming the new complete-file formats. [Other remaining limits](docs/html-media-hints-csp.md) include DRM, detached `new Audio(src)` loading, multiple selectable decoded streams, full caption styling/regions, picture-in-picture, and mature downloads. |
| ◩ | Web Audio | `OfflineAudioContext` renders bounded 128-frame `Float32` PCM graphs with buffer, oscillator, constant, gain, filter, delay, panning, channel-routing, WaveShaper, Analyser, DynamicsCompressor, and Convolver nodes. Shared [speaker/discrete mixing, channel modes, and legal delayed feedback](docs/web-audio-routing.md) preserve wide intermediate buses and pending tails. Deprecated `ScriptProcessorNode` supplies real author-produced PCM and asynchronous processing events; it is not AudioWorklet. A live `AudioContext` sends rendered PCM through the contained media worker to XAudio2 after user activation, with bounded backpressure and lifecycle handling. `decodeAudioData` asynchronously decodes supported PCM/float WAV, FLAC, Ogg/Vorbis, MP3, AAC/M4A, ADTS AAC-LC, audio-only WebM/Vorbis, Ogg/FLAC, and mapping-family-0 Ogg/WebM Opus into resampled `AudioBuffer` data with promise and callback completion. A [browser-granted captured microphone](docs/media-capture.md) can feed a live graph through `MediaStreamAudioSourceNode`, including an analyser-only branch. `AudioWorklet`, media-element sources, additional encoded formats, and full conformance remain unavailable; see the [supported scope and limits](docs/web-audio.md), [codec contracts](docs/audio-codecs-and-recording.md), and [new container boundaries](docs/encoded-audio-containers.md). |
| ◩ | Web Animations, CSS Animations, and Transitions | Script-created and stylesheet keyframes sample through the native cascade/paint path, with timelines, effect inspection, lifecycle events, scoped/layered names, and live keyframe CSSOM editing. Attribute-driven transitions include ancestor changes, a dedicated cascade origin, and reversing-shortening. Pseudo-elements, additive compositing, and compositor offloading remain open; see the [CSS animation](docs/css-animations.md), [Web Animations](docs/html5test-animation-media-csp.md), and [transition](docs/css-transitions.md) contracts. |
| ◩ | WebGL 1 | Windows Canvas/OffscreenCanvas use real ANGLE shader rendering, preferring hardware D3D11 with software WARP fallback. HDR textures/targets, depth, sRGB, MRT, compressed textures, instancing, vertex arrays, workers and loss/restoration have native pixel tests. Nineteen native extensions are capability-gated; the GLES2 provider preserves WebGL1 shader rules. WebGL1 antialiasing and full conformance remain open; see the [backend contract](docs/webgl-backend.md) and [extension coverage](docs/webgl-lifecycle-and-extensions.md). |
| ◩ | WebGL 2 | Windows Canvas/OffscreenCanvas admit a distinct public WebGL2 context backed by ANGLE: ESSL300, volume/array and sized textures, typed readback, multisample resolves, uniform buffers, transform feedback, samplers, queries and fences. Real hardware selection honors power hints where available; strict performance-caveat creation disables software fallback. This is bounded tested functionality, not full conformance or proof that the sibling game runs. See [native contracts and remaining gaps](docs/webgl2-foundations.md). WebGPU remains open. |
| ◩ | Accessibility | A bounded renderer semantic tree is validated and exposed with browser chrome through AccessKit and Windows UI Automation, including focus/invoke/value actions. Accessible-name/ARIA coverage, rich text patterns, live regions, and non-Windows adapters remain incomplete; see [Accessibility architecture](docs/accessibility.md). |
| ◩ | Process and site isolation | Each tab has a capability-free AppContainer renderer that owns remote-document parsing, JavaScript/DOM, CSS/layout, image/font decoding, Workers, and immutable presentation construction. The browser reconstructs privileged Fetch requests and owns persistent state; bounded IPC/queues, Job limits, hang detection, and tab-local containment cover aborts, access violations, OOM termination, and native stack overflow. Cross-site frame isolation is not implemented. |
| ☐ | Security-audited browsing | The browser has not received a security audit and is not suitable for sensitive authenticated browsing. |

See [JavaScript networking, modules, and workers](docs/javascript-network-runtime.md) for the
implemented contracts, ownership model, standards references, and narrower remaining boundaries.
The [DOM traversal and editing notes](docs/dom-traversal-editing.md) describe live
iterators/ranges, editable-state reflection, drag-data phases, and their remaining limits.

[Cooperative idle scheduling](docs/idle-callback-scheduling.md) covers scheduler-backed
`requestIdleCallback`, bounded native deadlines, timeout races, cancellation, and task fairness.

[ResizeObserver delivery](docs/resize-observer-delivery.md) covers native content/border box
measurements, branded entries, depth-limited callbacks, and before-paint updates. Vertical-writing,
SVG/iframe geometry, and broader rendering-loop compatibility remain incomplete.

The [loading standards implementation sequence](docs/loading-standards.md) records the
implementation sequence and owned-fixture acceptance criteria. Later slices are documented in
[stylesheet dependencies](docs/stylesheet-loading-dependencies.md), [document streams](docs/document-streams-and-pre-wrap.md),
and [parser observation/CSSOM ownership](docs/parser-observation-and-cssom.md). These are bounded
standards contracts, not a claim of complete HTML loading or Chromium-level startup performance.

The [technical-alpha release notes](docs/technical-alpha-release.md) describe the reproducible
unsigned Windows x64 archive, verification and cleanup, acceptance evidence, dependency policy,
and the safety limitations that apply before trying a public build. Development-only licenses and
provenance that are intentionally absent from the shipped graph are tracked separately in
[development third-party material](docs/development-third-party.md).

The deterministic alpha matrix covers long-form and portal pages, responsive articles, search
results, a capability dashboard, forms/storage, layout, media/fonts, and asynchronous mutation.
Its opt-in live URLs are observations, not CI truth. Modern Google results are not an accepted
compatibility baseline: previous tests encountered anti-automation responses, which can change
with profile, network, and time. Breeze renders the actual response; it does not silently substitute
another search provider. Passing a deterministic search fixture does not establish live Google support.
The [CSP3, Worker messaging, and embedded-document slice](docs/csp-script-workers.md)
records the challenge-page diagnosis and hidden iframe/image verification without
claiming that Google will serve results in a normal session.

The 2026-09-23 fresh-profile hidden release run for the Canvas bitmap/OffscreenCanvas slice
rendered **337 / 588** on HTML5test, up from **334 / 588** on the preceding
Canvas 2D slice, with zero JavaScript errors and no renderer exit.
The broader Canvas/Geometry/DOM standards batch in PR #180 remained at **337 / 588**
in the same hidden release conditions, also with zero JavaScript errors and
no renderer exit; the score does not measure most of those behavioral changes.
The preceding [EventSource/scroll-into-view slice](docs/html5test-eventsource-scroll.md)
rendered **323 / 588** on the same date. The 11-point increase reflects tested Canvas
path, ellipse, dash, blend, and export features in the prior slice; the additional
three points reflect bitmap and OffscreenCanvas capability probes. See the
[Canvas implementation and limitations](docs/html5test-canvas.md) for the behavioral scope.
The next [WebSocket, IndexedDB, and temporal-input slice](docs/html5test-websocket-indexeddb-forms.md)
rendered **379 / 588** in a 2026-09-23 fresh-profile hidden release run at the
same 1280×720 window, 125% scale, and `en-US` locale: **+42** versus the
preceding 337 / 588 baseline, with no JavaScript errors or renderer exits.
Intermediate headless runs measured 352 after WebSocket, 354 after the
temporal/color controls, and 379 after IndexedDB. These are feature-probe
observations, not a measure of full API conformance.
The preceding [indexes, Canvas text, Web Crypto, WebVTT, and Gamepad slice](docs/html5test-indexes-crypto-media.md)
renders **396 / 588** in a 2026-09-24 fresh-profile hidden release run
at 1280×720, 125% scale, and `en-US`: **+17** versus the prior 379 / 588
release observation. The added APIs are tested beyond
the site's feature probes; the score alone does not establish their completeness.
The score does not imply that the site's layout is pixel-correct or that every detected API is complete.
HTML5test is a capability inventory, not a percentage of browser completion or a conformance claim;
specifications and individual Web Platform Tests define the implementation/regression contracts.

The subsequent [resource loading, CSS Font Loading, responsive images, and forms batch](docs/resource-loading-fonts-responsive-forms.md)
adds bounded private HTTP caching, Subresource Integrity, preloads and modulepreloads,
WOFF2 decoding, document font APIs, DPR-aware source selection, image submit controls,
and programmatic FileList assignment. Its acceptance tests cover actual bytes,
network outcomes, rendering, and form entries; the HTML5test score alone does not
capture most of these contracts. At that point the file picker and full module graph remained open.
The 2026-09-24 fresh-profile hidden release run for this batch rendered
**401 / 588**, **+5** over the preceding 396 / 588 release observation, at
1280×720, 125% scale, `en-US`, and 10 seconds' settle. It returned HTTP 200
with zero JavaScript errors and no renderer exit. The score is an inventory
observation, not an assertion that every newly added API is complete.

The subsequent [image, editing, SVG filter, Web Animations, media-track, and CSP batch](docs/html5test-animation-media-csp.md)
rendered **420 / 588** with Breeze's default identity in a 2026-09-24
fresh-profile hidden run at 1280×720, 125% scale, `en-US`, and 10 seconds'
settle: **+19** over the preceding 401 / 588 release observation. In an
isolated Chrome-compatible User-Agent profile it rendered **432 / 588**;
the user-reported pre-batch baseline for that mode was **411 / 588** (**+21**).
Both runs returned HTTP 200 without JavaScript errors or renderer exits.
Different User-Agent modes can receive different test code, so their scores
should not be treated as interchangeable or as full conformance evidence.

The subsequent [CSS cascade layers, nesting, and Selectors Level 4 slice](docs/css-nesting-selectors.md)
rendered **423 / 588** with Breeze's default identity in a 2026-09-26
fresh-profile hidden release run using the same viewport, scale, locale, and
10-second settle: **+3** versus the prior dated 420 / 588 observation. It
returned HTTP 200 with zero JavaScript errors and no renderer exit. This is a
cross-date observation, not a controlled attribution of those three points
to CSS changes.

The [conditional CSS, scoped cascade, and CSSOM batch](docs/css-conditional-scope.md)
rendered **423 / 588** with Breeze's default identity in a 2026-09-26
fresh-profile hidden release run at the same viewport, scale, locale, and
10-second settle: **unchanged** from the preceding 423 / 588 observation.
The run returned HTTP 200, showed no JavaScript errors or renderer exits,
and rendered the score in the captured image. HTML5test does not award
points for most media-query, feature-query, scope, or CSSOM behavior; the
newly passing curated WPT cases and focused cascade tests are the acceptance
evidence for this standards slice, not an inferred score gain.

The subsequent [HTML media, origin-hint, and CSP batch](docs/html-media-hints-csp.md)
rendered **425 / 588** with Breeze's default identity in a 2026-09-26
fresh-profile hidden release run at 1280×720, 125% scale, `en-US`, and a
10-second settle: **+2** versus the preceding 423 / 588 observation under
the same settings. Both runs returned HTTP 200 with zero JavaScript errors
and no renderer exit. The contained-worker decode/play/seek proofs, hidden
renderer media lifecycle tests, and CSP/request-policy regressions establish
the behavioral scope; this two-point probe change is not a claim of full
standards conformance or a controlled loading-speed comparison.

The next [Beacon, speech, manual slots, prefetch, media, and CSP batch](docs/html5test-beacon-speech-shadow-prefetch.md)
rendered **430 / 588** with Breeze's default identity in a 2026-09-26
fresh-profile hidden release run using the same viewport, scale, locale, and
10-second settle: **+5** versus the preceding 425 / 588 observation. The run
returned HTTP 200 with zero JavaScript errors and no renderer exit. Targeted
diagnostics changed the speech-synthesis, AAC-in-M4A, and CSP rows from missing
to present. The FLAC and prefetch rows still read missing, despite real native
FLAC decode and hidden prefetch/reuse acceptance tests; this batch makes no
score claim for those narrower behaviors. The score is not a conformance
percentage or a controlled performance comparison.

The 2026-09-27 [offline Web Audio slice](docs/web-audio.md) rendered
**430 / 588** in a fresh-profile hidden release run with the same viewport,
scale, locale, and settle time. It returned HTTP 200 with no JavaScript errors
or renderer exit. This was **no score change** from the preceding observation:
offline graphs render real audio samples, but at that stage the live `AudioContext`
remained unexposed until its page-to-device path could play them reliably. No points were
claimed for a constructor without working behavior.

The next [Worker, Canvas, Fetch, DOM, and Notification standards batch](docs/html5test-websocket-indexeddb-forms.md)
rendered **435 / 588** with Breeze's default identity in a 2026-09-27
fresh-profile hidden release run at 1280×720, 125% scale, `en-US`, and a
10-second settle: **+5** versus a same-day **430 / 588** run of the preceding
release build. Both returned HTTP 200 with zero JavaScript errors and no
renderer exits. The Worker IndexedDB/WebSocket, live Canvas, keepalive,
Notification, URLPattern, and DOM behavior is verified by focused unit and
hidden browser tests; HTML5test does not cover most of those contracts. The
score remains a capability inventory, not a conformance percentage or
controlled loading-performance result.

The 2026-09-28 [Geolocation](docs/geolocation.md), [pre-capture device
enumeration](docs/media-devices-enumeration.md), live [Web Audio](docs/web-audio.md),
and [drag-data lifecycle](docs/dom-traversal-editing.md) batch rendered
**458 / 588** with Breeze's default identity, up **23** from a same-day
**435 / 588** `origin/main` baseline. Both were fresh-profile hidden release
runs at 1280×720, 125% scale, `en-US`, and a 10-second settle, returning HTTP
200 with no JavaScript errors or renderer exit. The after capture used commit
`fd3617c`; its Geolocation, live `AudioContext`, and `enumerateDevices` rows
account for the score gain. Drag/drop gains no HTML5test points because those
rows remain excluded for an unrecognized browser identity, although trusted
pointer-drag lifecycle tests exercise the new behavior. These are partial APIs,
not claims of full Geolocation, Media Capture, Web Audio, or DnD conformance.

A separate browser-owned [Permissions API subset](docs/permissions-api.md) now
queries existing notification, geolocation, and sensor grants without prompting;
it does not imply full API coverage or an unmeasured HTML5test score increase.

The 2026-09-28 [physical sensor batch](docs/sensors.md) rendered
**465–468 / 588** across three identical hidden runs with Breeze's default
identity, up **7–10** from the merged PR #199 **458 / 588** baseline. All
used a fresh-profile Windows x64 release run at 1280×720, 125% scale,
`en-US`, and a 10-second settle, returning HTTP 200 with zero JavaScript
errors or renderer exits. The two 468-point runs executed seven page scripts;
the 465-point run executed six. The score observes API availability, not
sensor hardware or permission conformance. Fake-provider, protocol, and
hidden-renderer tests exercise the physical-data contract without accessing
the user's devices. The adapter uses existing locked Windows API crates,
without adding dependencies or copied upstream code.

The next [protocol handler](docs/custom-protocol-handlers.md),
[Clipboard](docs/async-clipboard.md), [BroadcastChannel](docs/broadcast-channel.md),
and [File API](docs/file-api-worker-reads.md) batch rendered **469 / 588** in
three identical 2026-09-28 hidden release runs with Breeze's default identity.
The merged PR #200 baseline was **465–468 / 588** under the same viewport,
scale, locale, and settle settings. Each new run executed seven page scripts,
returned HTTP 200, and had no JavaScript errors or renderer exits. The result
is one point above the highest prior observation, but the baseline's 465-point
run executed only six scripts, so the range is not a controlled attribution to
these APIs. Browser-owned consent, cross-tab delivery, FileReader decoding,
and byte-stream behavior are covered by focused and hidden integration tests;
HTML5test does not establish their conformance. This batch adds no dependency
or copied third-party implementation.

The subsequent 2026-09-28 worker CacheStorage, StorageManager, Screen Wake Lock,
and Permissions API batch also rendered **469 / 588** in three identical hidden
fresh-profile release runs with Breeze's default identity. Each run returned
HTTP 200, executed seven page scripts, and had no JavaScript errors or renderer
exits. The score is unchanged from the preceding 469-point observation; the
new browser-owned storage, power, and permission contracts are covered by
focused unit and hidden integration tests, not established by HTML5test.

The following 2026-09-28 [URLPattern grammar slice](docs/url-pattern.md) also
rendered **469 / 588** in three identical hidden fresh-profile release runs.
Each returned HTTP 200, executed seven page scripts, and had no JavaScript
errors or renderer exits. The score is unchanged; HTML5test does not exercise
the new repeated and literal-affixed capture behavior, which is covered by
focused tests. No dependency or copied upstream code was added.

The 2026-09-29 bounded media, resource-prefetch, and browser-owned capture
batch rendered **485 / 588** in three identical hidden fresh-profile release
runs with Breeze's default identity, compared with the preceding **469 / 588**
observation under the same viewport, scale, locale, and settle settings. All
three returned HTTP 200, executed seven page scripts, and had no JavaScript
errors or renderer exits. The 16-point change is a batch-level observation,
not evidence that any individual API is fully conformant: HTML5test does not
exercise a real camera or microphone permission decision. Focused and hidden
integration tests cover the admitted media and capture behavior and its limits.

The subsequent [audio decoding, recording, and Web Audio batch](docs/audio-codecs-and-recording.md)
rendered **487 / 588** in three identical hidden fresh-profile release runs,
compared with **485 / 588** in three same-day runs of the preserved preceding
release executable. All six used Breeze's default identity, 1280×720 at 125%
scale, `en-US`, and a 10-second settle. Each returned HTTP 200, executed seven
page scripts, and had no JavaScript errors or renderer exits. The **+2** is a
batch-level observation, not a claim that HTML5test exercises the complete
codec, recording, or Web Audio behavior; focused tests cover the bounded
implementations and documented limits.

The subsequent URLPattern, CSS transitions, popover, native editing,
history-restoration, permissions, and Web Audio spatial/oversampling batch
also rendered **487 / 588** in three hidden fresh-profile release runs on
2026-09-29. This is **unchanged** from the preceding documented result and a
same-day capture of its preserved executable. All four returned HTTP 200 with
no JavaScript errors or renderer exits. These standards behaviors are covered
by focused and hidden integration tests; HTML5test does not currently award
additional points for them, and the unchanged score is not a conformance claim.

The following 2026-09-29 [file-upload](docs/file-upload.md) and
[text-control selection](docs/text-control-selection.md) batch rendered
**487 / 588** in three identical hidden fresh-profile release captures,
unchanged from the recorded preceding baseline above. All three used Breeze's
default identity, 1280×720 at 125% scale, `en-US`, and a 10-second settle;
each returned HTTP 200, executed seven scripts, and had no JavaScript errors
or renderer launch errors. The score does not establish real file selection,
multipart byte delivery, or native selection synchronization. Focused tests
and hidden integrations cover these bounded implementations. The batch adds
no dependency or copied third-party code.

The 2026-09-29 [form numeric-reflection](docs/form-numeric-reflection.md)
batch rendered **487 / 588** in three identical hidden fresh-profile release
captures, unchanged from the recorded preceding result. All three used the
default Breeze identity, 1280×720 at 125% scale, `en-US`, and a 10-second settle;
each returned HTTP 200, executed seven scripts, and had no JavaScript errors
or renderer exits. Focused tests establish the corrected meter/progress bounds,
numeric conversions, and text-length reflection; the score is not a conformance
claim and no additional points are attributed to these changes.

The 2026-09-30 [encoded-audio container](docs/encoded-audio-containers.md) and
[ordinary wheel measurement](docs/wheel-input-measurement.md) batch also rendered
**487 / 588** in three fresh-profile release captures, unchanged from #210.
These used the same default identity, scale and locale, with a 4.5-second settle;
all returned HTTP 200 with seven executed scripts and no JavaScript errors or
renderer exits. Real PCM decoding and bounded media queries are covered by
owned tests; no new score points or broad conformance are claimed.

Same-machine hidden diagnostics on 2026-09-30 compared Breeze with Chrome
154.0.8037.92. The before captures showed Breeze's score between 1.50–2.50 seconds
and Chrome's between 1.50–2.00 seconds; these are 250 ms filmstrip visibility
bounds, not precise completion timestamps. The after filmstrip first showed
Breeze's score at 2.001 seconds, absent at 1.750 seconds. Chrome rendered 579 / 588.
This small diagnostic sample does not establish the five-cold/five-warm
performance acceptance in [#205](https://github.com/VictorZakharov/better-web-browser/issues/205).

Ordinary wheel diagnostics exposed latency that the older direct-scroll paint
probe excludes. Each phase sent eight alternating-direction viewport wheels,
1000 ms apart. In the early and 12-second-settled runs, Breeze's enqueue-to-first
retained motion paint medians were **72.1 / 75.1 ms**, maxima **282.6 / 688.3 ms**;
renderer-local dispatch medians were **62.7 / 68.3 ms**. Chrome's separate
compositor-frame-receipt endpoint had medians **18.8 / 22.9 ms**, maxima
**41.9 / 28.7 ms**, with one early input left unattributed because its listener
verdict was missing. These endpoints differ, so no browser speed ratio is valid.
Breeze's content viewport was 1248.8×548 CSS px; Chrome's requested 1249×548
rounded to 1250×548 at 125% scale. Initial delays start at different readiness
events. The observed first-wheel spikes remain an investigation, not a claim
that interactive scrolling is fixed; see the linked measurement contract.

The subsequent 2026-09-30 [Ogg/Opus](docs/ogg-opus.md) batch rendered
**487 / 588** in three hidden fresh-profile release captures, unchanged from
the preceding batch. All three used the default Breeze identity, 1280×720 at
125% scale, `en-US`, and a 4.5-second settle; each returned HTTP 200, executed
seven scripts, and reported no JavaScript errors or renderer exits. The owned
tests verify real mono/stereo PCM, resampling, exact seeks, and complete
microphone recordings whose concatenated chunks decode with exact EOS trimming.
Opus support is bounded to Ogg mapping family 0; it does not imply WebM/Opus,
MediaSource, or 44.1 kHz recording. No score gain is inferred from those tests.

The same batch replaces repeated plain-element DOM lookup with a
[query-owned native hit-target index](docs/native-hit-testing.md). Two serial,
reversed-order before/after pairs per live HTML5test phase measured ordinary
wheel enqueue-to-motion-paint medians of **49.3 → 5.7 ms early** and
**49.2 → 5.5 ms settled**, including all first inputs. All 64 inputs were
viewport/painted with no omitted inputs, unmatched acknowledgements, script
errors or renderer exits. Early first-wheel stalls still approach 150 ms;
this does not declare all startup contention or #205 resolved. Owned 1k/2k/4k
fixtures verify the repeated-scan improvement and explicitly retain the
roughly 0.4–0.5 ms dispatch overhead on already-cheap late/inert targets.
The linked report includes endpoints, first-input outliers and limitations;
it is not a Chrome/Breeze speed ratio.
The verified Chrome reference separately measured 12.6/12.2 ms compositor-frame
receipt medians for eight early/eight settled wheels, including first inputs.
Its capture/CDP endpoint and load-readiness anchor differ from Breeze's; the
linked report retains those limits and the observer regression tests.

The 2026-10-01 [WebM/Opus and MediaCapabilities](docs/webm-opus.md) batch
rendered **487 / 588** in all three hidden fresh-profile release captures,
unchanged from the same-day pre-change main build (**487 / 588**). The settings
were identical: default Breeze identity, 1280×720 window, 125% scale, `en-US`,
and 4.5-second settle. Each returned HTTP 200, executed seven scripts, and had
no JavaScript errors or renderer exits. No score gain is claimed.

The new functionality is real complete-file WebM/Opus playback and Web Audio
decoding, incremental microphone WebM recording, optional EBML CRC integrity,
and [encoding/decoding capability queries](docs/media-capabilities.md) shared
by Window and dedicated Workers. Independent FFmpeg Ogg/WebM tones decode to
identical audible PCM; owned tests cover lacing, delay/padding, gain, exact
seeks, native-rate recording tails, asynchronous job cancellation and malformed
input. Queries match actual capture limits, not the wider decoder rate matrix,
and never claim unmeasured smoothness or hardware efficiency. WebM video,
SourceBuffer, WebRTC and DRM remain unsupported. The new serializer reuses
pinned MIT-licensed WebM/EBML crates; existing demuxer, codec and CRC dependencies
are reused, with reviewed provenance recorded in the linked contract.

The subsequent 2026-10-01 [channel routing, feedback, and legacy processing](docs/web-audio-routing.md)
batch also rendered **487 / 588** in three hidden fresh-profile release captures,
versus **487 / 588** in three same-day main captures. The default Breeze identity,
1280×720 window, 125% scale, `en-US`, and 4.5-second settle were unchanged.
All six captures returned HTTP 200 without JavaScript errors or renderer exits.
This is compatibility work guided by the missing standards, not a score gain:
the original 80-contract PCM/metadata fixture improved from **34/80** to **80/80**.
Chrome 154.0.8037.92 passed **72/80** under the same oracle; its eight differing
destination, feedback, and legacy processing behaviors are explicitly documented,
not hidden by browser-specific assertions. This comparison does not establish
overall browser superiority or full Web Audio conformance.

The subsequent 2026-10-01 [modern images and ImageBitmap](docs/modern-images-and-bitmaps.md)
batch rendered **487 / 588** in three hidden fresh-profile release captures,
unchanged from three same-day main captures using the same settings. All six
returned HTTP 200 without JavaScript errors or renderer exits. No score gain
is claimed. The original 15-image Canvas readback fixture improved from
**0/15 decoded** to **15/15 decoded**: eight AVIF and seven JPEG XL variants.
Unified-headless Chrome 154.0.8037.92 decoded the eight AVIF fixtures and rejected
the seven JPEG XL fixtures. This is format coverage, not a superiority score:
some subsampled YUV pixels differ because of chroma interpolation, and the
new paths deliberately retain documented SDR/first-frame/metadata limits.
Independent FFmpeg pixels, an exact Chromium 12-bit reference, and contained
renderer pixel tests back the implementation. Pinned open-source decoders
are reused with reviewed licenses; no benchmark or test-site behavior is faked.

The next 2026-10-01 [ImageDecoder and VideoFrame](docs/image-decoder-and-video-frame.md)
batch also rendered **487 / 588** in three identical hidden fresh-profile release
captures, unchanged from the same-day main build. The settings remain 1280×720,
125% scale, `en-US`, and default Breeze identity; all six captures returned HTTP
200 without JavaScript errors or renderer exits. No HTML5test score gain is claimed.

| Original behavior-level fixture | Before | After | Unified-headless Chrome 154.0.8037.92 |
| --- | ---: | ---: | ---: |
| Frame pixels, layouts, ownership, streams, animation and Canvas contracts | 0/40 | 40/40 | 38/40 |

The two Chrome differences are kept visible: this fixture's separate APNG poster
track is not exposed by Chrome, and its invalid-image-MIME query resolves false
instead of the current WebCodecs draft's TypeError rejection. These are explicit
contract differences, not a browser-superiority or full-conformance claim. Native
codecs, dedicated-worker tests and contained-renderer BGRA readbacks back the new
API paths. No new production dependency is added; bounded complete-input CPU
decoding, SDR conversion and explicit codec/streaming limits remain documented.
Returning animation frames does not yet add automatic animated `<img>` playback.

The 2026-10-02 [WebCodecs audio](docs/webcodecs-audio.md) batch rendered
**487 / 588** in three identical hidden fresh-profile release captures, unchanged
from the preceding main release's same-day **487 / 588** capture. All returned
HTTP 200 without JavaScript errors or renderer exits, at 1280×720, 125% scale,
`en-US`, and default Breeze identity. No HTML5test score increase is claimed:
its older inventory does not exercise these WebCodecs audio contracts.

| Original behavior-level fixture | Before | After | Unified-headless Chrome 154.0.8037.97 |
| --- | ---: | ---: | ---: |
| Audio samples, chunk ownership, MessagePort transfers and real codec output | 0/17 | 17/17 | 14/16 completed; final PCM probe timed out |
| Curated upstream WPT assertions | 5,960 | 5,988 | Not measured for this batch |

The reference comparison retains two differences: Chrome returned
`NotSupportedError` for an invalid zero-rate AudioData initializer instead of
the current draft's `TypeError`, and rejected explicit registered Opus `ogg`
format. Its signed 24-bit PCM probe was isolated after a timeout, not counted
as a pass. The full Breeze fixture remains seventeen contracts, including PCM.

This slice adds eight AudioData sample layouts, bit-preserving copying,
clone/transfer across Window and Worker realms, actual asynchronous Opus
encoding/decoding, and five registered raw PCM decoders. The existing locked
libopus dependency and header parser are reused; no new codec dependency or
copied codec implementation is introduced. Tests cover real decoded energy,
all admitted packet durations/rates, exact 24-bit sample expansion, reset/flush,
callback ordering and hidden AppContainer execution. Resource, timing, priming,
padding and unsupported-codec limits are explicit in the implementation notes.

The following 2026-10-02 [compressed audio](docs/webcodecs-audio.md) and
[AV1 VideoDecoder](docs/webcodecs-video.md) batch rendered **487 / 588** in three
hidden fresh-profile release captures, unchanged from three captures of merged
#218. All six returned HTTP 200 with no JavaScript errors or renderer exits, at
1280×720, 125% scale, `en-US`, and default Breeze identity. The six rendered score
captures are byte-identical. No HTML5test score gain is claimed: its older
inventory does not exercise these elementary WebCodecs paths.

| Original behavior-level fixture | Before (#218) | After | Unified-headless Chrome 154.0.8037.97 |
| --- | ---: | ---: | ---: |
| MP3/AAC-LC/FLAC/Vorbis packets, snapshots, FLAC encoding and error/reset contracts | 1/21 | 21/21 | 12/21 |
| AV1 inter-frame pixels, chunk/frame ownership, Canvas, aspect ratio and reset | 0/7 | 7/7 | 7/7 |
| Curated upstream WPT assertions | 5,988 | 5,991 | Not measured for this batch |

The audio reference's nine differences remain visible: six FLAC encoder cases
were unavailable, two Vorbis cases returned fewer samples than this fixture
expects, and its FLAC corruption case did not deliver the expected error.
These are functionality contracts, not a browser-superiority or speed score.
The AV1 comparison checks eight actual changing frames against an independent
FFmpeg pixel reference, not just the presence of a decoder constructor.

The batch reuses existing locked Symphonia, Claxon, flacenc and rav1d libraries;
no dependency or third-party source is added. MP3/AAC-LC/FLAC/Vorbis decoding
uses real stream dimensions; FLAC encoding emits unpadded raw frame packets.
AV1 retains inter-frame references in bounded software workers and produces
owned VideoFrames in Window/Worker realms, with native transfer and Canvas
painting. Higher AV1 profiles/depths, hardware decoding, color overrides and
VideoEncoder remain unavailable. These additions do not establish complete
media-site playback or universal codec conformance.

The following 2026-10-02 [stylesheet CSS Animations/CSSOM](docs/css-animations.md)
batch rendered **487 / 588** in three hidden fresh-profile release captures,
unchanged from three alternating captures of merged #219. All six returned HTTP
200 with no JavaScript errors or renderer exits, at 1280×720, 125% scale,
`en-US`, default Breeze identity and a 10-second observation window. No points
gain is claimed; this batch adds native behavior rather than new inventory flags.

| Measurement | Before (#219) | After | Unified-headless Chrome 154.0.8037.97 |
| --- | ---: | ---: | ---: |
| Original animation/CSSOM fixture contracts | 1/13 reached; 12 failed | 23/23 | 23/23 |
| Curated upstream WPT assertions / cases | 5,991 / 550 | 6,086 / 554 | Not measured for this batch |
| HTML5test page-ready, median of three | 246.0 ms | 236.1 ms | Not measured for this batch |
| HTML5test accumulated JavaScript time, median | 402.4 ms | 566.4 ms | Not measured for this batch |
| HTML5test combined working set, median | 177.2 MiB | 190.8 MiB | Not measured for this batch |
| Animation fixture ready, release median of three | Not measured | 219.8 ms | 446.1 ms |
| Animation fixture combined working set, median | Not measured | 44.1 MiB | 547.7 MiB |

The old fixture stops at missing keyframe CSSOM support after thirteen checks;
unreached checks are not counted as failures or passes. All twenty-three checks
complete in both new browsers; the perceptual screenshot difference is 0.018.
The new native validation/playback paths have a visible cost on HTML5test:
recorded JavaScript work rises about 164 ms and combined working set about
13.6 MiB. Similar first-paint timing is not evidence of faster score completion.
These small local samples are not a universal speed or memory comparison:
Breeze ready follows its first owned paint, whereas Chrome ready follows load;
combined working sets include two Breeze processes versus ten Chrome processes.
Captures ran without competing compilation, in alternating before/after order.

The batch reuses existing `cssparser` and Web Animations code with no new
dependency or copied implementation. It adds scoped/layered keyframes, real
timeline sampling/events, native authored-value CSSOM editing, transition
cascade/reversal fixes, and regression tests. Initial animation settings are
shared until edited. Pseudo-elements, additive composition, compositor
offloading and complete CSSOM serialization remain explicitly open.

The subsequent 2026-10-02 [WebGL 1 baseline](docs/webgl-backend.md) adds real
Windows shader rendering through pinned ANGLE/GLES2 and D3D11 WARP, including
indexed textured geometry, uniforms, framebuffer/renderbuffer objects, Canvas
painting and OffscreenCanvas worker export. Twenty-eight focused regression tests
exercise native pixels and security/lifecycle contracts; the shared rendering
fixture checks eighteen contracts against unified-headless Chrome. This is not
full WebGL conformance: antialiasing, extensions, restoration, WebGL 2 and WebGPU
remain unavailable. Non-Windows builds do not advertise a native WebGL backend.
Windows builds additionally require LLVM's `libclang.dll` version 19 or later;
`prepare-angle.ps1` validates its location without downloading a compiler.
Native and embedded license notices are included in release archives.

All three fresh-profile release captures show **502 / 588**, versus **487 / 588**
in three alternating captures of merged #220 (**+15**). These use the default
Breeze identity, 125% scale, `en-US`, a 1280×720 hidden window and a ten-second
observation period. All six return HTTP 200 without JavaScript errors or renderer
exits. Compilation and other test suites were idle during measurement.

| Measurement | Before (#220) | WebGL baseline | Unified-headless Chrome 154.0.8037.97 |
| --- | ---: | ---: | ---: |
| HTML5test rendered score, three runs | 487 / 588 | 502 / 588 | Not measured for this batch |
| Real textured WebGL fixture contracts | Backend unavailable | 18/18 | 18/18 |
| Curated upstream WPT assertions / cases | 6,086 / 554 | 6,086 / 554 | Not measured for this batch |
| HTML5test page-ready median | 248.8 ms | 265.4 ms | Not measured for this batch |
| HTML5test accumulated JavaScript median | 589.2 ms | 598.7 ms | Not measured for this batch |
| HTML5test combined working-set median | 193.2 MiB | 221.5 MiB | Not measured for this batch |
| WebGL fixture ready, median of three | Not measured | 245.6 ms | 844.0 ms |
| WebGL fixture combined working-set median | Not measured | 99.2 MiB | 550.9 MiB |

The new native backend costs about **28.3 MiB** on HTML5test in this sample;
page-ready rises about 16.6 ms. These are not score-completion or universal browser
performance claims. Ready follows Breeze's first owned paint versus Chrome's load;
combined working sets include two Breeze processes versus Chrome's multiprocess
tree. The rendering fixture uses approximately matching content viewports
(Breeze 1248.8×548 CSS pixels; Chrome 1249×548) and 125% scale. Exact shader/readback
pixels and copy orientation match; existing Canvas image-scaling and page-layout
differences mean the full screenshots are not pixel-perfect. Software WARP is
deliberately chosen for this first hidden, contained rendering baseline.

Reproduce the latest snapshot on Windows x64 with the release build above (1280×720 hidden window,
125% scale, `en-US`, new profile); retain both the JSON diagnostics and rendered score:

```powershell
./scripts/run-hidden-benchmark.ps1 -Url https://html5test.co/ `
  -Browser target/release/better-web-browser.exe -FreshProfile `
  -WindowWidth 1280 -WindowHeight 720 -DeviceScaleFactor 1.25 -Locale en-US `
  -SettleMs 10000 -TimeoutSeconds 60 -DiagnosticSelector '#score' `
  -Output target/html5test/2026-10-02-webgl-reproduction.json `
  -Screenshot target/html5test/2026-10-02-webgl-reproduction.png
```

The 2026-10-03 [WebGL lifecycle and extension batch](docs/webgl-lifecycle-and-extensions.md)
adds native context restoration, instancing, vertex-array objects, uint indices,
derivatives, fragment depth and explicit texture LOD. It reuses the existing
pinned ANGLE WebGL shader validator rather than implementing a shader compiler.
Only implemented, native-supported extensions are advertised. Trusted lifecycle
events, stale resource isolation, WebIDL conversion, native uniform reflection
and bounded diagnostic/IPC separation have dedicated regression tests.

Three final alternating fresh-profile release samples show **502 / 588 before**,
and **502, 502, 497 / 588 after**, with unified-headless Chrome **579 / 588**.
No HTML5test points are
claimed for this batch. The five required, unmodified upstream Khronos cases
pass **379 checks** (374 upstream assertions plus five required-capability checks);
the separate exploratory manifest records known gaps
instead of hiding failures or replacing upstream shader helpers.

| Measurement (three-run median unless stated) | Before (#221) | Lifecycle/extensions | Unified-headless Chrome 154.0.8037.97 |
| --- | ---: | ---: | ---: |
| HTML5test rendered score, three samples | 502 / 588 each | 502, 502, 497 / 588 | 579 / 588 each |
| HTML5test window ready | 18.3 ms | 19.4 ms | 231.7 ms |
| HTML5test page ready | 274.5 ms | 287.6 ms | 1,076.0 ms |
| HTML5test accumulated JavaScript | 579.8 ms | 602.4 ms | 598.4 ms |
| HTML5test combined working set | 220.4 MiB | 220.0 MiB | 639.3 MiB |
| Shared lifecycle/instancing fixture contracts, all three samples | Extensions unavailable | 47/47 | 47/47 |
| Shared fixture page ready | Not measured | 227.3 ms | 575.0 ms |
| Shared fixture combined working set | Not measured | 95.3 MiB | 549.2 MiB |

These are local observations, not a speed-up claim: working sets are similar,
and page-ready samples overlap (before 245.2–335.2 ms; after 255.5–318.1 ms).
The 497-point sample accumulated 2,302.0 ms of JavaScript, versus 572.6 and
602.4 ms in the other after samples. Additional baseline/new diagnostic captures
returned 502, but the original slow run lacked feature-row diagnostics: the
specific five-point failure is not isolated, and score stability is not claimed.
The outlier is retained, not discarded. Captures use the default Breeze
identity, fresh profiles, `en-US`, 125% scale and a ten-second observation period,
with no competing builds or tests. The content viewports approximately match
(Breeze 1248.8×548 CSS pixels; Chrome 1249×548). Breeze page-ready measures its
first owned paint; Chrome's ready endpoint follows load. Neither measures score
completion, and accumulated JavaScript includes work after first paint. Separate
browser identity, layout and process-tree differences prevent treating these
figures as equivalent full-browser performance. WebGL 2, WebGPU, antialiasing and
full Khronos conformance remain unimplemented; synchronous XHR and long-running
shader test tasks in debug builds remain explicit conformance gaps. The upstream
texture-LOD case passes in the final release sample, but remains outside the
required gate until it also completes within the unchanged debug watchdog.

New releases must refresh or explicitly date these observations using the
[evidence checklist](docs/technical-alpha-release.md#reproduction-and-release-authority).

### 2026-10-03 texture and staged GLES3 batch

The next 3D batch extends real native rendering rather than changing feature
probes. It reuses the locked ANGLE backend; it adds no graphics dependency or
copied renderer implementation. New public WebGL1 contracts include independent
S3TC, compressed sRGB and RGTC extension families, exact block-byte validation,
native signed/unsigned decoding and shared Window/Worker bindings. HDR, depth,
sRGB and multiple-render-target fixtures compare actual pixels with Chromium.

The [internal GLES3 foundation](docs/webgl2-foundations.md) covers shaders,
unsigned and non-square uniforms, integer/instanced attributes, primitive restart,
immutable sized textures, volume/array storage, layered attachments, multisample
resolve, typed framebuffer clears, samplers, uniform blocks, pixel-buffer
transfers, native queries/fences and transform feedback. Array framebuffer
copies preserve untouched regions and independent read/draw identities.
Texture storage now uses transactional per-image high-water accounting: repeated
definitions, shrinking/regrowing and mip regeneration do not repeatedly consume
the context budget. Malformed operations preserve existing image contents.

| Contract | Before this batch | Implemented in this batch |
| --- | --- | --- |
| Compressed native texture families | Not exposed | BC1/2/3, sRGB BC1/2/3, signed/unsigned BC4/5 |
| Compressed format admission | No public format set | Capability-gated, per-context enablement and typed queries |
| Texture lifetime accounting | Full-image charge repeated on redefinition | Transactional growth-only per-image high-water mark |
| Sized/immutable core storage | No staged core contract | Checked 2D/cube/volume/array mip definitions |
| GPU buffer/texture transfers | Owned CPU uploads/readback | Separate checked PBO-offset operations |
| Core shader data transport | WebGL1 uniforms | Typed uniforms, std140 blocks and native transform feedback |
| Array framebuffer copies | No core copy path | Native slice/subregion copies and neighbor preservation |
| Public `getContext('webgl2')` | `null` | Still `null` until coherent admission |

This is not a WebGL2 conformance claim. Remaining admission work includes
versioned JavaScript overloads and trusted event-loop completion for query/sync
visibility. The pinned provider has specific limitations in transform-feedback
array capture, true 3D framebuffer copies and framebuffer invalidation safety.
Unsafe operations remain unexposed or fail closed; tests do not substitute CPU
answers for unsupported GPU behavior. The [gd-clone roadmap](docs/gd-clone-compatibility.md)
tracks the unchanged sibling game's eventual end-to-end acceptance separately.

The shared repetition fixture exercises thousands of texture operations and
checks sampled pixels, error atomicity and renderer responsiveness. It is not a
performance benchmark. Required Khronos cases remain unmodified, with no failure
expectation overrides or relaxed JavaScript watchdog. HTML5test remains a guide
to missing functionality; this internal foundation does not earn WebGL2 points.

Final alternating fresh-profile release samples on October 3 show **502 / 588
before and after, in all three runs**, versus unified-headless Chrome 154 at
**579 / 588**. Scores were read from the settled screenshots, not inferred from
advertised APIs. Both browsers used 125% device scale, `en-US`, approximately
1249 x 548 CSS pixels and a 10-second settling interval. Existing HTML5test
layout differences remain; this graphics batch does not claim visual parity.

| Measurement | Before (PR #222) | After | Chrome 154 |
| --- | ---: | ---: | ---: |
| HTML5test rendered score, three samples | 502 each | 502 each | 579 each |
| Window-ready median | 15.940 ms | 16.249 ms | 220.401 ms |
| Harness page-ready median | 236.932 ms | 222.353 ms | 975.911 ms |
| Settled JavaScript time median | 588.687 ms | 579.432 ms | 501.250 ms |
| Process-tree working-set median | 222.254 MiB | 220.871 MiB | 636.496 MiB |
| Process-tree private-bytes median | 169.898 MiB | 167.578 MiB | 359.168 MiB |
| Required Khronos cases / passing assertions | 5 / 379 | 20 / 3122 | Not run against pinned suite |
| Shared HDR/depth, MRT, compression, lifetime assertions | Not this batch's baseline | 71 / 28 / 103 / 38 | 71 / 28 / 103 / 38 |

Harness page-ready definitions differ and do not measure score completion;
these values are not evidence that Breeze finishes HTML5test faster than Chrome.
The first baseline page-ready sample was 872.093 ms (the others 220.987 and
236.932 ms); it is retained in the median, not discarded. Process-tree memory
includes 2 Breeze processes versus 10 Chrome processes. These are small live
samples, not a general startup, memory or performance guarantee. All four shared
fixtures completed without JavaScript or browser errors in both browsers.

### 2026-10-04 public WebGL2 and framework acceptance

The [WebGL2 admission batch](docs/webgl2-foundations.md) connects the native
GLES3 foundation to distinct Window/Worker Canvas and OffscreenCanvas interfaces.
It includes bounded typed/PBO overloads, task-stable native query/fence results,
transform-feedback ownership, real multisample resolves and hardware-first ANGLE
selection with software fallback. Existing ANGLE, image decoding and color
management dependencies are reused, not replaced with a home-grown renderer.
Genuine 16-bit decoded images retain precision through high-precision WebGL
uploads and ImageBitmap crop/resize/clone/transfer; ordinary painting stays RGBA8.

Fresh-profile release captures on October 4 show **502 / 588 before and
507 / 588 after in all three runs**, versus **579 / 588** in Chrome 154.
The original Three.js 0.185.1 module fails to create its required WebGL2 context
on the baseline, while the same twelve acceptance scenes pass on this branch
and Chrome with matching sampled pixel values. No library patches, downgraded
renderer, fake capabilities or score-specific responses are used.

| Measurement | Before (main `b06fe0e`, PR #223) | After | Unified-headless Chrome 154 |
| --- | ---: | ---: | ---: |
| HTML5test rendered score, three samples | 502 each | 507 each | 579 each |
| Window-ready median | 16.972 ms | 19.428 ms | 239.525 ms |
| Harness page-ready median | 246.356 ms | 291.766 ms | 1056.164 ms |
| Settled JavaScript time median | 620.353 ms | 773.411 ms | 580.832 ms |
| Process-tree working-set median | 221.277 MiB | 264.730 MiB | 835.320 MiB |
| Process-tree private-bytes median | 169.398 MiB | 238.227 MiB | 359.980 MiB |
| Unmodified Three.js pixel scenes | WebGL2 context unavailable | 12/12 | 12/12 |
| Hardware/fallback creation contracts | Not implemented | 8/8 | 8/8 in the reference investigation |
| Required pinned Khronos cases / passing assertions | 20 / 3122 in the preceding batch | 74 / 8696 | Not run against pinned suite |

The extra real graphics setup adds JavaScript time and memory on this test;
this is a capability improvement, **not a performance win**. Both browsers use
125% device scale, `en-US`, approximately 1249 x 548 CSS pixels and a 10-second
settling interval. Scores come from the rendered screenshots. Harness readiness
definitions differ and do not measure score completion. These are small live
samples, not general startup/memory guarantees or sustained rendering benchmarks.
Process-tree memory includes two Breeze processes versus ten Chrome processes.
HTML5test layout differences still exist. Full WebGL2 conformance, an upstream
ANGLE immutable-NPOT storage fix, and unchanged gd-clone gameplay acceptance
remain open; the documented framework fixture is not proof that the game runs.

### October 5: Canvas and WebGL game-readiness batch

This batch adds native ANGLE parallel-shader completion, multi-draw with draw-ID
semantics, and independent indexed blending, including shared Window/Worker
pixel and invalid-input contracts. Canvas reuses existing tiny-skia and kurbo
for bounded coverage, adaptive curves, affine/dashed strokes, shadows and
solid/gradient/repeating-pattern shading. Worker byte transport and SVG input
reuse avoid repeated allocations; paint brands, matrix dictionaries and line
styles now follow their shared conversion contracts. These are general browser
changes, not patches to the sibling game or HTML5test-only detection behavior.

| Measurement | Before | After |
| --- | ---: | ---: |
| HTML5test rendered score, one fresh release sample each | 507 / 588 | 507 / 588 |
| Repeated eight-shader validation median | 14.9 ms | 9.3 ms |
| Repeated curved fill/stroke fixture | 135.9 ms | 45.5 ms |
| Forty changing-pen strokes without coverage reuse | 153.4 ms | 116.1 ms |
| Twelve 256-square gradient fills | 102.4 ms | 28.2 ms |
| Eight 256-square repeating-pattern fills | 102.7 ms | 14.5 ms |

The fresh hidden Chrome 154 score sample remains 579 / 588. The score comparison
uses main `054ad9f` and this batch with fresh profiles, 125% device scale and a
ten-second settling period; it does not measure time to score completion.
Other rows use their individual preceding implementations on the same machine
and are targeted samples, not general browser/game throughput. Chrome matched
the indexed-blend and pattern samples; fractional Canvas edge/gradient pixels
are not universally identical. Chrome can defer Canvas work beyond a timed
method call, so call timings alone are not a rendering-throughput comparison.

The unchanged gd-clone production build still times out in document texture
generation and has loading-screen layout/SVG differences. It is **not yet
playable**. SVG text was deferred rather than shipping incorrect nested
`textLength` behavior. See [game evidence](docs/gd-clone-compatibility.md),
[coverage measurements](docs/canvas-coverage-reuse.md),
[gradient contracts](docs/canvas-gradients.md),
[pattern/matrix contracts](docs/canvas-pattern-matrices.md), and
[indexed blending](docs/webgl-indexed-blending.md). No HTML5test point gain is
claimed for this batch.

### October 6: owned Canvas, font sources and real text shaping

Canvas drawing state, context/image/transfer brands and Web IDL conversion now
share private Window/Worker contracts. Native coverage, image painting, glyph
painting and all 26 compositing operators retain owned pixels and bounded
fallbacks. Connected image drawing uses the decoded resource and its response
authority, not author-controlled image helpers. SVG path parsing reuses the
already locked MIT/Apache-2.0 `svgtypes` dependency.

Document and worker fonts now use separate native registries. Live CSS font
discovery respects stylesheet ownership, active groups and descriptor coverage;
supported remote sources are tried in order through normal Fetch admission.
`font-feature-settings`, kerning, ligature and numeric variants reach real
OpenType glyph selection and advances, rather than only exposing property names.
Font CSSOM assignments reset supported longhands coherently, preserve authored
units, and distinguish specified feature lists from computed maps. Tests derive
real GSUB and kerning tables in memory from the existing CC0 Ahem fixture.

| Measurement | Before (`ebd42cc`) | After | Chrome 154 |
| --- | ---: | ---: | ---: |
| HTML5test rendered score | 507 / 588 | 507 / 588 | 579 / 588 |
| Unmodified CSS Fonts assertions | 22 / 80 | 80 / 80 | Not replayed |
| Unmodified CSS Font Loading assertions | 4 / 8 | 8 / 8 | Not replayed |
| Canvas repeated-label text | 46.6 ms | 17.8 ms | 43.0 ms |
| Canvas transformed images, nearest | 64.3 ms | 23.9 ms | 54.1 ms |
| Canvas transformed images, bilinear | 91.8 ms | 26.6 ms | 121.6 ms |
| Canvas changing thin strokes | 16.3 ms | 16.1 ms | 120.1 ms |
| Canvas compositing/shadow layers | 955.7 ms | 74.5 ms | 12.4 ms |

Canvas timings are medians of three rotating-order fresh hidden release runs
on this machine, with pixel readback inside the timed interval. These fixed-size
microbenchmarks are not a general browser-speed comparison: Chrome remains
substantially faster on the layer workload, and sampled fractional pixels and
text hashes are not universally identical. The score row uses one fresh sample
per browser at 125% device scale and ten seconds of settling, not time-to-score
measurement or matched-viewport visual acceptance. No HTML5test point gain is
claimed.

These remain bounded standards slices. Local font aliases, variation axes, full
CSS Font Loading layout readiness, container-dependent feature math and complete
paragraph-wide bidi remain open. Opaque Canvas images are currently rejected
before drawing instead of implementing full tainted-canvas propagation. No
claim of complete Canvas/CSS Fonts conformance or playable gd-clone follows from
the added API and pixel tests. See the [contracts and remaining gaps](docs/canvas-owned-compositing.md).

### October 6: font readiness, SVG text and bounded Canvas work

CSS Font Loading now tracks the renderer's actual pending environment: parsing,
stylesheets, used font requests/decoding and unpublished layout. Canvas-only
font use participates in loading, while unused faces do not manufacture
requests. Script font fetching uses the normal CORS/Fetch path with the `font`
destination and `font-src` policy. Bounded SFNT/table admission rejects unusable
containers; it is not a complete OpenType sanitizer.

SVG text uses the existing usvg shaping/outline backend and Breeze's font
catalog, including loaded CSS family aliases. A pinned, licensed usvg patch
corrects element-owned nested `textLength`; imports are excluded from the
original-work batch size. Renderer image acknowledgements publish changed SVG
pixels under stable keys and retire removed or rejected rasters. No synthetic
SVG geometry API is exposed just to claim support.

Canvas clipping, compact source/shadow storage and immutable coverage reuse
reduce redundant work while retaining independent dense/scalar pixel oracles.
V8 foreground tasks are pumped at bounded checkpoints, and hardware concurrency
reflects the shared worker-admission limit. The two-second script/renderer
watchdogs and containment budgets are unchanged.

| Measurement | Before (`e349b7e`) | After | Chrome 154 |
| --- | ---: | ---: | ---: |
| HTML5test rendered score | 507 / 588 | 507 / 588 | 579 / 588 |
| Unmodified CSS Font Loading assertions | 12 / 12 | 12 / 12 | Not replayed |
| Canvas compound clips | 122.9 ms | 27.1 ms | 122.6 ms |
| Canvas clipped strokes | 52.5 ms | 36.5 ms | 130.5 ms |
| Canvas clipped fills | 16.5 ms | 6.4 ms | 0.6 ms |
| Canvas changing shadow paint | 333.7 ms | 142.8 ms | 109.5 ms |
| Canvas changed shadow kernel | 29.1 ms | 16.7 ms | 4.1 ms |

Canvas rows are three-run medians from rotating-order fresh hidden release runs,
with readback inside the timed interval. Fixed-size Canvas fixtures do not
establish general browser speed or universal pixel parity. Breeze's recorded
before/after samples were unchanged; Chrome remains faster on several workloads.
The separate tall-stroke fixture had one baseline watchdog timeout in four runs
and none after; completed-run timings and that failure are both recorded in
[coverage measurements](docs/canvas-coverage-reuse.md). The score uses one fresh
sample per browser at 125% scale with ten seconds of settling, not time-to-score
or matched-viewport visual acceptance. No HTML5test point gain is claimed.

The matched-viewport game replay still shows loading in Breeze, while Chrome
reaches the lobby. Breeze's single-sample first presentation regressed from
893.5 ms to 2,780.7 ms, despite lower cumulative JavaScript time; this is not
accepted game startup. See [the full game result](docs/gd-clone-compatibility.md).

These are incomplete standards slices, not a claim of universal Chromium pixel
parity or playable gd-clone. SVG bidi/text-path/baseline limitations, composite
Unicode-range shaping and custom SVG font-feature selection remain open. See
[font readiness](docs/font-loading-readiness.md),
[SVG contracts and provenance](docs/svg-text-rendering.md), and
[Canvas coverage contracts](docs/canvas-coverage-reuse.md).

### October 7: responsive comparison lengths and Canvas region ownership

Typed CSS comparison lengths preserve percentage, font and viewport dependencies
until the correct sizing context is available. Responsive SVG sizing and the
following sibling's flow match Chrome for nine rectangles at three viewport
widths, plus a matched 125%-scale capture, within 0.02 CSS pixels. The aspect-ratio margin fix is general block
layout behavior, not a game stylesheet override.

| Controlled CPU/readback Canvas fixture | Before (#228) | After | Chrome |
| --- | ---: | ---: | ---: |
| Median of nine warmed batches across three fresh runs | 18.8 ms | 10.5 ms | 1.2 ms |
| Stable whole-bitmap checksum | 1672046960 | 1672046960 | 405226884 |

This is about a 44% Breeze median improvement on that fixture, not a general
browser-speed claim. Breeze's before/after pixels are identical; Chrome's
different, stable checksum reflects remaining raster differences. The fixture
requests CPU-oriented storage and includes a readback fence in every timed
batch. No deferred work or initial GPU-to-CPU migration is counted as a win.

The same native shader backend now handles bitmap clips for gradients and
repeating patterns. Three fresh matched runs, nine warmed batches per shader,
retain identical Breeze before/after pixels on the clipped readback fixture:

| Clipped shader median | Before extension | After extension | Chrome 154 |
| --- | ---: | ---: | ---: |
| Linear gradient | 9.7 ms | 1.6 ms | 0.4 ms |
| Radial gradient | 12.0 ms | 2.0 ms | 0.3 ms |
| Repeating pattern | 16.4 ms | 1.9 ms | 1.8 ms |

The before column already includes the ownership changes above. Chrome's
gradient pixels still differ; its pattern checksum matches. These are bounded
CPU/readback workloads, not evidence that the game now completes startup.

The private Canvas geometry transport also reduces a dense 64-square Path2D
readback workload from **51.5 to 7.2 ms** median, versus **5.5 ms** in Chrome,
with unchanged Breeze pixels. The before binary already includes the preceding
Canvas improvements. See [transport, ownership and measurement limits](docs/canvas-geometry-transport.md);
the combined gain is not attributed solely to binary encoding or to game startup.

Game startup is still unaccepted. Failed texture-generation worker tasks consume
substantial owner-thread CPU; sampled GC callback spans do not explain most of
their two-second execution budget. Diagnostics are opt-in, privacy-bounded and
do not raise that watchdog. Numerical worker measurements show no reliable
speedup from the WorkerGlobalScope correction. See
[CSS comparison and geometry contracts](docs/css-comparison-lengths.md),
[broader typed math and scalar-property contracts](docs/css-math-functions.md),
[Canvas ownership and measurements](docs/canvas-region-ownership.md), and
[the remaining game blocker](docs/gd-clone-compatibility.md).

Fresh October 7 captures still render **507/588 before and after** this batch;
the matched Chrome 154 reference renders **579/588**. CSS/Canvas correctness
and throughput work here is not a claimed score increase. Both Breeze captures
report no JavaScript errors. WebGL numeric-list conversion also has
[typed-array ownership/error regressions](docs/webgl-numeric-unions.md) checked
against Chrome, independently of the feature-detection score.

The broader CSS math slice includes stepped, exponential and trigonometric
functions, typed animation times, color components, easing and deferred
translation interpolation. Fresh hidden Chrome comparisons at 100% and 125%
device scale retain independent expected-value checks: all 364 translation
checks match within 0.02 CSS pixels. Known NaN, duplicated easing endpoint,
translucent readback and CSS/API timing discrepancies remain explicit in
[the comparison results](docs/css-math-functions.md); passing fixtures do not
imply universal pixel parity. Successful API timing/keyframe overrides survive
later CSS changes without suppressing updates to unclaimed properties.

Computed scalar math now uses the final element font, root font and viewport
for opacity, flex factors, z-index, preferred ratios and unitless line height.
The same phase resolves animation/transition times, font-dependent `linear()`
outputs, iteration counts and letter/word spacing. This removes declaration-order
guesses without reinterpreting inherited computed values using the child's font.
Shared native/reference fixtures check CSSOM, actual flex/inline rectangles,
live font changes, native timing and terminal paint—not only feature admission.
Text spacing implements CSS Text 3 length values, not a guessed percentage basis.

CSS variable expansion preserves token boundaries and decimal spellings and has
per-declaration limits on serialized size, nesting, token work, cumulative source
scanning and intermediate copying. Exhaustion invalidates the computed winner
instead of selecting a fallback or reviving an earlier declaration. This does
not claim complete computed custom-property graph or registered-property support.

Final-source verification on October 8 passed 583 curated upstream WPT files /
6,425 subtests and 74 required Khronos files / 8,696 subtests, without failure or
timeout overrides. The serial unit suite passed 4,989 tests (four existing tests
ignored); renderer integration passed 273, and live-runtime integration passed
138 (three existing tests ignored). Full Clippy, formatting and source-size
checks also passed. These are selected contracts, not full standards conformance,
and do not expand the hosted CI smoke gate. The October 7 score captures show
507/588 in Breeze before and after, versus 579/588 in Chrome; their ten-second
settling interval is not a time-to-score measurement.

### October 8: bounded graphics work and cancellable script agents

Canvas filter parsing now shares the CSS tokenizer, typed math and color parser.
Native color matrices, alpha-aware Gaussian effects, fractional rectangles,
compact path coverage and ImageBitmap resize reuse existing project backends.
Origin-clean state follows filter-dependent drawing and bitmap ownership;
private presentation pixels are not exposed through author-modified intrinsics.
Bounds and remaining raster differences are documented in
[filter effects](docs/canvas-filter-effects.md),
[compact coverage](docs/canvas-compact-coverage.md), and
[native resize](docs/imagebitmap-native-resize.md).

WebGL uses the existing pinned ANGLE compiler's worker-delegation interface with
a bounded renderer-owned pool. Native completion, link success and shader
validation remain distinct. Pending links preserve mutation/query ordering,
and ready compiler payloads are retired without deleting author executables.
Owner/generation-keyed uniform and attribute reflection and mutation-invalidated
index-range caches avoid repeated scans; mutable draw bounds are still checked.
Independent upload/readback allocations move internally without transferring
an author's ArrayBuffer or widening a destination view. See
[compiler scheduling](docs/webgl-compiler-scheduling.md),
[uniform storage](docs/webgl-uniform-location-storage.md),
[index ranges](docs/webgl-index-range-cache.md), and
[transfer ownership](docs/webgl-transfer-ownership.md).

Document and dedicated Worker entries have a general ten-second author-task
budget; the private regular-expression evaluator keeps two seconds. Explicit
agent retirement interrupts running JavaScript independently of a full Worker
mailbox and permanently rejects later entries. Native work retains its own
bounds and the renderer Job remains the hard backstop. This is an execution
policy change, not a speedup or site exception; see
[execution and cancellation](docs/script-execution-policy.md).

Production GPU storage limits remain 64 MiB per context and 128 MiB per renderer,
with the unchanged 1 GiB renderer Job. The unchanged sibling game is still not
accepted as rendered gameplay: its final production replay gets past loading
but has a blank 3D view and shader errors. An authorized larger-budget experiment is
separate from production policy, and faster owned fixtures alone cannot establish
game readiness. See [resource accounting](docs/webgl-resource-accounting.md) and
[the game acceptance history](docs/gd-clone-compatibility.md).

Fresh October 8 captures still render **507/588 before and after**, compared
with **579/588 in Chrome 154**. No score gain is claimed. Three fresh alternating
release runs on the same machine give these combined-batch medians, with CPU
sampling disabled and no concurrent builds/tests:

| Completed owned workload | Merged #229 | October 8 release | Chrome 154 |
| --- | ---: | ---: | ---: |
| Compile/link 24 materials | 533.3 ms | 163.0 ms | 104.8 ms |
| 2,000 draws, eight active attributes | 28.4 ms | 15.3 ms | 2.5 ms |
| 32 replacements of one 4-MiB buffer | 84.7 ms | 32.1 ms | 176.8 ms |
| 100 draws, 120,000 indices each | 7.0 ms | 3.4 ms | 3.3 ms |
| Twelve bilinear bitmap enlargements | 132.0 ms | 67.7 ms | 116.4 ms |
| Warm tall-stroke weave | 304.6 ms | 73.6 ms | 1.7 ms |
| Fractional rectangle fills | 12.9 ms | 3.6 ms | 0.2 ms |

Shader completion uses real native status; subsequent per-program pixel checks
pass in all runs. Other timed rows include completion/readback. Uploads also
preserve source independence after mutation. Canvas before/after hashes match
for the listed workloads, while Chrome raster/resize results generally differ.
Cold/changed tall geometry remains substantially slower than the warm case;
full measurements and limits are in the linked documents. These fixtures do not
measure game readiness, establish identical driver/kernel behavior, or rank
whole-browser speed. No dependency or copied upstream source is added.

Local verification of this source release passes 5,238 library tests, 273
renderer-process tests, 138 live-runtime tests and the three dedicated smoke
tests. Curated WPT passes all 583 files / 6,425 subtests; curated Khronos passes
all 74 files / 8,696 subtests, without expectation waivers. Full-target Clippy,
format and source-size checks also pass. These local suites do not expand CI
beyond its existing smoke workers or imply complete web-platform compliance.

YouTube remains work in progress: non-DRM video/audio can play, but startup, seeking/recovery,
video frame cadence, layout fidelity, and memory use are not an accepted browser baseline.
Passing media fixtures does not establish usable live-site playback. Wikipedia has dedicated
layout and scrolling regressions, but passing those pages is not a guarantee for every article.

## License

Breeze is available under the MIT License. Locked third-party dependencies and their license
provenance are documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
