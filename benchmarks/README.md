# Comparative benchmarks

`compare.ps1` builds Breeze and a separately supplied Chromium reference harness, runs both with fresh hidden processes, and generates raw JSON plus a median-based Markdown report.

Breeze memory, CPU, and process-count fields aggregate its browser process and any live sandboxed
renderer, matching the reference harness's process-tree accounting.

```powershell
.\benchmarks\compare.ps1 `
  -Urls https://example.org/,https://www.rust-lang.org/ `
  -Iterations 3 `
  -SettleMs 2000
```

Use `-ChromiumProject <path>` to select the Chromium reference harness. Use `-SkipBuild` after both release binaries have already been built.

The report deliberately includes a scope warning. Performance comparisons are exploratory until the measured page path is feature-equivalent. Chromium renders and executes the complete web platform; the prototype implements only a JavaScript/browser-API subset and still lacks canvas, media, accessibility, and site isolation.
