# Breeze (temporary name)

Breeze is a performance-first browser-engine MVP written in Rust. The product name is provisional and isolated in `src/branding.rs` so it can be replaced without touching engine code.

This is not a Chromium, WebView2, Gecko, or operating-system web-view wrapper. The executable owns its HTML DOM, CSS cascade, JavaScript bindings, layout, display list, resource loading, image/SVG/font decoding, form submission, cookie jar, and Win32 painting path.

## Run it

Requirements: Windows 10/11 and a current stable Rust toolchain.

```powershell
cargo run --release
cargo run --release -- https://www.google.com/
```

The normal page surface is always the default. **Reader** is an explicit optional feature; navigating or reloading returns to the normal page surface.

Current page support includes:

- HTML5 tree construction with an engine-owned DOM
- A growing CSS cascade with custom properties, `calc()` lengths, block/inline flow, flex, grid, table, float, and positioned layout
- External stylesheets, CSS background images, raster images, alpha compositing, inline/external SVG, and webfonts
- A bounded Boa JavaScript runtime with browser Annex B syntax, owned DOM bindings, events, startup timers, dynamically inserted classic scripts, navigation, storage, and cookies
- Native text/search/password/select controls, buttons, and GET forms
- Character-set decoding from BOM, HTTP headers, or HTML metadata
- Links, history, reload, scrolling, and background networking

## Task manager

Click **Task manager**. Its modeless popup refreshes every second and reports normalized CPU use, working/private/peak memory, handles, uptime, network activity, parsing time, and retained display items.

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

Three alternating hidden release runs of `https://www.neolisk.blog/` on 2026-08-13, with a 2-second settle period, produced these medians:

| Metric | Breeze | Chromium | Result |
|---|---:|---:|---:|
| Window ready | 11.7 ms | 192.7 ms | Breeze 16.42x faster |
| Process start to page ready | 495.0 ms | 846.5 ms | Breeze 1.71x faster |
| Navigation | 481.2 ms | 517.1 ms | Breeze 1.07x faster |
| Working set | 91.9 MiB | 607.4 MiB | Breeze 6.61x smaller |
| Private memory | 78.2 MiB | 395.1 MiB | Breeze 5.05x smaller |
| CPU time | 625.0 ms | 3,968.8 ms | Breeze 6.35x lower |
| Processes | 1 | 10 | Breeze uses 10x fewer |

Breeze page-ready is recorded after its first owned layout and paint; Chromium uses its load event and implements substantially more of the web platform. Treat this as a reproducible development snapshot, not a universal or feature-equivalent browser claim.

## Architecture

```text
URL/history -> WinHTTP -> charset decode -> HTML5 DOM
                                      |-> JavaScript/DOM mutation
                                      |-> CSS cascade
                                      |-> resource discovery/decode
                                      `-> box layout -> display list -> Win32/GDI paint
```

The page and Reader surfaces share navigation and networking, but Reader extraction is never selected automatically.

## Verification

```powershell
cargo test --all-targets
cargo build --release

.\target\release\better-web-browser.exe `
  --benchmark https://example.org/ `
  --output result.json `
  --screenshot result.png `
  --settle-ms 2000
```

Benchmark mode keeps its window hidden. `--screenshot` paints an offscreen PNG for visual
verification without putting a browser window on the desktop.

## Honest current limitations

- Windows-only native shell
- JavaScript and cookies implement only an early subset; fetch/XHR, modules, workers, and much of the browser API surface remain incomplete
- The JavaScript realm is retained on the document's UI thread for timer and script-completion tasks, but fetch/XHR, user-input task dispatch, and other event-loop sources remain incomplete
- JavaScript-created `Image` objects currently report asynchronous load errors until their fetch/decode path is connected to the renderer
- Canvas, media, downloads, tabs, accessibility, text selection, and site isolation are not implemented yet
- CSS selector/layout/painting coverage is useful on selected pages but remains far from the complete web platform
- External classic `async` scripts execute on arrival without delaying page-ready, but their fetch currently starts after first paint instead of overlapping HTML parsing; `defer` scheduling is not yet modeled separately
- Native form controls approximate browser control styling; a later owned widget painter is needed for tighter cross-platform parity
- No site isolation or security audit; do not use this MVP for sensitive authenticated browsing

Neolisk's blog is the current visual/performance fixture and renders through the owned engine with its responsive layout, scripts, images, SVG icons, and webfonts. Modern Google results are **not working yet**: Google currently serves an anti-automation challenge whose generated proof it rejects for this client; a fresh headless Chromium profile on the same machine/network is also sent to Google's unusual-traffic page. Breeze renders Google's actual HTTP error document and never reroutes it to another provider. DuckDuckGo's HTML results are an explicitly requested compatibility fixture and now render close to the Chromium reference, although generated select chevrons and some native-control details still differ; this is not evidence that Google search is solved.

As of 2026-08-13, the hidden release build completes HTML5test and renders a score of **158 / 588** with zero JavaScript errors. That deliberately low result is a compatibility inventory, not a conformance claim; Web Platform Tests remain the authoritative source for implementing and regressing individual standards features.

## License

Breeze is available under the MIT License. The modified vendored Boa engine and
its local patch inventory are documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
