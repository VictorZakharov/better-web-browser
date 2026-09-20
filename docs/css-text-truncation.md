# CSS text truncation

Breeze implements single-line `text-overflow` and the legacy
`-webkit-line-clamp` combination end to end, from parsing through layout,
paint and invalidation:

- `text-overflow: clip | ellipsis` (single keyword only).
- `-webkit-line-clamp: none | <positive integer>`.
- `-webkit-box-orient: horizontal | vertical` (`inline-axis`/`block-axis`
  parse as aliases and serialize to the canonical keywords).
- `display: -webkit-box` keeps laying out as ordinary block flow, but the
  authored value is recorded so the clamp can distinguish it from a plain
  block. A later `display: flex` still wins through the normal cascade.

## Activation

Single-line ellipsis needs `text-overflow: ellipsis`, a non-wrapping
`white-space` (`nowrap` or `pre`) and non-`visible` overflow, per CSS
Overflow 3 §2 (https://drafts.csswg.org/css-overflow-3/#text-overflow).
Chrome additionally requires the nowrap/overflow combination; content that
fits exactly, `clip`, or a box narrower than the marker itself paints
without a marker (the narrow case falls back to plain clipping, matching
Chrome 153).

Legacy clamping needs all three of the authored `-webkit-box` display, a
vertical orientation and a positive count, per CSS Overflow 4 legacy
compatibility (https://drafts.csswg.org/css-overflow-4/#legacy-compatibility)
and WHATWG Compatibility
(https://compat.spec.whatwg.org/#css-properties-and-values). Overflow does
not gate activation: the clamp itself suppresses later lines. The modern
unprefixed `line-clamp` shorthand, custom ellipsis strings, vertical writing
modes, multi-column fragmentation and editable-control ellipsis are
explicitly out of scope.

## Implementation decisions

- The marker is U+2026 shaped with the surrounding font, never three dots.
  Fitting walks grapheme boundaries (`unicode-segmentation`) and probes
  widths with `measure()` by binary search, so doubling input adds a
  logarithmic number of calls; `shape()`/`text_geometry()` run once on the
  final string.
- Truncation shortens paint only. Fragments, element boxes and Range mapping
  keep the authored prefix; the marker gains no source clusters, DOM strings
  never change, and geometry-only and painted layout agree.
- Marker ownership (font, color, link, node) follows the last kept text, so
  hovering or activating the marker behaves like the content it replaces;
  there is no dead click region.
- Clamp state is created fresh per block formatting context and threaded
  through that container's inline flushes only, so one container's budget can
  never truncate a sibling or a nested independent box (inline-block content
  is atomic: kept whole or dropped). A following sibling settles on the
  clamped box size.
- `text-overflow` swaps are paint-only in `layout_equivalent`; clamp,
  orientation and legacy-box changes trigger relayout. Paint-only flips still
  repaint through the regular layout path for rendered subtrees.

## Checks

- `cargo test --locked --lib css::tests::truncation` (cascade, CSS-wide
  keywords, serialization, `CSS.supports`, invalidation classification).
- `cargo test --locked --lib layout::tests::truncation` (single-line and
  clamp layout/paint/geometry/Unicode/complexity contracts).
- `cargo test --locked --lib layout::tests::truncation_dynamic` (live class,
  count, width and text updates through style refresh; paint-only flips).
- `tests/fixtures/text-truncation.html` with
  `scripts/test-text-truncation.ps1` for hidden Breeze vs headless Chrome
  comparison at matched content viewports.
