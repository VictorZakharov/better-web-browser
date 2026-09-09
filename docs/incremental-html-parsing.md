# HTML parser suspension and resumption

Updated 2026-09-09. This slice replaces the renderer's whole-DOM-before-script
startup path with a retained HTML tokenizer/tree builder. It is not complete HTML
streaming or complete `document.write()` support.

## Implemented contract

The existing html5ever driver normally consumes past its `Script` result. Breeze
now owns that boundary. The same tree builder, open-element stack, input queue,
DOM identities and V8 realm survive suspension. An inline classic script sees the
parsed prefix, not elements after its closing tag. An external blocking classic
script suspends further parsing until its fetch succeeds or fails. A renderer
checkpoint can present the prefix and process input/network work during that wait.

Script source, kind and fetch options are prepared from the reached element and
the then-current base URL. Speculative downloads may finish earlier; validated
bytes are cached under the existing resource budget, but do not admit execution.
The parser prepares each element once. Shared responses serve later owners without
sharing their execution or events. Removed/adopted owners retain the previous
preparation-time rules.

Ready async elements may execute while a blocking script is waiting. Deferred
classic/module execution waits for actual parser EOF. `readyState` stays `loading`
through blocking evaluation and changes to `interactive` at EOF; deferred, DCL
and window-load gates remain distinct. A successful blocking external script's
promise cleanup and `load` happen before parsing resumes. Fetch failure dispatches
`error` and releases the pause. Cancellation/navigation drops the retained parser
with the replaced document.

These boundaries follow HTML's [text insertion mode](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-incdata),
[script preparation/execution](https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element)
and [end-of-parsing steps](https://html.spec.whatwg.org/multipage/parsing.html#the-end).
The existing dependency is reused; no parser fork, dependency or site-specific
branch is introduced.

Parser checkpoints refresh cached DOM child lists, named properties and upgrades
for newly parsed custom elements. Resource discovery does not repeatedly clone
previously parsed script bodies. Existing node/input limits remain enforced;
depth truncation now stops parsing at a bounded chunk checkpoint instead of letting
an adversarial open-element chain grow until EOF. The retained scheduler yields
between script boundaries after an 8 ms slice; this is not a hard execution deadline
for an individual script or tokenizer chunk.

Buffered classic `document.write()` output now enters the retained tokenizer at
the parser insertion point, before unread input. That preserves tree-construction
context, split tokens and written-script ordering **after the caller returns**.
It deliberately does not claim the synchronous re-entrant write algorithm.

## Verification

Owned regressions cover prefix visibility, paused-renderer ping/idle behavior,
early speculative completion, async work during a stylesheet wait, failed blocking
fetches, current-base preparation, insertion-point writes and written scripts,
cached child lists/custom-element upgrades, inert incomplete scripts, retained
node identity, and adversarial depth limits. Existing renderer navigation tests
exercise replacement while a blocking script's response is still outstanding.

Some old tests depended on the invalid whole-DOM assumption. The fullscreen policy
fixture now creates a body before using `document.body`. The stylesheet-order test
installs its listener before the blocking stylesheet; a following inline script
cannot retroactively observe its load. Renderer helpers pump readiness events
instead of treating the first partial presentation as a completed document.

## Before / after / Chromium evidence

Three fresh-profile runs per browser on the same Windows machine and local server,
using the saved release from merged PR #140 (production revision `e4ec32a`), the
new release at `e0cf80a`, and headless Chromium `152.0.7977.83`. Runs were serial,
without simultaneous build/test workloads; screenshots were sampled every 500 ms.

| Observable milestone / contract | Before: PR #140 | Retained parser | Chromium |
| --- | --- | --- | --- |
| First sampled visible prefix, all three runs | 2.5 s | 0.5 s | 0.5 s |
| First sampled ready async panel, all three runs | 2.5 s | 0.5 s | 0.5 s |
| First sampled successful final state, all three runs | Never | 2.5 s | 2.5 s |
| Correct parser/execution/lifecycle contract | 0 / 3 | 3 / 3 | 3 / 3 |

The 0.5-second screenshots were inspected: the previous build was blank, while
the new build and Chromium showed the prefix and ready async panel, with the
blocking-script result still pending. At 2.5 seconds the new build and Chromium
showed all three success panels; the old build reported the failed contract.
All 63 sampled images were also checked for the fixture's colored stage panels.
The layouts are not pixel-identical; these observations test stage visibility.

The new release's async execution was 106 ms median (105–116 ms) after the first
inline bootstrap, versus Chromium's 111 ms (108–114 ms). Window load was 2,026 ms
(2,024–2,030 ms), versus Chromium's 2,016 ms (2,015–2,017 ms). The old build's
bootstrap clock started only after its two-second blocking fetch barrier; comparing
its bootstrap-relative timings as navigation speed would hide that delay.

The required trace is
`initial-true|async-true|block-true-loading|micro|block-load|tail|defer-interactive|DCL|LOAD`.
Both Breeze releases reported zero JavaScript errors, but only the new release
passed the behavioral contract. Absence of exceptions alone was not a passing
criterion. The Chromium report does not expose the same JavaScript-error counter;
its trace and rendered stages were checked directly.

Filmstrip values are observation bounds, not exact paint timestamps or a measured
fivefold speedup. The first 500 ms Chromium captures completed at 556–594 ms;
Breeze's completed at 500–501 ms. This deliberately delayed fixture establishes
that unrelated progress no longer waits behind the blocking source. It does not
establish whole-page loading within 10% of Chromium.

Release SHA-256 identifiers:

- Before: `0D7D69744BC5367023EB7A8EC800AB2831AFC2D548FC65968BA74BC01054C1BE`.
- Retained parser: `9211EFF636C133345D199C0EFDE91601E6972D03A0B1E2341DE240EB1F1AA55D`.

Local validation passed: 759 library tests (one existing ignored test), 95 binary
tests, 68 isolated-renderer tests, and 36 hidden live-runtime tests (three existing
ignored tests), including the new hidden parser fixture. Formatting, source-size
limits and all-target Clippy with warnings denied passed. The adversarial depth
regression completed within the 40 ms focused parser test run.

### Reproduce

Use `benchmarks/alpha/fixtures/parser-blocking-readiness.html`, served by
`scripts/serve-alpha-fixtures.ps1` in a hidden process with a `-ReadyFile`. It
contains a 100 ms async source before a two-second blocking classic source and a
100 ms deferred source after it. Both the prefix and async panel precede the
blocking element; the tail must not exist while that element is waiting.

Run Breeze only through `scripts/run-hidden-benchmark.ps1` with `-FreshProfile`,
`-SettleMs 2800`, `-FilmstripIntervalMs 500`, `-FilmstripDurationMs 3500`,
`-WindowWidth 1520`, `-WindowHeight 1000`, and diagnostic selectors `#prefix`,
`#fast`, `#complete`, `#trace`. Use `-Browser` for the saved merged release.
Chromium's existing launcher uses `--headless`, `--mute-audio` and `CreateNoWindow`;
match the reported viewport (1505.6 × 828 CSS px, rounded to 1506 × 828) and scale
1.25. Use the same settling/filmstrip intervals and selectors.

Inspect the images as well as the manifests. `PARSER_READINESS` console output
and `#complete[data-times]` report identical fixture-relative milestones.
`data-contract=pass` and title `Parser complete` require the whole execution trace.
Keep reports, profiles and screenshots in ignored output directories.

## Remaining boundaries

- The browser still transfers the complete main response before this parser starts.
  Streaming decoding/tokenization during main-response delivery and encoding
  restart rules remain work; generic `page_ready_ms` is not visual completion.
- [Synchronous `document.write()` re-entry](https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#document.write()),
  nested execution before the call returns, `document.open()/close()`, parser pause
  flags and destructive-write policy are not complete. A same-script read immediately
  after `write()` can still differ from Chromium. Fragment insertion is not a substitute.
- Stylesheet waiting consumes the current conservative admitted stylesheet set.
  Disabled/media applicability, pending imports, alternate sheets and exact
  script-blocking versus render-blocking state remain the next stylesheet slice.
- Custom-element construction is notified at parser script/EOF checkpoints, not
  at every individual token. Full parser custom-element reactions, parser-originated
  MutationObserver records, every callback-cleanup boundary, event-handler content
  attributes, and nested browsing-context loading still need dedicated coverage.
- Standalone `Page`/script test helpers still use a completed DOM. The production
  isolated renderer and hidden browser use the retained parser; standalone helpers
  are not evidence of incremental parsing correctness.

This is not a claim of YouTube fidelity, playback improvement, CPU/memory reduction
or whole-page loading within 10% of Chromium. Earlier module gaps remain documented
in [deferred/module readiness](deferred-script-readiness.md).
