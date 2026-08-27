# Breeze (temporary name)

Breeze is a performance-first browser-engine MVP written in Rust. The product name is provisional and isolated in `src/branding.rs` so it can be replaced without touching engine code.

This is not a Chromium, WebView2, Gecko, or operating-system web-view wrapper. The executable owns its HTML DOM, CSS cascade, JavaScript bindings, layout, display list, resource loading, image/SVG/font decoding, form submission, cookie jar, and Win32 painting path.

## Run it

Requirements: Windows 10/11 and a current stable Rust toolchain.

```powershell
cargo run --release
cargo run --release -- https://www.google.com/
```

Persistent cookies and `localStorage` use `%LOCALAPPDATA%\Breeze`; `sessionStorage` remains
tab-scoped and is not written to disk. Tests and isolated automation can set
`BREEZE_PROFILE_DIRECTORY` to an absolute profile directory.

For a faster optimized edit/build loop, use `cargo run --profile performance`. This profile keeps
optimization enabled but trades release LTO and single-unit code generation for incremental,
parallel compilation. Reproducible performance claims and distributable binaries always use the
canonical `release` profile.

The local rebuild and GitHub Actions timing methodology, latest measurements, and remaining CI
critical path are recorded in [docs/build-performance.md](docs/build-performance.md).

The normal page surface is always the default. **Reader** is an explicit optional feature; navigating or reloading returns to the normal page surface.

Current page support includes:

- HTML5 tree construction with an engine-owned DOM
- A growing CSS cascade with custom properties, `calc()` lengths, block/inline flow, flex, grid, table, float, and positioned layout
- External stylesheets, CSS background images, raster images, alpha compositing, inline/external SVG, and renderer-owned webfont parsing plus Rust text shaping, fallback, and rasterization
- A bounded Boa JavaScript runtime with browser Annex B syntax, owned DOM bindings, capture/target/bubble events, retained timers and microtasks, navigation, and browser-authoritative cookie/storage projections
- JavaScript Fetch/XHR, body and stream primitives, abort signals, static ECMAScript module graphs with top-level await, and isolated classic/module dedicated workers
- Native text/search/password/select controls and buttons whose web-visible state, trusted DOM events, link hit testing, and GET-form default actions are renderer-owned
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

The repository-owned public-alpha gate runs Breeze and unified-headless Chromium against eleven deterministic, original fixtures. Every sample uses a fresh hidden profile on the same machine; the harness aligns viewport, Windows scale, locale, fixture bytes, settle period, and cache policy, then records compatibility captures plus timing, scroll, memory, CPU, and process metrics.

```powershell
.\benchmarks\run-alpha.ps1 -Iterations 3
```

The CI gate requires intact major content, nonblank captures, no Breeze script errors, bounded visual difference, Breeze page-ready no slower than two times Chromium load, and stable six-second early scrolling on the long-form fixtures. Performance claims remain valid only for feature-equivalent controlled paths. See [the benchmark methodology](benchmarks/README.md) and [latest alpha evidence](docs/alpha-compatibility.md) for the matrix, metric definitions, thresholds, medians, and limitations.

### Renderer text cold-path comparison

Three-run hidden release medians on the Wikipedia earthquake fixture compare the historical GDI
control, the first renderer-owned COSMIC Text implementation, and the current lean renderer-owned
pipeline:

| Text backend | Page ready | Non-network | Layout/paint | Working set |
|---|---:|---:|---:|---:|
| GDI control | 576.915 ms | 223.193 ms | 105.540 ms | 155.7 MiB |
| COSMIC Text | 714.472 ms | 321.404 ms | 214.886 ms | 179.668 MiB |
| Fontique + HarfRust + Swash | 534.731 ms | 226.830 ms | 131.399 ms | 183.672 MiB |

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
cargo test --all-targets
cargo build --release
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

Benchmark mode keeps its window hidden. `--screenshot` paints an offscreen PNG for visual
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

### Web-platform regression suite

A pinned, curated 80-file Web Platform Test suite covers 570 upstream harness subtests across HTML
parsing, DOM and mutation, events, event-loop ordering, URLs, Fetch/XHR, cookies, forms, modules,
Web IDL, and CSS cascade/selectors/layout. Upstream fixtures stay in a separate sparse WPT checkout;
after preparing that checkout, the suite runs offline with one hidden command. All 570 selected
subtests pass at the pinned revision, with no expected-failure, skip, or timeout allowances:

```powershell
.\scripts\checkout-wpt.ps1 -Destination ..\wpt
.\scripts\run-wpt.ps1 -WptRoot ..\wpt
```

The runner emits `target/wpt/report.json`, enforces the 200-subtest minimum, and fails on
regressions, crashes, changed failure modes, and unexpected passes. This is a focused regression
gate, not Breeze's whole-platform pass rate. A separate discovery sample records 3 passes and 16
known failures across nearby unsupported behavior. See
[tests/wpt/README.md](tests/wpt/README.md) for the selection rationale, wptrunner evaluation,
provenance, licensing, expectation policy, filtering, and exact execution contract.

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
| ◩ | JavaScript and browser APIs | A bounded retained Boa realm provides owned DOM bindings, capture/target/bubble events, trusted pointer/keyboard/text/focus/scroll/visibility dispatch, timers, microtasks, navigation, browser-authoritative cookie/storage projections, and other early browser APIs. IME/composition and cancelable `beforeinput`, many HTML event-loop sources, and much of the wider browser API surface remain incomplete. |
| ☑ | HTTP navigation policy | Typed navigation and Fetch policy cover tuple origins, guarded headers, redirects, scoped cookies, CORS/preflight checks, bounded bodies, and document-wide cancellation. This is an early implementation rather than a security-audited replacement for a mature browser network stack. |
| ◩ | Cookies and Web Storage | Browser-owned cookies implement RFC-oriented domain/path, expiry, public-suffix, Secure, HttpOnly, SameSite, prefix, ordering, quota, and restart-persistence behavior. Origin-scoped `localStorage` persists and tab-scoped `sessionStorage` does not. Cross-document `storage` events, storage property-name traps, partitioned state, and user-facing data controls remain incomplete. |
| ◩ | JavaScript Fetch and XHR | Cookies, Fetch/XHR, abort signals, body primitives, and stream primitives are implemented. Network responses now stream incrementally and with backpressure from WinHTTP across renderer IPC, but the JavaScript realm still receives each completed body rather than a progressively delivered network stream. |
| ◩ | ECMAScript modules | Static module graphs and top-level `await` are implemented. Dynamic `import()` and import maps are not. |
| ◩ | Web Workers | Isolated classic and module dedicated workers are implemented. Shared Workers and Service Workers are not. |
| ◩ | Script scheduling | External classic `async` scripts execute on arrival without delaying page-ready. Their fetch starts after first paint instead of overlapping HTML parsing, and `defer` is not yet scheduled separately. |
| ◩ | Images and fonts | Document images, CSS backgrounds, SVG, alpha compositing, and webfonts are supported. The sandboxed renderer owns font parsing, advanced shaping, fallback, and glyph rasterization; the browser validates and composites only bounded raster assets and placements, so remote font bytes never enter the privileged process. CSS Fonts coverage, variable-font controls, vertical text, and JavaScript-created `Image` fetch/decode remain incomplete. |
| ◩ | Forms and input | Native text, search, password, select, and button controls plus GET forms are supported through renderer-owned DOM state and default actions. Control styling is approximate; reset/default-value behavior, IME/composition, cancelable `beforeinput`, broader form behavior, and document text selection remain incomplete. |
| ☑ | Tabs and windows | Multiple live tabs, history, tab search and restoration, keyboard shortcuts, multi-selection, reordering, and detach/redock across windows are supported. Persistent tab sessions across browser restarts are not. |
| ☐ | Canvas, media, and downloads | Canvas rendering, audio/video playback, and downloads are not implemented. |
| ◩ | Accessibility | A bounded renderer semantic tree is validated and exposed with browser chrome through AccessKit and Windows UI Automation, including focus/invoke/value actions. Accessible-name/ARIA coverage, rich text patterns, live regions, and non-Windows adapters remain incomplete; see [Accessibility architecture](docs/accessibility.md). |
| ◩ | Process and site isolation | Each tab has a capability-free AppContainer renderer that owns remote-document parsing, JavaScript/DOM, CSS/layout, image/font decoding, Workers, and immutable presentation construction. The browser reconstructs privileged Fetch requests and owns persistent state; bounded IPC/queues, Job limits, hang detection, and tab-local containment cover aborts, access violations, OOM termination, and native stack overflow. Cross-site frame isolation is not implemented. |
| ☐ | Security-audited browsing | The browser has not received a security audit and is not suitable for sensitive authenticated browsing. |

See [JavaScript networking, modules, and workers](docs/javascript-network-runtime.md) for the
implemented contracts, ownership model, standards references, and narrower remaining boundaries.

The [technical-alpha release notes](docs/technical-alpha-release.md) describe the reproducible
unsigned Windows x64 archive, verification and cleanup, acceptance evidence, dependency policy,
and the safety limitations that apply before trying a public build. Development-only licenses and
provenance that are intentionally absent from the shipped graph are tracked separately in
[development third-party material](docs/development-third-party.md).

The deterministic alpha matrix now covers long-form and portal pages, responsive articles, search results, a capability dashboard, forms/storage, layout, media/fonts, and asynchronous mutation. Its opt-in live URLs provide side-by-side evidence rather than CI truth. Modern Google results are **not working yet**: Google currently serves an anti-automation challenge whose generated proof it rejects for this client; a fresh headless Chromium profile on the same machine/network is also sent to Google's unusual-traffic page. Breeze renders Google's actual HTTP error document and never reroutes it to another provider. DuckDuckGo's HTML results remain a compatibility target, not evidence that Google search is solved.

As of 2026-08-13, the hidden release build completes HTML5test and renders a score of **158 / 588** with zero JavaScript errors. That deliberately low result is a compatibility inventory, not a conformance claim; Web Platform Tests remain the authoritative source for implementing and regressing individual standards features.

## License

Breeze is available under the MIT License. The modified vendored Boa engine and
its local patch inventory are documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
