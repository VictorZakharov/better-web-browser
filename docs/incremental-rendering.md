# Incremental rendering

Breeze aims to settle a large document, such as the Wikipedia article in
milestone 6 (#271), within about a second of navigation, as Chrome does. A
document keeps changing after its first frame: stylesheets, images and fonts
arrive, and scripts insert content. Each frame must cost in proportion to what
changed, not to the size of the document. This document describes the state each
rendering stage keeps between frames and how a stage finds what changed. Sections
are added as the stages are converted; #273 tracks the complete design.

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

## Not yet retained

Block, inline, flex, grid and table layout replay unchanged subtrees
(`subtree_replay`) but walk the whole tree each pass (#275, #276). Inline
geometry, paint order, hit-testing eligibility and sticky positions are
recomputed for the whole document (#277), and the display list is rebuilt (#278).
