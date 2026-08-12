# Comparative benchmarks

`compare.ps1` builds Breeze and the sibling `better-web-browser-chromium-baseline`, runs both with fresh processes, and generates raw JSON plus a median-based Markdown report.

```powershell
.\benchmarks\compare.ps1 `
  -Urls https://example.org/,https://www.rust-lang.org/ `
  -Iterations 3 `
  -SettleMs 2000
```

Use `-ChromiumProject <path>` if the baseline sibling has a different location. Use `-SkipBuild` after both release binaries have already been built.

The report deliberately includes a scope warning. Breeze’s MVP is a restricted readable-document engine; Chromium renders and executes the complete web platform.
