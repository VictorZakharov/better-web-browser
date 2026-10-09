# Defer non-rendered inline SVG work

Live documents and child documents prepare inline SVG rasters after the
presentation cascade, not while constructing a `Page` from an incomplete DOM.
This avoids decoding once with default styles and again after the actual
stylesheets, inherited colors, fonts and viewport are known. The standalone
`Page::parse` APIs preserve their eager image-metadata contract for immutable
layout callers.

Resource refresh skips serialization, hashing and rasterization only when the
computed styles prove that the SVG or an ancestor has `display: none`. Missing
styles cannot prove suppression. Zero size, offscreen placement, opacity and
`visibility: hidden` do not qualify: descendants can override visibility, and
those other properties do not remove the subtree from the rendering model.
See [SVG 2 rendering](https://www.w3.org/TR/SVG2/render.html#Rendered-vs-NonRendered).

The proof uses composed ancestry, including assigned slots and shadow hosts.
Fullscreen/popover descendants instead use shadow-including ancestry, because
a top-layer element is not clipped or suppressed by a hidden assigned slot.
Actual `display: none` ancestors still suppress it. See
[CSS top-layer styling](https://drafts.csswg.org/css-position-4/#top-layer-styling).

Deferral never removes DOM nodes or reference targets. A visible SVG can still
use a shape, filter or other definition inside a hidden sibling SVG in the
same tree. Hidden mutations do not install a new raster version. Reveal checks
the complete current decoder input, including references, inherited colors and
font inputs, before publishing replacement pixels. Detached SVGs still retire
their raster, version and publication state.

Tests cover first presentation, hidden mutations and reveal, changed stylesheets,
shared definitions, closed shadow roots, slot assignment, fullscreen, bounded
invalid source, resource removal, and child-document viewport changes. Font
usage collection likewise does not hydrate styles solely to discard a subtree
whose ancestor is already known to be `display: none`.

This is a work-avoidance optimization, not an assertion that every SVG is cheap
or that hidden content never needs resources. SVG source/raster admission limits
and the existing number-of-roots cap remain unchanged. Performance comparisons
must include reveal and final pixels, so deferring necessary work cannot be
mistaken for completing it faster.
