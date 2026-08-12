# Breeze (temporary name)

Breeze is a performance-first browser-engine MVP written in Rust. The product name is provisional and isolated in `src/branding.rs` so it can be replaced without touching engine code.

This is not a Chromium, WebView2, Gecko, or operating-system web-view wrapper. The executable owns its HTML DOM, CSS cascade, layout, display list, resource loading, image/SVG decoding, form submission, and Win32 painting path. The sibling Chromium project is an external measurement oracle only.

## Run it

Requirements: Windows 10/11 and a current stable Rust toolchain.

```powershell
cargo run --release
cargo run --release -- https://www.google.com/
```

The normal page surface is always the default. **Reader** is an explicit optional feature; navigating or reloading returns to the normal page surface.

Current page support includes:

- HTML5 tree construction with an engine-owned DOM
- A growing CSS cascade and box-layout implementation
- External stylesheets, raster images, transparent images, and inline SVG
- Native text/search/password controls, buttons, and GET forms
- Character-set decoding from BOM, HTTP headers, or HTML metadata
- Links, history, reload, scrolling, and background networking

## Task manager

Click **Task manager**. Its modeless popup refreshes every second and reports normalized CPU use, working/private/peak memory, handles, uptime, network activity, parsing time, and retained display items.

## Chromium comparison

The sibling `../better-web-browser-chromium-baseline` project launches an installed Chrome or Edge build with a fresh profile and measures the complete process tree. It is not referenced by this crate or shipped in this executable.

```powershell
.\benchmarks\compare.ps1 -Urls https://example.org/ -Iterations 3
```

Performance claims are valid only when the exercised page path is feature-equivalent. The visual acceptance target is perceptual parity: with the same viewport, scale, fonts, locale, and network state, a person looking at the page surfaces side by side should not be able to identify which is Chromium. Exact byte-for-byte raster equality is not required.

## Architecture

```text
URL/history -> WinHTTP -> charset decode -> HTML5 DOM
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
  --settle-ms 2000
```

## Honest current limitations

- Windows-only native shell
- JavaScript, canvas, media, cookies, downloads, tabs, accessibility, and text selection are not implemented yet
- CSS selector/layout/painting coverage is substantial enough for the classic Google page, but far from the complete web platform
- Native form controls approximate browser control styling; a later owned widget painter is needed for tighter cross-platform parity
- No site isolation or security audit; do not use this MVP for sensitive authenticated browsing

Google's same-user-agent fallback page is now a live visual fixture: it renders the real logo, navigation, apps SVG, styled sign-in link, search form, language row, and footer through the owned path. That is a milestone, not a claim of general-web parity.
