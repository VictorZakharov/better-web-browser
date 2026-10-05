# SVG resource boundary

SVGs continue to use the locked `resvg`/`usvg` 0.48.1 renderer (MIT OR Apache-2.0),
not a browser-owned SVG rasterizer. Its `raster-images` feature reuses the existing
locked GIF, WebP and JPEG packages; enabling it changes dependency edges, not
package versions or the set of shipped crates. Raster data images previously
parsed but did not draw because this build feature was disabled.

The default upstream string resolver is not suitable for a browser: it treats
image URLs as local filesystem paths. Breeze disables it for page and Canvas
SVG decoding. SVG resource URLs must be mediated by the browser resource broker;
this change does not implement remote SVG image fetching. A regression creates
an owned local image, proves upstream's default resolver can load it, and proves
the browser decoder cannot.

Embedded data images share an image-count and decoded-byte budget. Raster
dimensions are checked before decoding. Actual decoding and color conversion
reuse the existing bounded `image_decode` boundary, then normalize to bounded PNG
for the renderer. This prevents a second unbounded codec path, including hostile
GIF frame rectangles that do not match the logical image dimensions.

Nested SVG data images are limited to 16 levels and 64 image-resolution attempts;
their dimensions count toward the shared decoded-byte budget. SVGZ decompression
uses the already locked MIT/Apache-2.0 `flate2` implementation, with a 4 MiB
expanded-source ceiling before parsing. Invalid or over-budget resources are
omitted rather than acquiring filesystem authority or unlimited allocations.

These are integration restrictions and real decoding behavior, not HTML5test
feature-detection special cases. References:
[SVG secure processing modes](https://svgwg.org/svg2-draft/conform.html),
[SVG resource linking](https://svgwg.org/svg2-draft/linking.html), and
[the upstream image resolver](https://github.com/linebender/resvg/blob/v0.48.1/crates/usvg/src/parser/image.rs).

## Inline shared definitions

An inline SVG raster includes the dependency closure of fragment-only rendering
references from its own DOM tree, including definitions in sibling SVGs. The
first element with an ID wins; a matching non-SVG element is not skipped in favor
of a later SVG duplicate. Shadow trees are separate lookup scopes. Navigation
anchors and external URLs do not acquire decoder-side fetching authority.

CSS URL tokenization reuses the existing `cssparser` dependency. XLink attribute
namespaces survive serialization, and unprefixed SVG2 `href` takes precedence.
Imported definitions keep their author attributes without freezing inherited
`currentColor` from the definition's original parent onto a use instance.
Raster cache keys include imported definitions, so changes outside the local SVG
invalidate its pixels while compositor-only animation still reuses the raster.
Fragment percent-decoding shares HTML navigation's byte decoder (without form
decoding: `+` remains literal). CSS escapes are resolved by `cssparser`, and the
isolated decoder receives normalized URLs, using that same parser's string writer
when quoting is necessary. Ordinary fragment IDs keep the unquoted form accepted
by upstream's filter parser.

Serialization checks the 4 MiB byte ceiling while XML-escaping, with a 256-level
depth ceiling and 100,000-node budget. Reference discovery limits distinct IDs
and queue bytes; use expansion has independent node and pending-work budgets.
Cyclic use edges are omitted before entering upstream's parser. A small acyclic
but exponentially expanding use graph is also rejected before materialization.

This is not complete SVG styling support: document stylesheet rules affecting
imported geometry, remote use resources,
and SVG text remain separate work. The renderer's text feature is still disabled;
importing a game's text-only logo definitions does not claim that logo is drawn.
See [SVG use instances](https://svgwg.org/svg2-draft/struct.html#UseElement) and
[SVG URL processing](https://svgwg.org/svg2-draft/linking.html#URLReference).
