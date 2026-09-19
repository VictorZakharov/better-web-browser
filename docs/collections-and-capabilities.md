# Live collections and honest style capabilities

## Indexed DOM iteration

`NamedNodeMap`, `HTMLCollection`, and `DOMTokenList` use the indexed-getter
iteration algorithms required by [Web IDL](https://webidl.spec.whatwg.org/#define-the-iteration-methods).
Their `Symbol.iterator` is the actual `Array.prototype.values` intrinsic, not a
generator over an array snapshot. Each `next()` reads the current length and
index; deletion, replacement, and growth therefore remain visible until the
iterator has finished. An exhausted iterator stays finished.

`DOMTokenList` also exposes the Array `entries`, `keys`, `values`, and `forEach`
intrinsics required by its value-iterable declaration. `forEach` captures the
initial length but reads each visited value at the time of the callback. The
shared installer preserves Web IDL's different property descriptors for the
symbol method and named iterable methods.

This fixes consumers that iterate `element.attributes`, including Wikipedia's
observed exception. Focused mutation regressions and three unchanged upstream
WPT iterator files cover this contract. This is not a claim that all Web IDL
legacy-platform-object behavior is implemented.

## CSSStyleDeclaration capability exposure

Inline and rule declarations share named-property lookup. Only implemented CSS
property names expose string-valued attributes; unknown names return `undefined`
and are absent from `in`. Camel-case, dashed, `cssFloat`, and implemented WebKit
aliases resolve through the engine's property inventory. Unknown strings and
symbols retain ordinary JavaScript expando behavior without changing CSS text.
The distinction is required by
[CSSOM's supported CSS properties](https://drafts.csswg.org/cssom/#the-cssstyledeclaration-interface).

Previously every unknown property returned an empty string. Feature detectors
therefore selected unsupported paths, including off-canvas menus implemented
with 3D transforms. Breeze currently implements a 2D translation subset, not a
3D transform renderer. `perspective`, `transform-style`, and `filter` must not
advertise full support merely because the parser recognizes their containing-
block effects. Named-property exposure and `CSS.supports` now agree on that
boundary. See the [CSS Transforms two-dimensional subset](https://drafts.csswg.org/css-transforms-2/#two-dimensional-subset).

No URL, hostname, selector, or third-party stylesheet rewrite participates in
this decision. DuckDuckGo can select its own authored positioning fallback.
The original cross-browser collection/capability fixture checks closed, open,
and closed-again menu geometry without copying site code. Broader declaration
parsing, serialization, and full transform/filter rendering remain separate work.

## Render blocking and explicit geometry

While the document's existing render-blocking gate is closed, the renderer keeps
its CSSOM snapshot synchronized but defers construction of a paintable display
list. Producing and shaping a list that cannot be presented is unnecessary.
The dirty state survives until the gate opens, including a wakeup without a new
DOM mutation. Explicit CSSOM geometry reads still flush synchronously; async
scripts and renderer heartbeats still run. No stylesheet is ignored and no
unstyled presentation is deliberately exposed.

The hidden integration tests cover stylesheet completion, failure, removal,
imports, dynamic blockers, and synchronous geometry during blocking. The live
performance report separates first presentation from populated Appearance
controls; this scheduling change alone does not establish complete-page parity
with Chrome.
