# Stylesheet dependencies and blocking

Updated 2026-09-15. This slice implements stylesheet dependency loading through
fetch completion, cascade assembly, parser readiness, first presentation, and
element completion events. It does not claim complete CSSOM or loading conformance.

## Implemented contract

- Top-level, correctly ordered `@import` rules in linked and inline sheets discover
  nested HTTP(S) dependencies. The existing CSS tokenizer handles escaped URLs,
  comments, strings and nested condition tokens; text resembling an import inside
  a declaration or nested rule is not a request.
- Import media conditions and the engine's existing declaration-based `supports()`
  evaluator control discovery and application. False conditions do not introduce
  blocking requests. Changed media environments trigger dependency discovery.
- Imported rules precede the importing sheet's own rules. Repeated and diamond
  imports retain each cascade position even when their download is shared. Ancestor
  cycles terminate; URL fragments do not create extra downloads. Redirected sheets
  resolve relative imports and CSS URLs against their final response URL.
- Parser-created, enabled, matching sheets are tracked separately from paint
  blockers. Body links can block subsequent parser scripts without hiding prior
  content. Ordinary script-created links do not become parser-script blockers.
  Initial head sheets and inline imports gate first presentation; explicit dynamic
  head `blocking=render` admission remains supported. Async scripts remain runnable.
- Parent link/style completion waits for applicable imported sheets. A failed
  descendant produces an owner `error`, once, rather than premature `load`.
  Failures, removal, disabling and media changes release applicable readiness gates.
  Successful sibling and parent rules remain usable after an import failure.
- CSS MIME validation rejects non-CSS responses, retaining the same-origin quirks
  exception and absent-metadata CSS default. Redirect origin checks also protect
  stylesheet rule access. Imports retain the existing broker Fetch policy and
  resource byte limits.
- Event delivery wakes nonvisual work independently of repaint requests. An
  unchanged inline stylesheet's load event must not force a second layout/paint
  when scroll observers run.

The loading contract follows [CSS import processing](https://www.w3.org/TR/css-cascade-5/#at-import)
and HTML's [styling/script interaction](https://html.spec.whatwg.org/multipage/semantics.html#interactions-of-styling-and-scripting)
and [stylesheet links](https://html.spec.whatwg.org/multipage/links.html#link-type-stylesheet).

## Bounds and remaining work

The existing 16 distinct stylesheet URL admission limit is retained. Each import
expansion is capped at 256 occurrences and uses the CSS nesting guard. Discovery's
document-wide accumulator is bounded too. Rejected dependencies are terminal and
diagnosed, never permanently pending. These are explicit implementation limits,
not standards limits. Compiled rule budgets and per-resource byte limits remain.

Loaded import metadata is reused; unchanged DOM/source/media checkpoints skip
dependency rediscovery. Fetched payloads do not acquire an independent top-level
cascade position merely because they arrived.

Preferred/alternate titled sheets and per-occurrence `CSSImportRule` graphs/edits are now
covered by [parser observation and stylesheet ownership](parser-observation-and-cssom.md).
Separate boundaries remain: HTTP Default-Style/meta selection and user-facing set selection; cascade layers;
non-HTTP(S) stylesheet sources; complete CSS encoding inheritance and stylesheet
Fetch-option/CORS metadata; and full rendering-opportunity semantics. Layered
imports are not applied as unlayered rules. `supports()` retains the feature-query
subset already implemented by the engine; this change does not make every CSS
Conditional Rules query supported. Resolved `getComputedStyle().width` serialization
is also incomplete; import tests use computed font/color plus synchronous geometry
to verify what the parser observes.

## Reproduction and acceptance

`benchmarks/alpha/fixtures/stylesheet-dependencies.html` imports a child held for
1.5 seconds and a further leaf. A fast async script attaches completion listeners.
The parser, stylesheet owner and window-load milestones are recorded separately.
The final panel must be styled, with title `Stylesheet dependencies complete`.
The merged baseline ignores the imports and fails this fixture.

Start `scripts/serve-alpha-fixtures.ps1` in a hidden process. Run Breeze through
`scripts/run-hidden-benchmark.ps1` with a fresh profile, a 500 ms filmstrip interval,
and a 3-second filmstrip. Run the Chromium harness with its verified `--headless`
and `--mute-audio` launch settings, matching Breeze's reported CSS viewport and
device scale. `STYLESHEET_*_MS` console entries and the panel's `data-marks` attribute
measure the same milestones from the first fixture script. Do not compare Breeze's
first-presentation `page_ready_ms` with Chromium's window-load `page_ready_ms`.

Owned tests cover nested imports, parent/source order, duplicate occurrences,
redirects, cycles, safety limits, media changes, parser/body/dynamic ownership,
failure propagation, geometry visibility and no spurious scroll paint. The curated
WPT suite adds upstream `stylesheet-bad-mime-type.html` unchanged at its pinned
revision, including both empty and nonempty wrong-MIME responses. Its literal
Content-Type sidecars are served by the harness; unsupported header directives
fail closed. Captures and reports remain outside commits in ignored `target` output.

## Release evidence, 2026-09-15

Three sequential fresh-profile runs per browser on the same machine/fixture server:
merged baseline `201a814`, implementation `795dbc9`, Chrome `152.0.7977.83`.
The successful-chain fixture uses a 100 ms parent and 1,500 ms child response delay.
Times below are milliseconds from the first fixture script, median (range).

| Milestone / result | Merged baseline | This change | Chrome |
| --- | --- | --- | --- |
| Fast async script | 111 (105–119) | 109 (105–119) | 115 (115–119) |
| Parser observes completed imports | 127 (120–135), incorrectly early | 1,636 (1,631–1,654) | 1,636 (1,626–1,637) |
| Parent stylesheet `load` | 112 (106–121), incorrectly early | 1,627 (1,619–1,629) | 1,637 (1,635–1,646) |
| Window `load` | 143 (137–165), incorrectly early | 1,652 (1,647–1,686) | 1,637 (1,635–1,647) |
| Final fixture result | FAIL; imported styling absent | PASS | PASS |
| First inspected filmstrip sample with completed styling | Never | 2.0 s | 2.0 s |

The 500 ms filmstrips show no content at 1.5 s and the complete styled panel at
2.0 s in both updated Breeze and Chrome. These are sample bounds, not exact paint
timestamps. Breeze's viewport was 1505.6 by 828 CSS pixels; Chrome used the rounded
1506 by 828 viewport, both at scale 1.25. Breeze captures also include native browser
controls; Chrome captures contain page content only. The baseline's shorter times
are incorrect dependency skipping, not a performance advantage. This correctness
slice is not evidence of a whole-page speedup.

The separate `stylesheet-conditional-import.html` probe records parent completion
and Resource Timing URLs. Chrome 152 fetched the nonexistent `stylesheet-unused.css`
despite `supports(not (display: block))`, then fired `error`. Breeze did not fetch it
and fired `load`. CSS Cascade 5 requires not fetching supports-false imports; this
known reference discrepancy is retained rather than copied into the engine. The
successful-chain comparison above excludes that independent condition; an isolated
renderer regression verifies no request and successful parent completion.

The complete curated WPT run passed **220 cases / 2,169 subtests**, with zero
regressions, crashes or timeouts. Local validation passed **1,396 tests** (four
ignored), including 123 isolated-renderer tests, plus strict Clippy, formatting and
source-size checks.

A fresh-profile live smoke of `Coron,_Palawan` completed with no JavaScript errors,
renderer exits or stopped runtime; all 12 scroll-paint samples completed. Its final
Breeze screenshot had the article columns, infobox and sidebars without obvious
overlap. Chrome received a fundraising banner absent from Breeze, so these live
captures are not an identical-content pixel-parity or speed comparison. The owned
fixture, not changing live Wikipedia content, is the reproducible acceptance gate.
