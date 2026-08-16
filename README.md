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

The normal page surface is always the default. **Reader** is an explicit optional feature; navigating or reloading returns to the normal page surface.

Current page support includes:

- HTML5 tree construction with an engine-owned DOM
- A growing CSS cascade with custom properties, `calc()` lengths, block/inline flow, flex, grid, table, float, and positioned layout
- External stylesheets, CSS background images, raster images, alpha compositing, inline/external SVG, and webfonts
- A bounded Boa JavaScript runtime with browser Annex B syntax, owned DOM bindings, capture/target/bubble events, startup timers, dynamically inserted classic scripts, navigation, storage, and cookies
- Native text/search/password/select controls, buttons, and GET forms
- Character-set decoding from BOM, HTTP headers, or HTML metadata
- A typed Fetch/navigation pipeline with tuple origins, guarded headers, redirect modes, scoped cookies, CORS/preflight checks, bounded streaming bodies, and document-wide cancellation
- A capability-free Windows AppContainer renderer lifecycle with bounded IPC, Job limits, crash recovery, hang detection, and Task Manager diagnostics; this Stage 2 child handles lifecycle messages only and does not yet receive page bytes
- Links, history, reload, scrolling, and background networking

## Task manager

Click **Task manager**. Its modeless popup refreshes every second and shows a process tree rooted at
the privileged browser, with one child row per renderer context. Rows report CPU, working/private
memory, handles, uptime, lifecycle state, restarts, and exit diagnostics; document-engine activity
is reported separately.

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

Breeze memory and CPU totals include both its browser process and the live lifecycle-only renderer.
Breeze page-ready is recorded after its first owned layout and paint; Chromium uses its load event
and implements substantially more of the web platform. Treat this as a reproducible development
snapshot, not a universal or feature-equivalent browser claim.

## Architecture

The current page-engine data flow remains in the browser process while the lifecycle-only renderer
boundary is exercised separately:

```text
URL/history -> Fetch policy -> WinHTTP -> charset decode -> HTML5 DOM
                       |                          |-> JavaScript/DOM mutation
                       |                          |-> CSS cascade
                       |                          |-> resource discovery/decode
                       |                          `-> box layout -> display list -> Win32/GDI paint
                       `-> origin/CORS/cookies/redirects/cancellation
```

The page and Reader surfaces share navigation and networking, but Reader extraction is never selected automatically.
The networking boundary and its standards/platform ownership are documented in
[docs/fetch-pipeline.md](docs/fetch-pipeline.md).
The accepted renderer isolation boundary, threat model, IPC contract, and staged Windows migration
are documented in
[ADR 0001](docs/architecture/0001-renderer-process-boundary.md).
Central hostile-input budgets, decoder preflights, renderer termination behavior, and the fuzzing
contract are documented in [docs/security-and-fuzzing.md](docs/security-and-fuzzing.md).

## Verification

```powershell
cargo test --all-targets
cargo build --release
./scripts/run-fuzz-smoke.ps1

.\target\release\better-web-browser.exe `
  --benchmark https://example.org/ `
  --output result.json `
  --screenshot result.png `
  --scroll-samples 12 `
  --window-width 1920 `
  --window-height 1080 `
  --diagnostic-selector '#main' `
  --settle-ms 2000
```

Benchmark mode keeps its window hidden. `--screenshot` paints an offscreen PNG for visual
verification without putting a browser window on the desktop. Repeatable `--diagnostic-selector`
options add bounded computed-style, resource-decode, and native-control geometry facts to the JSON
report; omit them during normal measurements.

### Web-platform regression suite

A pinned, curated 30-case Web Platform Test suite covers HTML parsing, DOM, events, event-loop ordering,
URLs, and the CSS cascade. Upstream fixtures stay in a separate sparse WPT checkout; after preparing
that checkout, the suite runs offline with one hidden command. All 30 cases pass at the pinned
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

## Honest current limitations

- Windows-only native shell
- JavaScript and cookies implement only an early subset; the Fetch policy foundation exists, but the JavaScript `fetch`/XHR APIs, modules, workers, and much of the browser API surface remain incomplete
- The JavaScript realm is retained on the document's UI thread for timer and script-completion tasks, but fetch/XHR, user-input task dispatch, and other event-loop sources remain incomplete
- JavaScript-created `Image` objects currently report asynchronous load errors until their fetch/decode path is connected to the renderer
- Canvas, media, downloads, tabs, accessibility, text selection, and site isolation are not implemented yet
- CSS selector/layout/painting coverage is useful on selected pages but remains far from the complete web platform
- External classic `async` scripts execute on arrival without delaying page-ready, but their fetch currently starts after first paint instead of overlapping HTML parsing; `defer` scheduling is not yet modeled separately
- Native form controls approximate browser control styling; a later owned widget painter is needed for tighter cross-platform parity
- No site isolation or security audit; do not use this MVP for sensitive authenticated browsing

The public blog page used above is the current visual/performance fixture and renders through the owned engine with its responsive layout, scripts, images, SVG icons, and webfonts. Modern Google results are **not working yet**: Google currently serves an anti-automation challenge whose generated proof it rejects for this client; a fresh headless Chromium profile on the same machine/network is also sent to Google's unusual-traffic page. Breeze renders Google's actual HTTP error document and never reroutes it to another provider. DuckDuckGo's HTML results are an explicitly requested compatibility fixture and now render close to the Chromium reference, although generated select chevrons and some native-control details still differ; this is not evidence that Google search is solved.

As of 2026-08-13, the hidden release build completes HTML5test and renders a score of **158 / 588** with zero JavaScript errors. That deliberately low result is a compatibility inventory, not a conformance claim; Web Platform Tests remain the authoritative source for implementing and regressing individual standards features.

## License

Breeze is available under the MIT License. The modified vendored Boa engine and
its local patch inventory are documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
