# Retained Canvas pixel checkpoints

Canvas pixels are a replaced paint source, not DOM or CSS box geometry. Breeze
can retain an existing display list for an explicitly pixel-only document task.
This applies equally to 2D and native WebGL presentation, not to particular sites.

The scheduler distinguishes a surface-pixel request from a full layout request.
Coalescing never downgrades full or unknown work, in either merge order. Runtime
outcomes additionally reject DOM mutations, invalidation, navigation, viewport
changes, fonts, selection, media and fullscreen actions. The renderer rejects
pending resources, parsing, frame layout and a dirty/blocked rendering checkpoint.

The renderer drains each Canvas snapshot once and verifies a connected existing
bitmap with unchanged attribute and natural dimensions and a retained image key.
First publication, resize, disconnected/reconnected nodes, missing pixels or an
unknown retained image fall back to normal style/layout. Consumed valid pixels
are installed even on that fallback, so the first or resized frame is not lost.

Stable snapshots still pass through normal observers, image updates, presentation
and metadata. This does not disable author script or change requestAnimationFrame
ordering. Pure native framebuffer work may produce no onscreen bitmap update;
that cannot invalidate an otherwise stable display list.

Focused hidden-process tests assert new pixels plus unchanged geometry and cover
CSS margin, text, resize, first publication and retired image keys. Broader Canvas
integration tests cover removal/reattachment, media and native WebGL export.

The `Canvas pixel checkpoint reused retained style and layout` diagnostic is
enabled only by requested selectors. The historical `full_layout_rebuilds`
benchmark counter counts render-requested presentations, not actual layout
executions; it must not be used to infer how many passes this path eliminated.
Use checkpoint diagnostics and measured phase times instead.

This is intentionally narrow. A game that changes DOM/CSS every frame still
requires the appropriate style, geometry and paint work. Successful fixture tests
are not evidence of a game startup speedup. Optimize with small command-level
workloads that check actual results, and keep the unchanged application's
rendering as a separate functionality check.

Compatibility basis: [HTML Canvas](https://html.spec.whatwg.org/multipage/canvas.html#the-canvas-element).

## Exact unchanged inline and animation styles

An inline `style` mutation or native animation/transition overlay still runs the
cascade, including generated pseudo content. If the resulting computed styles
and generated boxes are exactly
unchanged, a settled document can reuse its existing layout and paint list and
publish any new same-size Canvas pixels. This is not a paint-equivalence test:
color, opacity, transforms, visibility, fonts and image changes all use the
ordinary resource/layout path. A failed comparison keeps the first cascade's
change evidence; it does not run a second comparison against the new values.

The optimization requires explicit style-only mutation evidence, not a general
control-state invalidation. Mixed text,
tree, state, resize, stylesheet, resource and viewport changes cannot qualify.
SVG, fullscreen and shadow/slot ancestry also retain the general path. Unknown
roots, first Canvas exports and changed Canvas natural sizes fall back normally.

Relational selectors can observe nodes outside a dirty subtree. The compiled
rule set records nonlocal dependencies once, including `:has()` nested inside
functional and nth-child selector lists. Unknown or mixed mutations still refresh
the whole composed tree when these dependencies exist. Inline/overlay declaration
writes cannot change class, tree or control-state membership; they widen only
when a relative selector can inspect the `style` attribute (including nested
functional and nth-child filters). Local equality therefore cannot conceal a
changed ancestor or sibling match, without forcing a document-wide cascade for
an unrelated `:has(.class)` on every animation frame. Shadow/part and scoped sheets
do not qualify for this shortcut and keep their existing refresh policy. See
[Selectors Level 4 §4.5](https://drafts.csswg.org/selectors/#relational).

Page unit tests compare nonlocal refreshes with fresh cascades. Hidden renderer
tests cover equal CSS serialization with new pixels, genuine paint/geometry
changes, relational ancestor changes, mixed text changes and first exports.
