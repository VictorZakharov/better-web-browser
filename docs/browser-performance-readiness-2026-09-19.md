# Collections, readiness, and search — September 19, 2026

The [implementation contract](collections-and-capabilities.md) covers live indexed
iteration, supported CSSOM properties, and deferral of unpresentable display lists.
This follows [PR #167's URL/request assessment](browser-performance-url-2026-09-16.md).
The HTML search fallback remained enabled at this historical checkpoint. The later
[modern-search acceptance](modern-search-acceptance.md) records its retirement.

This is the historical #168 assessment. The subsequent
[form/navigation slice](form-submission-and-navigation.md) fixes query submission
and identifies the result-link iframe dependency; the measurements and failures
below describe the earlier build, not a fresh measurement of that follow-up.

## Validation

| Measurement | Merged #167 | This slice |
|---|---:|---:|
| Focused live-iteration regressions | 2 fail | 2 pass |
| Curated WPT gate | 380 files / 3,354 assertions | 383 files / 3,372 assertions |
| Owned alpha matrix | 45 pairs / 15 fixtures | 48 pairs / 16 fixtures pass |
| Closed DDG menu left edge, 1,248.8 px viewport | 989.6 px, covering results | 1,252.4 px, outside viewport |
| Wikipedia Main Page first presentation, median | 611.0 ms | 416.4 ms (-31.8%) |
| Coron first presentation, median | 585.7 ms | 507.6 ms (-13.3%) |
| Main Page layout/paint work | 317.3 ms | 259.4 ms (-18.2%) |
| Coron layout/paint work | 893.0 ms | 1,084.7 ms (+21.5%) |

The full library suite passes 1,165 tests (one ignored); the browser binary 115;
WPT runner unit tests 17; renderer integration 129; live runtime 82 (three
live-service tests ignored). Clippy with warnings denied, formatting, and source
size pass, with no size-ceiling increase. The rendering tests cover completion,
failure, removal, imported sheets, and explicit geometry while paint is blocked.

All 48 release Breeze/Chrome alpha pairs pass existing visual/performance ceilings.
The new collection/capability fixture's median visual difference is 0.057 against
its 0.12 ceiling. It observes live mutations and asserts closed, open, and
closed-again menu geometry. The initial fixture run failed because it lacked the
gate's required `#main` region; that structural fixture error was corrected, not
waived. No upstream WPT assertions, thresholds, or browser APIs were overridden.

## Live measurement method

- Before: merged #167, main `9506d98`, preserved release SHA-256
  `26CE3B40660A2317F09E01919C407115C1F1A53E12ADB8519FB64B1521C0ED44`.
- Three interleaved before/after/Chrome trials per Wikipedia page, fresh temporary
  profiles, ten-second settle, no concurrent builds or test suites, OS caches not
  flushed. Windows machine with 24 logical processors; Chrome 153.0.8010.52.
- Breeze window 1280 x 720, content 1248.8 x 548 CSS px; Chrome requested
  1249 x 548 (1250 after scale rounding); scale 1.25 and locale en-US.
- First trial of each browser/page also records a ten-second filmstrip at 500 ms
  intervals. All final Wikipedia runs have eight Appearance inputs, HTTP 200, and
  no Breeze uncaught errors or renderer stop. The previously observed attribute
  error did not recur in either build in this series; the deterministic failing
  reproduction, not a favorable live sample, establishes its fix.
- Timing samples were taken after the production fixes, before the hidden
  control-input harness extension; the final release repeats all owned gates and
  live interaction checks. Capture/selector diagnostics add overhead. Samples with
  extra late script activity are retained, not discarded.

## Readiness and resource use

**First presentation is not complete-page usability.** In the representative
Coron filmstrips, Appearance is still empty at 2.0 s and populated at 2.5 s in
both builds. Chrome already shows those controls in its first 0.5 s filmstrip
sample. Main Page's final-build controls appear between 1.0 and 1.5 s. This does
not establish a general improvement in late-widget readiness or Chrome parity.

Medians over the same series:

| Measurement | Before | After |
|---|---:|---:|
| Coron style work | 482.4 ms | 536.3 ms |
| Coron cumulative CPU | 2,718.8 ms | 2,875.0 ms |
| Coron working / private memory | 192.13 / 159.61 MiB | 191.59 / 158.28 MiB |
| Main Page style work | 286.8 ms | 295.5 ms |
| Main Page cumulative CPU | 1,312.5 ms | 1,500.0 ms |
| Main Page working / private memory | 145.86 / 111.71 MiB | 146.67 / 113.87 MiB |

Coron's total layout work and CPU rose despite earlier first presentation. One
after Coron run used 275.76 MiB and 4,359.4 ms CPU; one Main Page run used
246.00 MiB and 2,312.5 ms CPU. These runs performed additional script/mutation
work and are included in the medians. Three live samples do not isolate causality
or prove a memory improvement/leak. Full-document style/layout remains a priority.

| Page | Breeze first presentation | Chrome load / FCP | Working set B/C | Private B/C | CPU B/C |
|---|---:|---:|---:|---:|---:|
| Main Page | 416.4 ms | 817.6 / 324.0 ms | 146.67 / 645.13 MiB | 113.87 / 374.77 MiB | 1,500.0 / 4,781.3 ms |
| Coron | 507.6 ms | 716.7 / 270.7 ms | 191.59 / 676.30 MiB | 158.28 / 407.77 MiB | 2,875.0 / 5,265.6 ms |

Chrome load includes debugger/harness startup; FCP is navigation-relative; Breeze
first presentation is process-relative. These are not speed-ratio measurements.
Hidden-window creation is 9.6–9.9 ms for Breeze; Chrome debugger readiness is
201.9–216.3 ms. Those are different milestones, not interactive startup parity.
Memory sums the process tree and can double-count shared resident pages. The
engines implement different feature sets and CPU instrumentation covers different
work; the smaller Breeze process tree is not equivalent-work evidence.

A separate final-build Coron six-second early-scroll trace passes: p95
input-to-paint 6.413 ms, maximum 9.877 ms, and zero scroll-only style/layout
rebuilds. A trusted click on Wikipedia's Large text radio selects it and visibly
reflows the article; it produces no uncaught or console errors. Those checks
establish functionality after initialization, not faster control availability.

## Search acceptance and newly identified gap

The closed-menu overlap is fixed by honest feature exposure, allowing the site's
own authored non-3D fallback. No site-specific layout rule was added. Owned
cross-browser tests establish open/close geometry; live trusted activation also
opens the menu at the right edge and closes it with its authored close button.
Escape left the live menu open; no keyboard-dismissal parity is claimed.

The hidden harness can now send native control value/focus events and Enter,
without evaluating script in the page. A loopback search form successfully submits
`standards 🦀` to the correctly encoded query URL. However, the modern DDG search
attempt from `web standards` to `wikipedia web browser` **fails**: the site's React
error boundary reports `TypeError: e.submit is not a function`, and its search
component disappears without navigation. `HTMLFormElement.submit()` is absent.
The uncaught-error array is empty because the site caught the error; console
errors and the final screenshot/URL therefore remain essential acceptance checks.

Programmatic form submission needs a separate standards-based vertical slice,
including its distinction from `requestSubmit`, validation/event behavior,
successful controls, method/encoding, and navigation. This PR does not add a
GET-only stub to claim that API works. See [HTML form submission](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#form-submission-algorithm).
Chrome receives a challenge on the live modern search URL, so no equivalent-content
Chrome search comparison is claimed. The HTML search fallback stays unchanged.

Repeated address navigation from `web standards` to `web browser` and back ends
with the correct title/query, ten result titles, and no uncaught/console errors.
Result activation is **not accepted**: the link-waiting harness reached its 120 s
deadline and terminated its test process tree. A bounded selector-activation
rerun completed but stayed on the results URL rather than reaching the selected
Wikipedia result. The reports do not identify a cause for that separate navigation
failure; it must not be attributed to the confirmed form-method gap without
further evidence. Both failed runs are retained.

## Evidence and reproduction

Ignored artifacts are under `target/readiness-search-proof`: the paired live JSON
and filmstrips (`coron-*`, `main-*`), `alpha-final`, `wpt-final.json`, unit/integration
logs, and the separately retained live interaction reports. Third-party captures
and temporary probes are not committed. Use the current release binaries with:

```powershell
./scripts/run-wpt.ps1 -WptRoot G:/Git/wpt -SkipBuild -Jobs 4
./benchmarks/run-alpha.ps1 -SkipBuild -Iterations 3
./scripts/run-hidden-benchmark.ps1 -Url 'https://en.wikipedia.org/wiki/Coron,_Palawan' `
    -Output target/coron-readiness.json -FreshProfile -DeviceScaleFactor 1.25 `
    -SettleMs 10000 -FilmstripDirectory target/coron-film `
    -DiagnosticSelector '#vector-appearance input'
```
