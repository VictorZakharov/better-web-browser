# Breeze

Breeze is a performance-first, document-oriented browser MVP written in Rust. It uses native Win32 controls, Windows WinHTTP for HTTP/TLS, a compact HTML-to-document parser, and a retained GDI text display list. The release executable has no third-party runtime dependencies.

This is the first half of the proposed dual-engine design: the small readable-document fast path. It is not yet a general Chromium replacement.

## Run it

Requirements: Windows 10/11 and a current stable Rust toolchain.

```powershell
cargo run --release
```

You can also open a URL immediately:

```powershell
cargo run --release -- https://example.org/
```

The address bar accepts HTTP(S) addresses and search terms. Click blue text to follow links; use the mouse wheel or native scrollbar to navigate long documents.

## Built-in task manager

Click **Task manager** in the toolbar. The modeless popup refreshes every second and displays:

- Normalized process CPU utilization
- Working set, private memory, and peak working set
- Process handle count and uptime
- Active requests, completed/failed pages, and downloaded bytes
- Last HTML parse duration and retained draw-item count

For an automated smoke check, start with `--task-manager`.

## Chromium comparison

The sibling `../better-web-browser-chromium-baseline` project controls an installed Chrome/Edge build through the Chrome DevTools Protocol. It launches a fresh profile for every run and measures the entire Chromium process tree.

Run a comparison from this repository:

```powershell
.\benchmarks\compare.ps1 -Urls https://example.org/ -Iterations 3
```

Reports are written beneath `benchmark-results/<timestamp>/`. Each report includes medians for window readiness, process-start-to-page-ready time, navigation, CPU time, working set, private memory, and process count.

The benchmark is intentionally transparent about scope: Breeze downloads and lays out readable HTML, while Chromium executes the complete page with CSS, JavaScript, media, accessibility infrastructure, GPU services, and site-isolated subprocesses.

## Architecture

```text
Address/search input
        │
        ▼
URL normalization and history
        │
        ▼
Background WinHTTP request ─────► atomic telemetry
        │
        ▼
Bounded HTML document parser
        │
        ▼
Compact blocks and linked spans
        │
        ▼
Width-aware retained text layout
        │
        ▼
Visible-item native GDI painting
```

The network request runs off the UI thread. Parsed documents are capped at 2 MiB of rendered text, response bodies at 16 MiB, and scripts, styles, iframes, SVG, canvas, and templates are discarded before layout.

## Test and benchmark modes

```powershell
cargo test --all-targets
cargo build --release

.\target\release\better-web-browser.exe `
  --benchmark https://example.org/ `
  --output result.json `
  --settle-ms 2000
```

Benchmark mode paints the page, waits for the requested settle period, writes one JSON record, and exits automatically.

## Current limitations

- Windows-only native shell
- No CSS, JavaScript, images, forms, cookies, downloads, tabs, or compatibility fallback yet
- Readable-content extraction rather than standards-complete HTML layout
- UTF-8 and BOM-marked UTF-16 decoding only
- Basic accessibility and no text selection
- Parser and native boundary have not been security-audited

Do not use this MVP for sensitive authenticated browsing. The next meaningful milestone is a hardened parser plus tabs/hibernation, followed by a full-engine compatibility handoff.
