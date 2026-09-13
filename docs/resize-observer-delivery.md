# ResizeObserver: layout boxes and rendering delivery

This slice advances [#87](https://github.com/VictorZakharov/better-web-browser/issues/87);
it does **not** close the issue or claim complete ResizeObserver conformance.

## Implemented contract

- Native layout retains used content and border boxes separately from transformed document
  bounds. Block/atomic-inline boxes, replaced images, outer SVG CSS boxes, and native controls
  provide these measurements; margins and document position do not become content size.
- `contentRect` starts at the padding origin. All three entry box arrays are populated;
  `observe({box})` selects the size that activates a notification, not which arrays exist.
  Device-pixel content sizes use native DPR with nearest-integer rounding, a UA-defined choice
  allowed by the specification. Non-replaced inline and non-generated boxes report zero.
- Entries, sizes, and content rectangles have branded read-only interfaces. Observation options,
  receivers, and targets are validated. Frozen arrays do not expose mutable internal history.
- Observer callbacks run in construction order, targets in observation order, with a microtask
  checkpoint after each callback. Repeated gathers use increasing flattened-tree depth, including
  slotted/shadow descendants, so a callback can resize a deeper target in the same checkpoint.
- Self/ancestor resize cycles dispatch the standard loop error and leave undelivered observations
  for a later checkpoint. Callback exceptions do not discard other observers. Document cancellation
  destroys the realm and pending work.
- Registration requests renderer work directly, without calling author-overridable timers.
  Resize callbacks settle before a presentation; IntersectionObserver callbacks remain separate
  queued tasks. Only the final observer-mutated state needs another paintable display list.

The contracts follow the [Resize Observer processing model](https://drafts.csswg.org/resize-observer/#processing-model)
and [HTML rendering update](https://html.spec.whatwg.org/multipage/webappapis.html#update-the-rendering).

## Evidence

`benchmarks/alpha/fixtures/resize-observer.html` grows a component, moves it without resizing it,
and changes only its padding. Its assertions are also run through a hidden live-browser test.
The reference run uses unified-headless Chromium, muted audio, and an isolated temporary profile.

| Final fixture result | Merged baseline | This slice | Chromium |
|---|---:|---:|---:|
| Content-box callbacks | 4 | 2 | 2 |
| Border-box callbacks | 4 | 3 | 3 |
| Content size (CSS px) | 468 × 168 | 420 × 120 | 420 × 120 |
| Border size (CSS px) | 468 × 168 | 468 × 168 | 468 × 168 |
| Content padding origin | 0, 0 | 20, 20 | 20, 20 |
| Fixture assertion | FAIL | PASS | PASS |

The 18 selected upstream ResizeObserver files improve from 10 passing on the merged baseline
to 18 passing, without expected-failure allowances. The complete curated gate is 145 files /
753 harness subtests. Local tests additionally cover branding, fractional DPR, cancellation,
callback microtasks/exceptions, first presentation, late registration, and animation-frame ordering.
Screenshots confirm the component result, not whole-browser visual parity: surrounding line-height
and vertical spacing still differ from Chromium. These are compatibility results, **not load-time
or YouTube performance measurements**. Captures and machine reports stay outside commits.

## Remaining boundaries

The broader upstream inventory still exposes vertical writing-mode/logical-size support,
fragmentation, nested SVG/foreignObject and cross-document geometry, scrollbars, and CSS zoom
gaps. Layout still omits some boxes, notably the normal HTML root and `visibility:hidden` boxes;
this observer layer cannot supply their correct used sizes yet. Do not infer full geometry
support from the API being present.

`eventloop.html` waits on the unimplemented `body.onload` Window-handler alias. `notify.html`
passes its 15 subtests but still reports an overall resize-loop error; that broader scheduling
case is not in the green gate. `observe-019.html` assumes DPR 1 and fails on this desktop's
DPR 1.25 despite correct native scaling; explicit local tests cover both DPR 1.25 and 2 instead.
Reftests, iframe/server-dependent tests, and these failing cases are not silently rewritten or
counted as passing. Keep #87 open until the remaining behavior has standards-backed coverage.

```powershell
./scripts/run-wpt.ps1 -WptRoot ../wpt -Filter 'Resize observers'
cargo test --locked --lib resize_observer
cargo test --locked --test renderer_process resize_observer
cargo test --locked --test live_runtime resize_observer
```
