# CSSOM View point queries

Breeze implements `Document.elementFromPoint()` and `elementsFromPoint()` and the
corresponding `ShadowRoot` methods from the [CSSOM View draft](https://drafts.csswg.org/cssom-view/#dom-document-elementsfrompoint).
The implementation consumes the same layout paint order, clipping geometry,
and inherited hit-test eligibility used by native input dispatch. It does not
infer stacking from DOM order or special-case a site.

The script host requests a retained hit-test snapshot only when a point query
occurs. The snapshot contains element references, fragments, scroll boxes,
clip paths, and exclusion IDs; it does not duplicate glyphs or display items.
Mutations invalidate it with layout geometry. The JavaScript binding converts
coordinates and checks the viewport before crossing the host boundary. An
outer root sees a shadow host instead of internals, while a query on the
`ShadowRoot` can see its own descendants. A document query appends the root
element as specified.

`pointer-events: auto | none` is inherited and participates in both script
queries and native input hit testing. `visibility: hidden` is also excluded.
An eligible descendant can opt back in with `pointer-events: auto`.

Curated upstream CSSOM View tests cover element ordering, DOM and visibility
changes, coordinate conversion, inline fragments, and shadow-root retargeting.
Remaining standards work includes shape-aware SVG hit testing, image maps,
more complete pseudo-element/anonymous-box ordering, and 3D transformed
geometry. Additional upstream probes expose missing RTL inline placement,
positioned floats nested inside inline ancestors, and default table-cell
spacing/directional mapping. A specified-height table now allocates surplus
height to its rows rather than leaving empty cells at zero height, but that
does not by itself satisfy those broader table tests. Promote the remaining
upstream cases only when their underlying layout support lands.
