# Deferred scripts and module graph readiness

Implemented 2026-09-09. This slice separates the parser's ordered deferred list
from its async set, and module graph preparation from evaluation. It does not
establish whole-page or YouTube performance parity.

## Contract

The authoritative and speculative script classifiers agree: an external classic
script without `async` or `defer` still blocks the current startup phase. Deferred
classic scripts and modules can download while the first document frame is shown.
Inline classic `defer` is ignored; an inline module joins the deferred list unless
it has `async`. `async` takes precedence over `defer`.

At the current whole-document parser's EOF, readiness becomes `interactive`.
Deferred elements invoke in document order, after their source/module graph is
ready. A later completed response cannot overtake an earlier deferred element.
The independent async set can proceed while that list waits. Each invocation
finishes its promise jobs before a later script is selected. DOMContentLoaded
stays gated until the deferred list drains; window load also waits for the async
set and the existing admitted load-delaying resources. Pending top-level await
does not keep either document event or the external module's load event waiting.

These decisions follow HTML's [script preparation/execution](https://html.spec.whatwg.org/multipage/scripting.html#prepare-the-script-element)
and [end-of-parsing steps](https://html.spec.whatwg.org/multipage/parsing.html#the-end).
Rendering opportunities while waiting are not a promise of a frame after every
script or at a specific millisecond.

## Ownership and safety

- `document/parser_scripts.rs` owns prepared elements, the async set, and deferred
  list. Shared response bytes do not merge distinct element execution/event tasks.
- `parser_scripts/modules.rs` owns outstanding graph dependencies and terminal
  results. Network completion, not clock polling, makes preparation runnable.
- V8 compiles/discovers module requests without linking or evaluating the body.
  Compiled records belong to the realm's module map, including shared dependencies
  and cycles. Another root does not re-evaluate an already evaluated dependency.
- Prepared source/element identity survives removal and `src` changes. Adoption
  into another document prevents execution in the original document. The external
  file flag is retained at preparation time, not inferred from a later attribute.
- Failed root/dependency fetches release the list and produce trusted element
  errors. Fetch validation includes existing HTTP/MIME/CORS rules. Script parse,
  link, and runtime exceptions are diagnostics, not failed-resource events;
  successfully fetched external scripts still fire load. Inline modules do not
  fire an external-file load event.
- Roots and imported source bytes are charged before compilation. Reusing a
  prepared source does not charge it again during invocation. Existing per-script,
  per-page, resource and execution-watchdog limits remain active. Graph scheduling
  is bounded to 32 imported resource identities per document and 32 preparation
  passes, using the current `MAX_DYNAMIC_SCRIPTS` budget; exhaustion is reported
  and releases the affected element rather than leaving it pending forever.
  No network loader runs inside a parser-prepared script invocation.

## Validation

The owned fixture `benchmarks/alpha/fixtures/deferred-script-readiness.html` serves
a two-second deferred classic source, a module importing a three-second dependency,
a later inline module, and a 100 ms async source. Its visual stages and trace check
that early content can render, async execution proceeds, deferred order is retained,
and DCL/load do not wait for a never-resolving module evaluation promise.

Isolated renderer tests additionally withhold responses entirely, exercise shared
and failed fetches, strict module MIME validation, detached owners, shared cyclic
graphs, and renderer responsiveness during the wait. The same authored fixture
runs through the real hidden browser in `tests/live_runtime/deferred_scripts.rs`.
Core tests verify pre-evaluation byte accounting and module error event semantics.

## Release comparison

Three serial fresh-profile rounds per browser on 2026-09-09, same machine and owned
HTTP fixture server. Baseline is merged PR #139 (`2023f90`, merge `e92c079`);
candidate is `d4b3c7a`; reference is Chromium `152.0.7977.83`. No compiler/test suite
ran alongside these timing rounds. All browsers were hidden/headless and silent.

| Observable / contract | Merged baseline | This change | Chromium |
| --- | --- | --- | --- |
| First sampled initial content, all three runs | 5.5 s | 0.5 s | 0.5 s |
| First sampled async content, all three runs | 5.5 s | 0.5 s | 0.5 s |
| Async invocation after fixture bootstrap, median (range) | 3,049 ms (3,024–3,088) | 106 ms (95–112) | 110 ms (108–113) |
| First sampled successful final state | Never | 3.5 s | 3.5 s |
| Ordered execution / lifecycle fixture passes | 0/3 | 3/3 | 3/3 |

The 500 ms screenshots show the intended removal of the first-presentation
barrier, not an exact elevenfold speedup. Actual first successful sample capture
times were baseline 5,500.1–5,500.5 ms, candidate 500.4–500.5 ms, Chromium
554.8–564.3 ms. Chromium screenshot completion has additional capture latency.
We inspected the early and final images and checked the stage colors across all
filmstrips. This is a loading-contract comparison, not pixel-level layout parity.

Async timestamps are relative to the first inline fixture script. The old build
starts that script only after its two-second initial fetch barrier, so these values
exclude that initial wait. They must not be substituted for navigation timings.
Candidate DCL median was 3,049 ms and load 3,064 ms; Chromium's were 3,013 ms and
3,013 ms, on the same bootstrap-relative clock. The remaining small differences
in this deliberately network-delayed fixture do not prove the requested 10% margin
on arbitrary pages. No YouTube, CPU, memory or playback-speed improvement is claimed.

Raw async milliseconds in round order: baseline `3049, 3024, 3088`; candidate
`95, 106, 112`; Chromium `110, 113, 108`. Both Breeze builds reported zero JS
errors, illustrating why a clean error counter alone is not a compatibility test:
the baseline misses external deferred/module load events and executes async late.
The Chromium harness does not export a JS error counter; its `data-contract=pass`
and captured stages are the recorded correctness evidence.

### Reproduce

Start `scripts/serve-alpha-fixtures.ps1` in a hidden process with `-ReadyFile`.
Use `scripts/run-hidden-benchmark.ps1` with `-FreshProfile`, `-SettleMs 4500`,
`-FilmstripIntervalMs 500`, `-FilmstripDurationMs 6000`, `-WindowWidth 1520`,
`-WindowHeight 1000`, and selectors `#content`, `#fast`, `#complete`, `#trace`.
Use `-Browser` for the saved baseline. Chromium's existing launcher retains
`--headless`, `--mute-audio` and `CreateNoWindow`; match Breeze's viewport
(1505.6 × 828 CSS px, rounded to 1506 × 828) and device scale 1.25.
Read Breeze's `DEFERRED_READINESS` console record and Chromium's `#complete`
`data-times` attribute, and inspect both filmstrip manifests and images.
Reports, profiles and captures belong in ignored output directories, not commits.

The measured release executable SHA-256 is
`0D7D69744BC5367023EB7A8EC800AB2831AFC2D548FC65968BA74BC01054C1BE`;
baseline SHA-256 is `B2D268BDFD6F41E7FD27BE9C1EDF7068E17191AEBCBEAC178209F0810B1B19D2`.

## Explicit remaining gaps

The HTML parser still builds the whole DOM before initial classic scripts run;
incremental tokenization, parser insertion points and `document.write` are the next
slice. Stylesheet applicability and precise script-blocking sheet state remain a
separate gap; this change does not claim those rules are complete. Explicit
`blocking=render` is not implemented here.

This is parser-prepared classic/module readiness, not complete module support.
Dynamic `import()`, import maps/attributes, dynamically inserted module elements,
redirect-aware module identity/base URLs, and worker module loading need separate
coverage. The existing worker loader remains synchronous. Full HTML callback
cleanup checkpoints, inline event-handler content attributes and nested browsing
context load accounting remain the previously documented gaps. No site-specific
selectors, URLs, security exemptions or shortened watchdogs are used.
