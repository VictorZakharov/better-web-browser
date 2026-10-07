# SVG text rendering

Breeze uses the existing resvg/usvg backend for SVG glyph shaping, outlines,
paint and text placement. The text feature is enabled; this is not a replacement
SVG engine or a browser-identification-specific path.

## Fonts and publication

On Windows, SVG family selection uses the same lazy Fontique catalog as Canvas
and document text. Loaded web fonts retain their CSS family aliases rather than
being selected by their internal OpenType family names. Font bytes are shared
with fontdb, preserving collection indices. Selection does not scan directories,
fetch URLs, or read arbitrary author-provided file paths.

Inline SVG serialization supplies computed font family, size, weight, style,
letter spacing and word spacing for text elements. Definitions are not frozen
to the original parent's inherited font: a `use` instance retains inheritance
from its referencing element. External-document computed fill/stroke rules are
not generally serialized by this change.

A page refresh shares an immutable font snapshot across its SVGs. Text raster
cache stamps include font contents and matching descriptors; shape-only SVGs do
not invalidate when an unrelated font loads. Renderer integration tests assert
actual Ahem outline pixels after font publication and after font-size mutation,
not just the existence of a text API or a successful network response.

Renderer-owned SVG and Canvas rasters share the acknowledged image-update queue.
A changed SVG reuses its stable key but is sent again; unchanged refreshes do not
clear a deferred update. Removing or rejecting an SVG retires its old raster.

## Element-owned text lengths

The pinned usvg source has a focused correction for nested `textLength` ranges.
Ranges are recorded in rendered-character coordinates, resolved child first,
and applied across shaped paint spans. An already-resolved child is an atomic
spacing unit for its ancestor. Glyph scaling changes advances and outlines once;
absolute placement and anchoring follow length adjustment.

The contract follows the [SVG 2 text layout algorithm](https://www.w3.org/TR/SVG2/text.html#TextLayoutAlgorithm).
Exact Ahem pixel tests cover paint-span boundaries, nested ranges, spacing versus
glyph scaling, anchors, zero lengths and invalid child values. The unchanged
Arial regression also checks nested glyph scaling and translation together.
Provenance, licenses and the precise upstream patch inventory are recorded in
`vendor/usvg/BREEZE_PATCHES.md`. Imported upstream code is not counted toward
the batch's original-work line target.

## Admission and remaining limits

The bounded decoder admits at most 64 XML element levels, 16,384 text Unicode
scalars and 256 text/tspan length adjustments. XML tokenization checks depth
before recursive tree construction. Expanded text is counted after parsing;
markup-bearing internal entities are rejected, while ordinary DOCTYPEs and
text-only entities remain supported. Existing source, node and image budgets
still apply. Font loading admits at most 256 faces and the existing aggregate
font-byte budget, including collection face counts before fontdb allocation.

These are operational limits, not claims of complete SVG 2 conformance.
Upstream text-path, baseline, bidi and shaping limitations still apply. Composite
CSS unicode-range selection and custom font-feature descriptors are not fully
bridged into usvg shaping. Complex text-length ranges combining relative shifts
and multiple absolutely positioned chunks need additional reference coverage.
SVG DOM geometry methods such as `getBBox` are not implemented by this raster
slice; no placeholder geometry is exposed to claim support.

Resolved descendants keep their explicit length under parent glyph scaling as
well as spacing. A parent wholly composed of resolved descendants, or requesting
less length than their fixed extent, does not rescale or reflect those glyphs.
This overspecified-parent boundary is explicit, not full SVG layout conformance.

The Chrome 154 reference preserves the original Arial direct/nested placement,
but is not byte-identical on the Ahem fixtures. Fractional glyph-edge seams differ;
its parent spacing distributes a gap within a resolved child, and zero
`textLength` retains natural glyphs. Breeze follows the documented SVG 2
child-ownership/zero-length contracts rather than claiming Chrome pixel parity.
