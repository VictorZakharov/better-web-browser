# Query-owned native hit targeting

Ordinary viewport wheel and pointer input needs a current DOM target before
event dispatch. A fast retained scroll-paint counter does not include this
work, IPC delivery, or cancellation by an author listener. See the separate
[wheel measurement contract](wheel-input-measurement.md).

## Change and preserved behavior

The plain-element fallback previously scanned reverse paint order and called
`Dom::find_node` for every candidate. Each lookup could traverse the same DOM
again: resolving P candidates from D owned nodes could cost O(D×P) DOM work.
The fallback now builds one query-owned node-ID index and resolves those
candidates from it. DOM resolution becomes O(D+P); this does not claim the
entire hit test is linear. Visual rectangles, ancestor scroll clips, links,
controls, and event propagation retain their own work.

The index uses the same ownership traversal as `Dom::find_node`, including
template contents, closed shadow trees and adopted ID namespaces. The first
owned occurrence wins if IDs collide. Detached or generated layout-only nodes
are not silently admitted. No index survives the query: template-content
changes need not advance the current document's mutation version, so a
version-only persistent cache would permit stale targets.

This does not implement or change JavaScript `elementFromPoint`. Existing
link/control precedence and explicit renderer targets are unchanged. Fallback
selection still uses reverse paint order, nonempty visual bounds, inclusive
edges, hit exclusions and the current sticky/scroll-clipped geometry. Native
trusted events still cross the ordinary renderer boundary; listener
cancellation must prevent the viewport default action.

## Regression evidence

Owned tests compare old and indexed selection on identical retained geometry,
including overlapping and empty boxes, sticky and nested-scroll geometry,
clip paths, pointer-event exclusions and top-layer clip escape,
closed shadow content, adoption, removal/reinsertion, template mutations and
document retirement. Work-count tests cover 1,000, 2,000 and 4,000 plain blocks
without relying on unstable CI timing thresholds.

Hidden renderer tests deliver actual untargeted trusted wheel input, including
a long document whose listener cancels scrolling. The test waits for a marker
set after listener installation: a parser can publish its first paint before
a trailing script runs, so static text is not proof that a listener is ready.
Timeouts and production scheduling are not relaxed to make the test pass.

## Timing method and limits

The 2026-09-30 diagnostics use the preserved merged before-build and the new
release build on the same machine, serially without competing builds/tests.
Breeze runs use fresh hidden profiles, `en-US`, 1280×720 outer windows, 125% scale,
and recorded 1248.8×548 CSS-pixel content viewports. Each fixture receives eight
ordinary `(600,300,+600)` wheels, starting 12 seconds after its own readiness
point and then 1,000 ms apart. A second pair reverses before/after run order.
All decisions, missing values and paint outcomes remain in the raw reports;
cancelled or unacknowledged input is not assigned zero latency.

The owned fixtures isolate these paths:

- **First block, 1k/2k/4k:** the first block spans 6,000 CSS pixels; later
  offscreen candidates precede it in reverse paint order. All eight wheels
  remain inside that same target.
- **Last block, 4k:** overlapping blocks put the last candidate first in the
  scan. The former DOM lookup still walks to that late owned node.
- **No block hit, 4k:** blocks are narrower than the wheel's x coordinate;
  selection reaches the body/root fallback. This is not a missing DOM target.
- **Inert nodes, 4k:** one painted block precedes 4,000 unpainted template nodes.
  The old early lookup can be cheap; the full index has extra work. This
  control must be reported, not filtered out of an improvement claim.

First-input times are retained separately from the median of inputs 2–8.
These small diagnostic pairs are not a statistical browser benchmark. Live
HTML5test additionally includes startup script/DOM/layout work and uses a
different document shape. Chrome's compositor-frame receipt and Breeze's
hidden retained-motion paint are different endpoints, so their numbers do
not establish a browser speed ratio or on-screen input-to-display latency.

### Owned-fixture results

Values below are medians of inputs 2–8 pooled across both pairs (14 inputs per
build/case), in milliseconds. Dispatch is renderer-local; paint is measured
from browser enqueue to the first attributed hidden retained-motion paint.

| Fixture | Dispatch before | Dispatch after | Motion paint before | Motion paint after |
| --- | ---: | ---: | ---: | ---: |
| First block, 1k | 3.688 | 0.540 | 4.837 | 1.498 |
| First block, 2k | 11.854 | 0.858 | 13.099 | 1.930 |
| First block, 4k | 49.602 | 1.364 | 50.826 | 2.376 |
| Last block, 4k | 0.370 | 0.736 | 1.500 | 1.747 |
| No block hit, 4k | 50.024 | 1.398 | 51.290 | 2.517 |
| Inert nodes, 4k | 0.282 | 0.793 | 1.387 | 1.892 |

All 192 inputs were acknowledged as viewport wheels and produced motion paint;
there were no omitted inputs or unmatched acknowledgements. The first-input
paint range was 7.172–57.367 ms before and 7.294–8.487 ms after; those samples
are not silently discarded from the raw evidence. The repeated large-document
scans improve substantially, while already-cheap late/inert targets pay about
0.4–0.5 ms of extra dispatch for constructing the index. This is a measured
tradeoff, not a claim that every hit becomes faster.

### Live HTML5test results

Two serial pairs per phase, with the second pair's build order reversed, sent
eight alternating-direction wheels each. Initial delays were 1,000 ms (early)
and 12,000 ms (settled), followed by 1,000 ms spacing. All 64 Breeze inputs
were viewport/painted, without omitted inputs, unmatched acknowledgements,
JavaScript errors, renderer exits or launch errors.

This table includes **all 16 inputs per build/phase**, including the first
wheel. Values are milliseconds and use the same endpoints as the fixture table.

| HTML5test phase | Dispatch median before / after | Motion-paint median before / after | Motion-paint maximum before / after |
| --- | ---: | ---: | ---: |
| Early | 45.389 / 1.712 | 49.321 / 5.668 | 171.183 / 150.911 |
| Settled | 45.352 / 1.638 | 49.203 / 5.467 | 55.613 / 11.186 |

The two first-wheel paints were 171.183/159.471 ms before and
150.911/149.123 ms after in the early phase; settled values were
55.454/55.613 ms before and 11.070/11.186 ms after. The index removes the
repeated-dispatch cost, but does not eliminate early startup contention:
the first after input spends only about 1.2 ms in dispatch while its
enqueue-to-paint interval still approaches 150 ms. This is not an assertion
that all interactive latency or issue #205's score-visibility acceptance is fixed.

The default-identity score remained 487/588 in three separate no-wheel
fresh-profile captures. A 250 ms filmstrip showed the after score absent at
2,250.122 ms and present at 2,500.067 ms; the preceding batch's single filmstrip
bound was (1,750.373, 2,000.537] ms. These small live-network samples do not
establish a score-completion speed improvement or regression. Score visibility
is distinct from page-ready and ordinary wheel response.

A separate fresh unified-headless Chrome 154.0.8037.92 capture rendered
579/588. Its sampled filmstrip had no score at 1,512.939 ms and a score at
1,750.625 ms (host capture times; compositor source frames were older).
The requested 1249×548 CSS viewport rounded to 1250×548 at 125% scale.
This single cold diagnostic is not issue #205's required five-cold/five-warm
navigation-to-visible-score comparison, nor a controlled before/after result.

### Chromium wheel reference

The reference observer initially read verdicts immediately after a compositor
frame and could see zero through seven listener events for eight inputs: native
motion preceded main-thread listener delivery. Missing values stayed null.
The corrected [observer contract](wheel-input-measurement.md) waits within one
absolute budget for the exact event verdict without changing the earlier frame
timestamp. Twenty-nine new pure checks and six hidden trusted-event fixtures
cover admission, deadlines, cancellation, missing evidence, nesting and retirement.
Earlier incomplete reports are retained, not turned into zero-latency samples.

One fresh Chrome run per phase used the same eight requested coordinates/deltas,
125% scale, `en-US`, and 1,000 ms spacing. All 16 inputs had observed uncancelled
viewport verdicts and direction-consistent frames, with no omitted/unattempted
inputs or capture/cleanup errors. Values below include each first wheel.

| Chromium phase | CDP reply median | Compositor frame-receipt median | First / maximum frame receipt |
| --- | ---: | ---: | ---: |
| Early | 1.097 ms | 12.590 ms | 38.247 ms |
| Settled | 0.854 ms | 12.185 ms | 14.656 ms |

Chrome's own load readiness was 1,221.855/1,204.460 ms; Breeze's readiness is
first owned layout/paint, so the early starts are not navigation-synchronized.
These are independently labeled endpoints, not evidence of Breeze/Chrome speed
parity or a ratio: Chrome frame receipt includes compositor capture and delivery,
while Breeze reports hidden retained paint. No display scanout is measured.

## Interrupting native wheel animation

An allowed wheel default action in the opposite direction cancels unfinished
native viewport travel and its fractional input residues. The new target starts
at the currently painted position, not the previous animation's future target.
Cancellation happens before pixel/notch quantization, so even a subpixel reverse
gesture stops old travel; subsequent fractional input still accumulates normally.
Same-direction input continues to accumulate, zero deltas remain no-ops, and
document boundaries still clamp motion.

Renderer output compaction preserves opposite nonzero wheel directions as
separate ordered reports rather than netting their distances. This interrupts
animation, not DOM event delivery: trusted listeners, `preventDefault()`, nested
scrolling and authoritative absolute scroll requests retain their normal rules.
Unit tests cover both directions, long backlogs, alternating gestures, fractional
input, edge clamping, and the production first-frame response curve. Benchmark
traces retain actual admission/first-paint viewport positions; missing evidence
is null, never inferred from an acknowledgement or requested delta.

Eight fresh hidden release runs used 1280×720 at 125% scale, `en-US`, an initial
2,000 CSS-pixel scroll, eight 126 CSS-pixel wheel inputs in one direction, then
one opposite input. Both directions and 15/30 ms spacing were tested on an owned
long page and live HTML5test. All 72 inputs retained their verdicts, without
omissions, unmatched acknowledgements, JavaScript errors or renderer exits.
Every reverse input's first owning paint moved 30.4 CSS pixels in the new
direction: 0.843–1.135 ms enqueue-to-paint on the owned page, and
4.036–24.681 ms on HTML5test. These are small diagnostic samples of hidden
retained paint, not monitor scanout or a Chrome speed comparison. The earlier
hit-index before/after timings above were measured before this reversal follow-up.

## References

- [DOM event dispatch](https://dom.spec.whatwg.org/#dispatching-events)
- [UI Events wheel behavior](https://www.w3.org/TR/uievents/#events-wheelevents)
- [CSSOM View scrolling](https://drafts.csswg.org/cssom-view/#scrolling)
