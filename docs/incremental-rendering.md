# Incremental rendering

Breeze aims to settle a large document, such as the Wikipedia article in
milestone 6 (#271), within about a second of navigation, as Chrome does. A
document keeps changing after its first frame: stylesheets, images and fonts
arrive, and scripts insert content. Each frame must cost in proportion to what
changed, not to the size of the document. This document describes the state each
rendering stage keeps between frames and how a stage finds what changed, for the
stages already converted and as the design for the rest. Each child issue of #271
implements one part and is reviewed against its section here.

## Stage budgets

The article has about 24k elements and 230 KB of CSS. The budgets add up to a
settled page within about a second; every stage is designed in its final form for
them, so no stage needs to be rebuilt for a later milestone.

| Stage | Measured before #271 | Budget |
|---|---|---|
| Full-document style | 500-800 ms | <= 50 ms |
| Full-document layout and display list | 300-400 ms | <= 50 ms |
| Frame for a small DOM or class change | 85-100 ms | < 5 ms |
| Script and DOM-binding work during load | ~1 s | <= 300 ms |
| First styled frame | 1.0 s | <= 0.4 s |

A stage whose cost follows the size of the document rather than the change fails
review even when it is fast today; the counters below make that visible.

## Measurement

Every layout pass adds to `Page::layout_stats` (`LayoutStats`,
`src/engine/layout/engine/stats.rs`): passes, box-tree nodes rebuilt, block
layouts computed and subtrees replayed, display items emitted and replayed, and the
time of each phase (box tree, block layout, inline geometry, scrollable overflow,
paint order, hit testing, sticky positions). The renderer reports the sum with
each update in `PageLoadReport::layout`. The hidden benchmark writes the run's
total to `layout_work` in `run.json`, and each `renderer_runtime_timeline` sample
carries the cumulative pass, rebuilt-box, laid-out-block and emitted-item counts,
so a frame whose counts jump with the document size stands out.

`layout_workload` in `src/engine/layout/engine/stats/workload.rs` is an offline
page of the article's shape (generated text, a three-column grid, sticky side
menus, floats, tables and many links). Its ignored test prints each pass's
`LayoutStats` for a full layout and for the small changes that dominate a
settling page:

```
cargo test --release --lib layout_workload -- --ignored --nocapture
```

Settle times compare both browsers by one rule. The hidden benchmark
(`scripts/run-hidden-benchmark.ps1 -FilmstripDirectory`) and the headless
Chromium harness (`benchmarks/chromium --filmstrip-directory`) write the same
filmstrip layout, and `dotnet run --project benchmarks/chromium -- --analyze-filmstrip
<directory> [--content x,y,width,height] [--region x,y,width,height]` reports the
first frame whose page area is not one uniform color, the frame from which a region
(such as an infobox logo) stops changing, and the last frame whose page area
changed. Breeze frames include its toolbar and status bar, which change while
loading, so pass `--content` with the page area for them; Chromium's headless
frames are all page. Timings are reported, never used as pass/fail gates.

## Change stamps

Stages recognize unchanged work by comparing stamps recorded when the work was
done with their current values. Nothing pushes dirty bits into a later stage.
Reading a stamp is cheap, and a stage that does not run in a frame keeps its
records valid.

| Stamp | Owner | Advances when |
|---|---|---|
| `Node::subtree_mutation_version` | DOM | the node or a shadow-including descendant changes: child lists, attributes, character data, control state, focus or popover state |
| `Node::child_list_version` | DOM | the node's own children change |
| `StyleSet::own_layout_generation` | style | the node's computed style or generated content changes |
| `StyleSet::subtree_layout_generation` | style | the same holds for the node or a composed descendant |

Style generations are drawn from one process-wide counter, so a value recorded
against one `StyleSet` never matches another one. A full restyle, or styles
computed for a different viewport, therefore invalidates every record.

A shadow-tree node's composed subtree can include slotted light DOM whose
mutations do not reach the shadow-tree node's version. Stages never skip work for
nodes inside shadow trees on the strength of their stamps.

## Box tree

Retained in `Page::layout_box_tree`; implemented in
`src/engine/layout/engine/box_tree/retained.rs`.

The box tree applies `display: none` and `display: contents`, inserts generated
`::before` and `::after` boxes, takes open popovers out of their parent, drops
insignificant whitespace between table parts and wraps misparented table parts
in anonymous boxes (CSS 2.2 §17.2.1). Its children for a node depend only on the
node's composed subtree: its DOM, including popover state and whitespace text,
and its computed styles. Anonymous boxes inherit the style of their parent box.

For each node whose children it built, the retained tree records the node's
subtree mutation version and subtree layout generation. An update walks down from
the layout root:

- A node whose recorded stamps equal its current ones keeps its whole subtree
  without visiting it.
- Any other node rebuilds its own children and records its stamps. Its unchanged
  children are then skipped in turn, so a change rebuilds the path from the root
  to the changed node and the changed subtree.
- Generated pseudo-elements, nodes inside shadow trees and unstyled nodes record
  no stamps and are rebuilt whenever they are reached.

A rebuilt node releases its previous children. Its anonymous boxes go to a pool
keyed by parent and first wrapped child, and a box of the same display is reused
from the pool, so layout results that name an anonymous box keep naming the same
box. A previous child that the node does not place again is forgotten together
with the boxes it owns, unless another node has placed it since. Every retained
node records its owning box for this purpose; a moved node therefore survives
whichever of its old and new parents is rebuilt first.

Zoom scales the styles anonymous boxes inherit without a stamp of its own, so a
tree under zoom, or the first update after zoom is removed, rebuilds every node.
A document layout whose root changes, for example when an element enters
fullscreen, starts a new tree. A fixed-layer layout of a subtree updates the
retained tree when the tree contains its root; otherwise it builds a tree for
that pass only, whose anonymous boxes later layouts must not retain.

On the saved article, a layout pass spent 23-27 ms building the box tree. The
retained tree updates in about 0.2 ms when nothing changed and rebuilds only the
changed path after a small change. A seeded mutation fuzz compares the retained
tree with a fresh build after every step: insertions, removals, moves, display
and table-role changes, generated content, whitespace text, slot assignment,
popovers and zoom.

Blink and Gecko also keep their layout trees across frames. Style recalculation
marks the elements whose boxes must be rebuilt (Blink's layout-tree reattach,
Gecko's frame reconstruction hint), and DOM insertions and removals construct or
destroy boxes for the affected children. Breeze reaches the same result by
comparing stamps, so the box tree needs no notifications from the DOM or the
cascade and cannot miss one.

## Post-layout passes

Retained in `Page::layout_index`; implemented in
`src/engine/layout/engine/post_layout_index.rs`.

After block layout, three passes act on a few kinds of nodes: inline geometry
gives a plain inline element without boxes of its own the union of its children's
bounds, hit testing excludes nodes whose `pointer-events` or `visibility` disable
them and the unstyled nodes below them, and sticky positioning builds a layer for
each `position: sticky` box. Each pass used to walk the whole document. Which
nodes qualify depends only on DOM and computed styles, so the index keeps, for
each subtree, its root when it qualifies and the entries of the children that
contain qualifying nodes, stamped like the box tree. A pass rebuilds the entries
on the path to a change, then reads the qualifying nodes by following entries that
contain some, skipping every subtree that contains none. Small subtrees and
unstyled leaves are not retained; walking them again with their parent is cheaper.

The geometry and sticky passes follow the composed tree and hit testing the
shadow-including tree. A shadow host's stamps cover both of its trees: shadow-tree
mutations advance its mutation version, and every node below it has a
composed-parent chain through it that style generations follow. Shadow-tree nodes
themselves are rebuilt whenever reached, as in the box tree.

On the saved article, a class change on one table-of-contents entry spent 2.8 ms
on hit-test eligibility, 2.2 ms on sticky boxes and 5.7 ms on inline geometry;
with the index these take about 0.1, 0.1 and 3.7 ms. Inline geometry still visits
every inline element of the document, because the bounds it unions come from this
pass's layout; it becomes proportional to the change with retained layout results
(#275). Tests compare the index with the
whole-document walks after insertions, removals, moves, class, display and
visibility changes and slotted content.

## Planned stages

These follow the RenderingNG/LayoutNG and Stylo designs where they fit Breeze's
single renderer thread and process isolation.

- **Layout results (#275, #276).** Each box keeps its last layout result with the
  constraint space it used (available sizes, percentage bases, floats), as
  LayoutNG's cached layout result. A pass lays out only boxes whose style,
  children or constraints changed, and stops propagating up the containing-block
  chain where a box's size, baseline and float exclusions are unchanged; unchanged
  siblings move by offset. This replaces `subtree_replay`, which copies every
  retained subtree's items into each pass. The result also carries the box's
  scrollable overflow, so the document overflow walk (about 7 ms per pass on the
  article) becomes a read, as LayoutNG's layout overflow does. Inline formatting
  contexts reuse line boxes before the first damaged inline item; flex and grid
  reuse item layouts whose constraints are unchanged; tables reuse rows whose
  content and column widths are unchanged.
- **Display list (#278).** Display items are retained in chunks per stacking
  context and paint phase, like Blink's paint chunks. A frame re-records damaged
  chunks and re-sorts only stacking contexts whose children or z-order changed;
  hit-test order follows the chunks. The presentation already sends edits against
  the previous display list.
- **Style invalidation (#279).** Invalidation sets per class, id and attribute
  (self, descendant and sibling sets, keyless subjects through their nearest keyed
  compound) restyle the elements a changed token can match. A set that cannot be
  bounded falls back to the subtree, with the reason counted.
- **Style computation (#280, #281, #282).** Computed styles are split into shared,
  reference-counted groups (inherited, non-inherited, rare) as Gecko's style
  structs, so inheriting or sharing a group is a pointer copy and unchanged groups
  compare by pointer. A style-sharing cache serves siblings with equal matching
  inputs under Blink's and Gecko's safety rules. Independent subtrees then restyle
  in parallel.
- **Loading (#283, #285).** Requests are prioritized by destination and viewport
  as Chromium's fetch priorities are, ready scripts run between frames, and the
  first frame is produced from the first parsed screenful.

## Full passes by design

Some changes invalidate every record, and their passes stay full: the first layout
of a document, a viewport, zoom or style-viewport change (new style generations),
a measurer epoch change after a web font arrives, and a new layout root such as
fullscreen. Their cost is bounded by the full-document budgets above rather than
by the change, and each happens a few times per load. The retained records stay
valid for the next small change, and the pass after a full one replays instead of
repeating it.
