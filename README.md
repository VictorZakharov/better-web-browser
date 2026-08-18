# Breeze (temporary name)

Breeze is a performance-first browser-engine MVP written in Rust. The product name is provisional and isolated in `src/branding.rs` so it can be replaced without touching engine code.

This is not a Chromium, WebView2, Gecko, or operating-system web-view wrapper. The executable owns its HTML DOM, CSS cascade, JavaScript bindings, layout, display list, resource loading, image/SVG/font decoding, form submission, cookie jar, and Win32 painting path.

## Run it

Requirements: Windows 10/11 and a current stable Rust toolchain.

```powershell
cargo run --release
cargo run --release -- https://www.google.com/
```

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
- A bounded Boa JavaScript runtime with browser Annex B syntax, owned DOM bindings, capture/target/bubble events, retained timers and microtasks, navigation, storage, and cookies
- JavaScript Fetch/XHR, body and stream primitives, abort signals, static ECMAScript module graphs with top-level await, and isolated classic/module dedicated workers
- Native text/search/password/select controls, buttons, and GET forms
- Character-set decoding from BOM, HTTP headers, or HTML metadata
- A typed Fetch/navigation pipeline with tuple origins, guarded headers, redirect modes, scoped cookies, CORS/preflight checks, bounded streaming bodies, and document-wide cancellation
- One capability-free Windows AppContainer renderer per tab, owning remote-document decoding, HTML/DOM, JavaScript, CSS/layout, image/font decoding, Workers, and immutable presentation output behind bounded IPC, Job limits, crash recovery, hang detection, and Task Manager diagnostics
- Browser-owned multi-tab contexts with independent history, scrolling, native-control focus, navigation and Fetch brokerage, in-flight completion routing, and isolated renderer lifecycles
- Desktop tab workflows including Ctrl/Shift multi-selection, ordered drag/reorder, detach/redock across windows, searchable open/recent tabs, Ctrl+N/Ctrl+Shift+W, Ctrl+Shift+A, Ctrl+T/W/Shift+T, Ctrl+Tab/PageUp/PageDown, Ctrl+Shift+PageUp/PageDown, Ctrl+1-9, Ctrl+L/R, F5, Alt+Left/Right, middle-click, and Ctrl+click
- Links, history, reload, scrolling, and background networking

## Task manager

Click **Task manager**. Its modeless popup refreshes every second and shows a process tree rooted at
the privileged browser, with one child row per stable tab/renderer context. Rows report CPU, working/private
memory, handles, uptime, lifecycle state, restarts, and exit diagnostics; document-engine activity
is reported separately.

## Performance monitor

The bottom-right status-bar counter reports completed content frames for the current tab's active
scroll animation. Click it to toggle a native rolling two-second monitor with FPS, p95 and maximum
frame interval, long-frame count, and paint, JavaScript, style, layout, and resource-processing
time. **Copy diagnostics** exports the URL, summaries, and raw frame-interval series as text for a
bug report. A 250 ms display timer repaints only browser chrome and the monitor surface; those
updates are excluded from the content-frame sequence and cannot inflate its FPS.

## Chromium comparison

The comparison harness runs Breeze and a separately supplied Chromium reference as fresh hidden processes, then reports median timing, memory, CPU, and process counts.

```powershell
.\benchmarks\compare.ps1 `
  -Urls https://example.org/ `
  -Iterations 3 `
  -ChromiumProject <path-to-reference-harness>
```

Performance claims are valid only when the exercised page path is feature-equivalent. The visual acceptance target is perceptual parity: with the same viewport, scale, fonts, locale, and network state, a person looking at the page surfaces side by side should not be able to identify which is Chromium. Exact byte-for-byte raster equality is not required.

### Current benchmark snapshot

Six runs per browser, collected as two alternating three-run hidden release batches against a representative public blog page on 2026-08-14 with a 2-second settle period, produced these combined medians:

| Metric | Breeze | Chromium | Result |
|---|---:|---:|---:|
| Window ready | 13.7 ms | 198.7 ms | Breeze 14.52x faster |
| Process start to page ready | 420.0 ms | 775.6 ms | Breeze 1.85x faster |
| Navigation | 407.2 ms | 436.0 ms | Breeze 1.07x faster |
| Working set | 97.2 MiB | 609.3 MiB | Breeze 6.27x smaller |
| Private memory | 76.5 MiB | 392.1 MiB | Breeze 5.12x smaller |
| CPU time | 523.4 ms | 3,976.6 ms | Breeze 7.60x lower |
| Processes | 2 | 10 | Breeze uses 5x fewer |

Breeze memory and CPU totals include both its browser process and the live page renderer.
Breeze page-ready is recorded after its first owned layout and paint; Chromium uses its load event
and implements substantially more of the web platform. Treat this as a reproducible development
snapshot, not a universal or feature-equivalent browser claim.

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
```

The page and Reader surfaces share navigation and networking, but Reader extraction is never selected automatically.
The networking boundary and its standards/platform ownership are documented in
[docs/fetch-pipeline.md](docs/fetch-pipeline.md).
The accepted renderer isolation boundary, threat model, IPC contract, and staged Windows migration
are documented in
[ADR 0001](docs/architecture/0001-renderer-process-boundary.md).
The renderer-owned Rust text stack, font-byte boundary, and measured dependency decision are
documented in [ADR 0002](docs/architecture/0002-renderer-owned-text-stack.md).
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
post-load timer and async-script tasks; deferred work resumes one task at a time after a 100 ms
quiet period.

### Web-platform regression suite

A pinned, curated 70-case Web Platform Test suite covers HTML parsing, DOM, events, abort signals,
event-loop ordering, URLs, Web IDL, Fetch, XHR, modules, and the CSS cascade. Upstream fixtures stay
in a separate sparse WPT checkout; after preparing that checkout, the suite runs offline with one hidden command. All 70 cases pass at the pinned
revision, with no expected-failure or timeout allowances:

```powershell
.\scripts\checkout-wpt.ps1 -Destination ..\wpt
.\scripts\run-wpt.ps1 -WptRoot ..\wpt
```

The runner emits `target/wpt/report.json` and fails on regressions, crashes, changed failure modes,
and unexpected passes. See [tests/wpt/README.md](tests/wpt/README.md) for provenance, licensing,
expectation policy, filtering, and the exact execution contract.

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
| ◩ | JavaScript and browser APIs | A bounded retained Boa realm provides owned DOM bindings, capture/target/bubble events, timers, microtasks, navigation, storage, and other early browser APIs. User-input task dispatch, many HTML event-loop sources, and much of the wider browser API surface remain incomplete. |
| ☑ | HTTP navigation policy | Typed navigation and Fetch policy cover tuple origins, guarded headers, redirects, scoped cookies, CORS/preflight checks, bounded bodies, and document-wide cancellation. This is an early implementation rather than a security-audited replacement for a mature browser network stack. |
| ◩ | JavaScript Fetch and XHR | Cookies, Fetch/XHR, abort signals, body primitives, and stream primitives are implemented. Progressive delivery from the network into JavaScript streams is not yet connected. |
| ◩ | ECMAScript modules | Static module graphs and top-level `await` are implemented. Dynamic `import()` and import maps are not. |
| ◩ | Web Workers | Isolated classic and module dedicated workers are implemented. Shared Workers and Service Workers are not. |
| ◩ | Script scheduling | External classic `async` scripts execute on arrival without delaying page-ready. Their fetch starts after first paint instead of overlapping HTML parsing, and `defer` is not yet scheduled separately. |
| ◩ | Images and fonts | Document images, CSS backgrounds, SVG, alpha compositing, and webfonts are supported. The sandboxed renderer owns font parsing, advanced shaping, fallback, and glyph rasterization; the browser validates and composites only bounded raster assets and placements, so remote font bytes never enter the privileged process. CSS Fonts coverage, variable-font controls, vertical text, and JavaScript-created `Image` fetch/decode remain incomplete. |
| ◩ | Forms and input | Native text, search, password, select, and button controls plus GET forms are supported. Control styling is approximate, broader form behavior is incomplete, and document text selection is not implemented. |
| ☑ | Tabs and windows | Multiple live tabs, history, tab search and restoration, keyboard shortcuts, multi-selection, reordering, and detach/redock across windows are supported. Persistent tab sessions across browser restarts are not. |
| ☐ | Canvas, media, and downloads | Canvas rendering, audio/video playback, and downloads are not implemented. |
| ☐ | Accessibility | An accessibility tree and platform accessibility integration are not implemented. |
| ◩ | Process and site isolation | Each tab has a capability-free AppContainer renderer that owns remote-document parsing, JavaScript/DOM, CSS/layout, image/font decoding, Workers, and immutable presentation construction. Bounded IPC, Job limits, hang detection, and tab-local containment cover aborts, access violations, OOM termination, and native stack overflow; reload creates a fresh process/session/document identity. Cross-site frame isolation is not implemented. |
| ☐ | Security-audited browsing | The browser has not received a security audit and is not suitable for sensitive authenticated browsing. |

See [JavaScript networking, modules, and workers](docs/javascript-network-runtime.md) for the
implemented contracts, ownership model, standards references, and narrower remaining boundaries.

The public blog page used above is the current visual/performance fixture and renders through the owned engine with its responsive layout, scripts, images, SVG icons, and webfonts. Modern Google results are **not working yet**: Google currently serves an anti-automation challenge whose generated proof it rejects for this client; a fresh headless Chromium profile on the same machine/network is also sent to Google's unusual-traffic page. Breeze renders Google's actual HTTP error document and never reroutes it to another provider. DuckDuckGo's HTML results are an explicitly requested compatibility fixture and now render close to the Chromium reference, although generated select chevrons and some native-control details still differ; this is not evidence that Google search is solved.

As of 2026-08-13, the hidden release build completes HTML5test and renders a score of **158 / 588** with zero JavaScript errors. That deliberately low result is a compatibility inventory, not a conformance claim; Web Platform Tests remain the authoritative source for implementing and regressing individual standards features.

## License

Breeze is available under the MIT License. The modified vendored Boa engine and
its local patch inventory are documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
