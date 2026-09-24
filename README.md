# Breeze (temporary name)

Breeze is a performance-first browser-engine MVP written in Rust. The product name is provisional and isolated in `src/branding.rs` so it can be replaced without touching engine code.

This is not a Chromium, WebView2, Gecko, or operating-system web-view wrapper. The executable owns its HTML DOM, CSS cascade, JavaScript bindings, layout, display list, resource loading, image/SVG/font decoding, form submission, cookie jar, and Win32 painting path.

## Run it

Requirements: Windows 10/11 x64, PowerShell 7, Rust via rustup (the repository pins
Rust 1.95.0), and the Visual Studio C++ build tools/Windows SDK for the MSVC target.
The first build downloads locked dependencies and the checksum-verified V8 library.

```powershell
./scripts/prepare-v8.ps1 -Profile release
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

- HTML5 tree construction with an engine-owned DOM
- [Live DOM collection iteration and supported CSSOM property exposure](docs/collections-and-capabilities.md), with mutation-aware iterators and authored capability fallbacks
- [Detached HTML/XML DOMParser documents](docs/detached-document-parsing.md), inert parsing, namespace-aware XML nodes, and shared XHR document-response parsing
- [URL and native request resolution](docs/url-request-resolution.md), explicit public bases, live query parameters, and requests independent of author URL replacements
- [Synchronous document streams and replacement](docs/document-streams-and-pre-wrap.md), plus [parser mutation notifications and autonomous custom-element construction](docs/parser-observation-and-cssom.md)
- A growing CSS cascade with custom properties, `calc()` lengths, block/inline flow, flex, grid, table, float, and positioned layout
- Standards-based layout fixes and their headless Chrome comparisons are tracked in [layout compatibility](docs/layout-standards.md), including explicit remaining gaps.
- External stylesheets with [nested import loading and separate script/paint gates](docs/stylesheet-loading-dependencies.md), CSS background images, raster images, alpha compositing, inline/external SVG geometry (SVG text is not yet painted), and renderer-owned webfont parsing plus Rust text shaping, fallback, and rasterization
- [Owned and imported CSSOM](docs/parser-observation-and-cssom.md): preferred titled sheets, per-occurrence import identity, rule edits reflected in the cascade, and constructed/adopted sheets
- A bounded V8 JavaScript runtime with browser Annex B syntax, owned DOM bindings, capture/target/bubble events, retained timers, [native microtasks independent of author Promise implementations](docs/native-microtasks-and-resource-invalidation.md), navigation, and browser-authoritative cookie/storage projections
- [HTML event-handler attributes](docs/html-event-handlers.md), with lazy compilation, DOM scope lookup, stable listener ordering, cancellation, and body/window forwarding
- [IntersectionObserver geometry and queued snapshots](docs/intersection-observer-geometry.md), with containing-block overflow clips, nested scroll margins, and callback microtask checkpoints (remaining geometry and v2-visibility gaps are explicit)
- Progressive document/worker Fetch response streams with bounded backpressure, Fetch/XHR body primitives, abort signals, static/dynamic document ECMAScript modules with top-level await, and isolated classic/module dedicated workers
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

The visual benchmark runs on every push to `main`, not on pull requests. It requires intact major content, nonblank captures, no Breeze script errors, bounded visual difference, Breeze page-ready no slower than two times Chromium load, and stable six-second early scrolling on the long-form fixtures. PRs retain core, renderer, and focused Windows integration tests, lint, formatting, dependency/security policy, and harness self-tests. Curated WPT and full-browser end-to-end tests also run on main. Relevant local integration tests and visual comparisons remain necessary before review: deferred CI checks can first detect a regression after merge. Performance claims remain valid only for feature-equivalent controlled paths. See [the benchmark methodology](benchmarks/README.md), [CI policy and timings](docs/build-performance.md), [modern-search acceptance](docs/modern-search-acceptance.md), and [latest alpha evidence](docs/alpha-compatibility.md) for the matrix, metric definitions, thresholds, medians, and limitations.

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
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all -- --check
./scripts/check-source-size.ps1
./scripts/prepare-v8.ps1 -Profile release
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

A pinned, curated 397-file Web Platform Test suite covers 3,393 upstream harness subtests across HTML
and detached HTML/XML parsing, DOM and mutation, events, event-loop ordering, URLs, Fetch/XHR, cookies, forms, modules,
Web IDL, window/port messaging, [Web Storage values and persistence](docs/web-storage.md), User Timing/PerformanceObserver,
and CSS cascade/selectors/layout and stylesheet MIME validation. Upstream fixtures stay in a separate sparse WPT checkout;
after preparing that checkout, the suite runs offline with one hidden command. All 3,393 selected
subtests pass at the pinned revision, with no expected-failure, skip, or timeout allowances:

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
| ◩ | HTML and DOM | The engine owns its DOM and implements substantial HTML5 tree construction, mutation, and event propagation behavior. Web-platform conformance is still incomplete. |
| ◩ | CSS, layout, and painting | The cascade, custom properties, calculated lengths, common block/inline, flex, grid, table, float, and positioned layouts, images, SVG, and webfonts work on selected pages. Selector, layout, invalidation, and painting coverage remain incomplete. |
| ◩ | JavaScript and browser APIs | A bounded retained V8 realm provides owned DOM bindings, capture/target/bubble events, trusted pointer/keyboard/text/focus/scroll/visibility dispatch, timers, microtasks, navigation, browser-authoritative cookie/storage projections, and other early browser APIs. IME/composition and cancelable `beforeinput`, many HTML event-loop sources, and much of the wider browser API surface remain incomplete. |
| ☑ | HTTP navigation policy | Typed navigation and Fetch policy cover tuple origins, guarded headers, redirects, scoped cookies, CORS/preflight checks, bounded bodies, and document-wide cancellation. This is an early implementation rather than a security-audited replacement for a mature browser network stack. |
| ◩ | Cookies and Web Storage | Browser-owned cookies implement RFC-oriented domain/path, expiry, public-suffix, Secure, HttpOnly, SameSite, prefix, ordering, quota, and restart-persistence behavior. Origin-scoped `localStorage` persists; `sessionStorage` is tab-scoped. Named properties preserve UTF-16 values, and same-origin tabs synchronize local storage with ordered `storage` events. Child-frame event scope, partitioned state, and user-facing data controls remain incomplete; see the [storage contract](docs/web-storage.md). |
| ◩ | IndexedDB | Browser-owned origin-scoped databases provide asynchronous transactions, ordered keys/ranges/cursors, structured-cloned values, restart persistence, and secondary indexes with unique, multi-entry, compound-key-path, and cursor operations. Worker exposure and full cross-tab scheduling remain incomplete; see the [current standards slice](docs/html5test-indexes-crypto-media.md). |
| ◩ | JavaScript Fetch and XHR | Document and dedicated-worker Fetch bodies stream progressively through default readers, with byte-based backpressure, cloning, and cancellation. Fetch/XHR and Body primitives are implemented; BYOB, streaming uploads, and complete pipe/transform semantics remain incomplete. See the [streaming contract and measurements](docs/progressive-fetch.md). |
| ◩ | WebSocket | Document WebSockets use browser-owned network transport, bounded IPC, CSP/mixed-content policy, text/binary frames, and close events. Worker exposure, extensions, and compression remain incomplete; see the [standards slice](docs/html5test-websocket-indexeddb-forms.md). |
| ◩ | ECMAScript modules | Static graphs, top-level `await`, and [dynamic document JavaScript modules](docs/dynamic-modules.md) are implemented. Import maps/attributes, other module types, and dynamic worker imports remain gaps. |
| ◩ | Web Workers | Isolated classic and module dedicated workers are implemented. Shared Workers and Service Workers are not. |
| ◩ | Web Crypto and Gamepad | Secure Window and dedicated-worker realms expose CNG-backed digests, HMAC, AES, key derivation, NIST-curve ECDH/ECDSA, and RSA-OAEP/PSS/PKCS#1 v1.5 with bounded key import/export. The Windows Gamepad API polls XInput controllers after user interaction. Other algorithms, hardware-backed key storage, non-XInput devices, and haptics remain gaps; see the [current standards slice](docs/html5test-indexes-crypto-media.md). |
| ◩ | Script scheduling | Streaming parsing, [synchronous writes](docs/synchronous-document-write.md), [document replacement](docs/document-streams-and-pre-wrap.md), [synchronous dynamic inline classics](docs/inline-scripts-and-table-geometry.md), parser mutation notifications, autonomous custom-element construction/reactions, independently ready classic `async` scripts, deferred/module readiness, [dynamic module insertion](docs/dynamic-modules.md), and document load tasks are implemented slices. Stylesheets have separate parser-script and paint gates. Customized built-ins and the complete HTML rendering/event-loop model remain incomplete. |
| ◩ | Images and fonts | Document images, CSS backgrounds, SVG, alpha compositing, detached JavaScript-created `Image` fetch/decode, and webfonts are supported. The sandboxed renderer owns font parsing, advanced shaping, fallback, and glyph rasterization; the browser validates and composites only bounded raster assets and placements, so remote font bytes never enter the privileged process. CSS Fonts coverage, variable-font controls, vertical text, and broader image resource-selection behavior remain incomplete. |
| ◩ | Forms and input | Native text, search, password, select, and button controls plus GET forms are supported through renderer-owned DOM state and default actions. Checkboxes/radios have separate checked/default state, activation, grouping, and reset behavior. Date/month/week/time/datetime-local numeric values, applicable UTC date values, and color sanitization have targeted standards coverage. Control styling, broader form/reset behavior, IME/composition, cancelable `beforeinput`, and document text selection remain incomplete. |
| ☑ | Tabs and windows | Multiple live tabs, history, tab search and restoration, keyboard shortcuts, multi-selection, reordering, and detach/redock across windows are supported. Persistent tab sessions across browser restarts are not. |
| ◩ | Canvas, media, and downloads | Bounded software Canvas 2D provides real sRGB pixels, SVG paths, fills/strokes, gradients/patterns, clipping, shadows, filters, compositing, shaped/rasterized text, Geometry Interfaces, `ImageData`, image drawing, `ImageBitmap`, `OffscreenCanvas` (including workers), and PNG/JPEG/WebP export. The contained media worker provides non-DRM H.264/AAC MP4 playback, synchronized XAudio2 output, progressive Media Source input, and play/pause/seek/volume/mute/fullscreen controls. HTML text tracks can load WebVTT and paint bounded captions; audio/video track lists expose the accepted decoder streams and allow disabling their output. [Remaining limits](docs/html5test-animation-media-csp.md) include broader codecs, DRM, multiple selectable decoded streams, full caption styling/regions, picture-in-picture, and mature downloads. |
| ◩ | Web Animations | Script-created keyframe animations participate in the CSS cascade, with document timelines, effect inspection, playback promises/events, easing, replacement, and bounded style commitment. Painter interpolation covers numbers, colors, and supported 2D translations; compositor offloading and the full animation/transition model remain open. See the [animation contract](docs/html5test-animation-media-csp.md). |
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
capture most of these contracts. The file picker and full module graph remain open.
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

Reproduce the latest snapshot on Windows x64 with the release build above (1280×720 hidden window,
125% scale, `en-US`, new profile); retain both the JSON diagnostics and rendered score:

```powershell
./scripts/run-hidden-benchmark.ps1 -Url https://html5test.co/ -FreshProfile `
  -WindowWidth 1280 -WindowHeight 720 -DeviceScaleFactor 1.25 -Locale en-US `
  -SettleMs 10000 -TimeoutSeconds 60 -DiagnosticSelector '#score' `
  -Output target/html5test/2026-09-24-animation-media-breeze.json `
  -Screenshot target/html5test/2026-09-24-animation-media-breeze.png
```

New releases must refresh or explicitly date these observations using the
[evidence checklist](docs/technical-alpha-release.md#reproduction-and-release-authority).

YouTube remains work in progress: non-DRM video/audio can play, but startup, seeking/recovery,
video frame cadence, layout fidelity, and memory use are not an accepted browser baseline.
Passing media fixtures does not establish usable live-site playback. Wikipedia has dedicated
layout and scrolling regressions, but passing those pages is not a guarantee for every article.

## License

Breeze is available under the MIT License. Locked third-party dependencies and their license
provenance are documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
