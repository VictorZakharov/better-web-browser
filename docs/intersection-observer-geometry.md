# IntersectionObserver: native clips and queued snapshots

This advances [#88](https://github.com/VictorZakharov/better-web-browser/issues/88),
not a claim of complete IntersectionObserver conformance.

## Implemented slice

- Read native fragment bounds and walk the containing-block chain, rather than
  calling an author-overridable `getBoundingClientRect`. Absolute/fixed descendants
  that escape an explicit root are not treated as its normal-flow descendants.
- Apply ancestor overflow clips in viewport coordinates, including nested scroll
  offsets and sticky/fixed positioning. Explicit clipping roots use the padding
  edge, not the border edge. `overflow:clip` clips but is not a scroll container.
- Parse and serialize `scrollMargin`; expand every participating scrollport, not
  just the root. Combine root and scroll margins for a scrolling root, using the
  original rectangle for percentage resolution. `rootBounds` excludes scroll margin.
- Sample geometry after layout/resize-observer settling and native input
  microtasks. Queue immutable entries separately from callback delivery; a later
  layout or callback mutation cannot rewrite an earlier observer's snapshot.
- Preserve construction/observation ordering and run a microtask checkpoint after
  each callback. `takeRecords()` drains the queued snapshots, and `disconnect()`
  stops future sampling without discarding already queued records. Internal
  delivery does not call an overridden author `takeRecords()`.
- Registration wakes idle rendering directly. Delivery is a later renderer task;
  unchanged geometry does not queue another callback. Delay wakeups use private
  scheduling rather than author timers. Document teardown discards realm work.

The primary contract is the [Intersection Observer processing model](https://w3c.github.io/IntersectionObserver/).
Geometry shares the existing CSSOM View scroll/sticky projection and cached styles;
there is no website-specific selector, forced layout polling interval, or new dependency.

## Evidence, 2026-09-16

| Contract | Merged #163 release | This slice |
|---|---:|---:|
| Selected unmodified upstream observer files | 15/36 pass | 36/36 pass |
| Selected observer assertions reached | 56/100 pass | 106/106 pass |
| Full strict curated gate | 325 files / 2,720 assertions | 361 files / 2,826 assertions |
| Owned geometry/delivery fixture | Fails clipping/margins; author geometry override aborts completion | 13/13 pass |

Failures can prevent later assertions from running, so the before/after assertion
denominators differ. No upstream files or assertions were edited and no expected
failures were added. Four focused runtime tests cover snapshots, disconnect,
draining, callback microtasks, branding, and validation. Hidden renderer tests
cover actual scroll input and promise mutations; a live-browser test runs the
original `benchmarks/alpha/fixtures/intersection-observers.html` fixture.

At a matched 1262 x 539 CSS-pixel viewport, muted unified-headless Chrome
153.0.8010.47 passes **12/13** owned assertions. Its remaining mismatch is kept
visible: a 220 x 120 padding root with both margins `10%` produces ratio **0.56**,
versus Breeze's **0.74**. The current specification resolves all four percentage
offsets against the undilated **width**; Breeze retains that contract rather than
changing the expectation to obtain a green Chrome screenshot. Both report
`rootBounds.width = 264`. This is not pixel parity or complete API parity.

```powershell
./scripts/checkout-wpt.ps1 -Destination ../wpt
./scripts/run-wpt.ps1 -WptRoot ../wpt -Filter 'Intersection Observer'
cargo test --locked --lib intersection_observer
cargo test --locked --test renderer_process viewport_observers
cargo test --locked --test live_runtime intersection_observers
```

## Remaining boundaries

The wider 46-file diagnostic probe passes 36 files, not the entire upstream suite.
Remaining failures include root-element/viewport scrollbar geometry, hidden and
zero-area layout boxes, absolute static-position geometry, detached-document
activation, a frame-dependent propagation test, and exact fractional/threshold
expectations. They are not silently counted as passing coverage. Transform/clip-path
mapping, nested browsing contexts and cross-origin geometry remain separate slices.

`trackVisibility` cannot yet prove compositor occlusion/opacity, so `isVisible`
conservatively remains false; this is not implemented positive v2 visibility.
Absolute margins currently serialize at integer CSS-pixel precision, matching the
pinned margin-unit tests, not a claim of arbitrary fractional-length precision.
Keep #88 open. Modern DuckDuckGo compatibility and fresh performance evidence are
reported separately; API presence alone does not establish that the site works.
