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

Overflow ellipsis needs `text-overflow: ellipsis`, non-`visible` overflow,
and inline content extending past the line's end edge, per
[CSS Overflow 3](https://drafts.csswg.org/css-overflow-3/#text-overflow).
An unbreakable word can overflow with `white-space: normal` too; `nowrap`
is not required. Content that fits exactly, `clip`, or a box narrower than
the marker itself paints
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

- The marker is U+2026 shaped with the block container's font and color.
  Fitting walks grapheme boundaries (`unicode-segmentation`) and probes
  widths with `measure()` by binary search, so doubling input adds a
  logarithmic number of fitting calls. Source geometry uses the original run;
  raster shaping uses the visible prefix.
- Overflow ellipsis shortens paint only. Fragments, element boxes, Range
  offsets and scroll extents retain the original text, including the hidden
  suffix. Nested inline wrappers retain their box sizes and share one marker
  budget. The synthetic marker gains no source clusters.
- Marker visual styling is independent from its link/node ownership; a
  differently styled nested span must not change the marker's font or color.
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
- The same runner with `-Contracts` uses `text-truncation-contracts.html`.
  It asserts original per-character Range positions and scroll widths against
  a clipped control, includes the leading astral-character crash regression,
  and captures nested wrappers, normal whitespace, and mixed marker styling.
  A missing success marker or JavaScript error fails the run.

## Review regression verification (2026-09-20)

The original implementation passed its unit tests but failed an independent
Chrome comparison. The corrected paint-only path is covered by six regression
tests, replacing the old empty-prefix Unicode assertion:

| Contract | Before correction | After correction |
|---|---|---|
| Leading astral character, truncated suffix | UTF-8 slicing panic | No renderer error |
| First visible character's Range (owned 20px monospace fixture) | Empty rectangle | Same 21px x / 11px width as Chrome |
| Original 165px text scroll extent in a 100px box | Reduced to 100px | Preserved at 165px |
| Two nested inline wrappers | Two extra characters hidden | Same truncation point as plain text |
| Long word with normal whitespace | No ellipsis | Ellipsis rendered |
| Blue 30px run in a red 20px block | Blue 30px marker | Red 20px block-styled marker |

The release build passed both owned captures against headless Chrome 153 and
the complete 16-fixture deterministic alpha gate (one iteration each).
Local checks passed: 1,270 library tests, 118 browser-shell tests, 130 renderer
tests and 82 live-runtime tests; existing ignored tests remain ignored.
These are targeted contracts, not a claim of full rendering parity: font
fallback for the mathematical-script glyph still differs from Chrome.
