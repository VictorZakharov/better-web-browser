# Context Transfer: Better Web Browser

Last updated: 2026-08-13 (America/Toronto)

## 2026-08-13 continuation

The renderer batch below was committed as `e9761fd` (`Expand standards rendering and
diagnostics`). The HTML5test compatibility batch is included in the subsequent local
checkpoint containing this handoff. These commits have not been pushed.

- CSS custom properties now cascade/inherit before `var()` substitution, including
  fallbacks and cycle handling. Linear `calc()` lengths support px, percent, em,
  vw, and vh. The implementation follows the CSS Variables and CSS Values specs
  and reuses `cssparser` component values instead of introducing an ad-hoc tokenizer.
- CSS background images, stylesheet-relative URLs, background geometry/repeat,
  external SVG rasterization, alpha compositing, functional selectors, DPI-aware
  viewport metrics, and several inline-formatting fixes are implemented.
- Normal inline text with only vertical margins no longer becomes an unbreakable
  atomic box, and punctuation after nested inline elements stays attached to the
  preceding text.
- WinHTTP now returns HTTP error responses with their document bodies, matching
  Fetch's distinction between HTTP error statuses and network errors. Failed
  subresources remain filtered out.
- Final hidden release captures are `target/final-neolisk-release.png` and
  `target/final-ddg-release.png`. Neolisk is recognizably close to Chromium.
  DuckDuckGo HTML has matching result width/wrapping and coherent geometry; its
  remaining obvious difference is missing generated select chevrons/native-control
  detail. Both captures returned HTTP 200 with zero JavaScript errors.
- Google still is **not solved**. Breeze remains on Google and never proxies or
  reroutes to another provider. A fresh headless Chromium profile on the same
  machine/network also received Google's unusual-traffic page. Breeze now retains
  and renders Google's HTTP 429 response document instead of showing a blank page.
- A fresh alternating, hidden/headless Neolisk comparison (three samples each)
  measured median page-ready at 574.9 ms for Breeze versus 381.1 ms for Chromium:
  Breeze is currently 1.51x slower, not 3x. Median working set was 33.1 MiB versus
  604.9 MiB (Chromium 18.3x higher); private memory was 17.8 MiB versus 392.7 MiB
  (22.1x higher). Breeze used one process versus Chromium's eleven.
- HTML5test initially stopped at its loading rectangle. Three general engine gaps
  were fixed: Boa's existing `annex-b` feature now accepts browser-required
  HTML-like JavaScript comments; dynamically inserted classic external scripts
  load and execute in the same realm with `load` events; and the bounded startup
  timer queue now uses monotonic virtual due times so rescheduled short polls do
  not starve later timers. JavaScript-created `Image` objects asynchronously report
  `error` until their decode path is connected, allowing unsupported-format probes
  to finish honestly instead of hanging forever.
- The hidden release capture `target/html5test-release.png` now renders an actual
  score of **73 / 588**, HTTP 200, with five scripts, 4,673 DOM mutations, and zero
  JavaScript errors. This is a useful backlog, not a conformance claim. A persistent
  post-load JavaScript event loop, modules, fetch/XHR, workers, and JavaScript image
  decoding remain incomplete. Use selected Web Platform Tests as the normative
  implementation/regression source and HTML5test as the broad dashboard.
- Post-change Neolisk release samples were 513.4, 539.1, and 561.5 ms page-ready
  (539.1 ms median), about 33 MiB working set, 12 scripts, seven DOM mutations, and
  zero errors. The existing visual result remained intact.
- Validation is clean: `cargo fmt --all -- --check`, 73 tests across all targets,
  `cargo clippy --all-targets -- -D warnings`, `git diff --check`, and a release
  build all succeeded.

## Read this first

Continue the owned Rust browser in `G:\Git\better-web-browser`. The active product goal is still twofold:

1. Render `https://www.neolisk.blog/` close enough to Chromium to be recognizably the same page while remaining faster and dramatically lighter.
2. Render a real modern Google results page. A Google recovery/challenge banner or classic fallback page is **not** success.

The user explicitly objected to premature “done” claims. Verify rendered output and benchmark results before reporting completion.

## Non-negotiable constraints

- The browser must own HTML, DOM, CSS, JavaScript bindings, layout, resource loading, and painting.
- Do not embed or fall back to Chromium, CEF, Electron, WebView2, Gecko, or another browser engine.
- The sibling Chromium project is only an external benchmark/visual oracle.
- Reader mode stays optional and is never a compatibility fallback.
- Page-ready performance must be competitive. The user explicitly said a 3x page-ready regression is unacceptable even with a 10x memory win.
- Be candid about incomplete platform coverage and non-equivalent benchmark paths.
- `Breeze` is temporary branding; keep visible identity centralized in `src/branding.rs`.

## Repository state

- Workspace: `G:\Git\better-web-browser`
- Branch: `main`
- Private remote: `https://github.com/VictorZakharov/better-web-browser.git`
- This handoff and the renderer/performance work are in the latest `main` checkpoint; run `git log -1 --oneline` for its hash.
- The worktree was clean when that checkpoint was prepared. Preserve any changes made afterward.
- Use per-command safe-directory overrides for Git if needed:

  ```powershell
  git -c safe.directory=G:/Git/better-web-browser status --short
  ```

## Ripgrep / sandbox behavior

`rg` works for the user and is on the host `PATH`, but sandboxed shell calls still cannot resolve it. Invoke `rg` through `shell_command` with `sandbox_permissions: "require_escalated"` and prefix rule `['rg']`. Do not fall back to slow recursive PowerShell searches unless `rg` is genuinely unavailable.

## What was implemented in this worktree

### Renderer/platform expansion

- Much broader CSS selector, cascade, media-query, grid, flex, float, positioning, percentage-radius, inheritance, control styling, and layout coverage.
- Inline SVG/currentColor rendering, lazy/high-density images, webfont discovery, WOFF reconstruction, dynamic GDI font registration, and post-font relayout.
- A Boa 0.21.1 JavaScript runtime with owned DOM bindings, events, timers/jobs, navigation, storage, URL/base64 helpers, DOM mutation, and browser shims.
- Boa is vendored under `vendor/boa_engine` with a GC map/set borrow-panic fix. Two upstream warning sites were also modernized; Clippy is clean.
- `String.prototype.substr` compatibility polyfill, needed by Google's live challenge script.
- Real JavaScript cookie bridge: `document.cookie` set/get, cookie updates returned to the network layer, domain/path/secure scoping, expiration by `Max-Age`, and header-injection rejection.
- A separate Boa realm for same-origin `iframe.contentWindow`, replacing the incorrect parent-window stub.
- Persistent `HttpClient` for the browser lifetime, so server cookies and pooled connections survive normal navigation/home-to-search flows.
- Browser-like `Accept` and `Accept-Language` request headers.

### Performance work

- Shared WinHTTP session and cached per-origin connections.
- Windows direct-access detection avoids a roughly 170 ms cold automatic-proxy discovery penalty when WinHTTP is configured for no proxy.
- Parallel critical resource fetching, currently up to 24 requests per batch.
- Reader parsing deferred until Reader is actually opened.
- Repeating/long timers no longer block first paint; timer settlement is bounded.
- Async scripts are outside the first-paint path.
- Computed `StyleSet` is cached between resource refresh and layout, cutting Neolisk layout/paint substantially.
- Webfonts are fetched after page-ready and trigger a UI-thread relayout when they arrive.
- Temporary per-resource profiler allocations/timers were removed before the final benchmark.
- Benchmark JSON now includes HTML parse, resource processing, JavaScript, style refresh, and layout/paint timing fields.

Important honesty caveat: non-render-blocking async scripts are currently skipped from the first-paint phase and are **not yet executed later**. On Neolisk this excludes cross-origin GTM analytics and does not change the visible page, but it is still incomplete behavior and makes full feature-equivalence claims inappropriate. Either schedule async execution after first paint or implement an explicit tracker-blocking policy; do not quietly leave the ambiguity.

## Final Neolisk benchmark from this session

Exact harness:

```powershell
.\benchmarks\compare.ps1 -Urls https://www.neolisk.blog/ -Iterations 3 -SettleMs 2000
```

Latest report:

```text
benchmark-results/20260812-205845/REPORT.md
```

Medians from the current final build:

| Metric | Breeze | Chromium | Result |
|---|---:|---:|---:|
| Window ready | 13.0 ms | 228.6 ms | Breeze 17.52x faster |
| Process start to page ready | 482.9 ms | 514.3 ms | Breeze 1.07x faster |
| Navigation | 469.8 ms | 82.6 ms | Breeze still much slower after navigation begins |
| Working set | 35.7 MiB | 653.1 MiB | Breeze 18.29x smaller |
| Private memory | 18.3 MiB | 441.7 MiB | Breeze 24.15x smaller |
| CPU time | 218.8 ms | 3,750.0 ms | Breeze 17.14x lower |
| Processes | 1 | 11 | Breeze uses one process |

This clears the user's total page-ready gate, but narrowly. Do not hide the metric-definition caveat:

- Breeze page-ready = first owned layout/paint.
- Fonts and async scripts are outside Breeze's critical path.
- Chromium page-ready = CDP `Page.loadEventFired` and executes the full platform.

The benchmark disclosure in `benchmarks/compare.ps1` was updated accordingly.

An earlier current-path run before the iframe/session changes was `benchmark-results/20260812-202855/REPORT.md` at 455.0 ms Breeze vs 471.3 ms Chromium. Use the newer `205845` report.

## Neolisk visual status

The optimized release was captured after deferred fonts settled at:

```text
target/neolisk-release-optimized.png
```

It shows the intended Montserrat typography, dark two-column layout, avatar, search box, article metadata, tags, and navigation. It is materially close to the user's Chromium reference. That capture predates only the network-session/iframe-realm changes, which should not affect Neolisk appearance; nevertheless, make one fresh same-size capture before any final visual claim.

## Google status: still not solved

Direct Google search still returns the recovery/challenge document, not results.

What is now proven:

- The live Google challenge executes all five inline scripts without a JavaScript error.
- It generates an `SG_SS` proof-cookie update (about 1.2 KiB; do not print or persist its value in diagnostics).
- Breeze sends JavaScript cookies on the next HTTP request; a local TCP wire test verifies the exact `Cookie` header.
- Cookie domain/path/secure scoping tests pass.
- The follow-up Google request still receives HTTP 429.
- Persistent home-page session cookies do not fix it. A home-to-search probe stored four normal Google cookies and still got the same challenge.
- Breeze, Chrome-compatible, Firefox-compatible, browser-header, and Chrome client-hint probes all received the same initial challenge over non-Chromium Windows/network stacks.
- A real Breeze home-page form submission was exercised after session persistence. It returned to a classic `/webhp` surface, not modern results.
- An isolated iframe JavaScript realm is now implemented and correct by test, but Google still rejects the proof.

Useful local artifacts:

```text
target/google-cookie-benchmark.json
target/google-home-search-session.png
```

The most defensible current conclusion is that Google's anti-automation/browser fingerprint rejects this WinHTTP/Boa client even after ordinary browser semantics are supplied. Do not call the cookie or iframe changes a Google fix. Also do not turn Chromium into a hidden fetch/render fallback.

Potential next investigation, if continuing Google work: compare a fresh Chromium request and Breeze request at the **structural/header/TLS-capability level**, with cookie/token values redacted. Determine whether a standards-relevant API remains missing before attempting more challenge-specific work. Avoid printing `SG_SS` values; an escalation reviewer already rejected unredacted diagnostics.

## Validation at wrap-up

Current source validation:

```text
cargo test --all-targets
52 library tests passed
2 Windows application tests passed
0 failed

cargo clippy --all-targets -- -D warnings
passed cleanly
```

The latest release build completed successfully as part of the final benchmark. Full optimized links are currently slow (the last clean release build took about 3 minutes after vendored-Boa changes), so batch edits before rebuilding.

## Checkpoint cleanup

The temporary Google/Neolisk diagnostic programs under `examples/` were removed before the checkpoint. `README.md` now describes the implemented JavaScript/cookie subset and explicitly states that modern Google results remain blocked. Files under `target/` and `benchmark-results/` are generated/ignored diagnostics unless Git status says otherwise.

## Recommended next-session order

1. Read this file completely and inspect `git status`/the latest benchmark report.
2. Do not rerun the 3-minute release build immediately; first batch any changes.
3. Decide and implement correct post-first-paint handling for async scripts (or an explicit tracker policy), then remeasure Neolisk so the comparison is less ambiguous.
4. Continue Google only with evidence-backed, standards-relevant work; do not claim success until a captured real results page is visible.
5. Make a fresh final Neolisk capture at the same viewport as the Chromium reference.
6. Batch changes, rerun format/Clippy/tests, build release once, then rerun the paired benchmark if code changed.

## Working rules

- Use `apply_patch` for source/file edits.
- Preserve unrelated dirty-worktree changes.
- Use `rg` first for searches (with the escalation workaround above).
- Do not use the in-app browser-control skill to test this owned browser; inspect our generated captures and benchmark output directly.
- Keep commentary concise during long builds/benchmarks.
- Never report “done” from compilation or HTTP status alone; inspect the actual rendered page.
