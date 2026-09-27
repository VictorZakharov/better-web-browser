# Declarative Shadow DOM parser contract

Navigation HTML, network-streamed HTML, `document.write()`, and
`Document.parseHTMLUnsafe()` opt in to the
[HTML parser's declarative shadow-root algorithm](https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inhead).
A `<template shadowrootmode="open|closed">` is consumed when its parent is a
[valid shadow host](https://dom.spec.whatwg.org/#dom-element-attachshadow) with
no existing root. The parser attaches a shadow root before consuming the
template's children, and constructs those children directly in that root.
`shadowrootdelegatesfocus`, `shadowrootclonable`, and
`shadowrootserializable` are boolean attributes. `shadowrootslotassignment`
selects manual assignment only for its `manual` state; otherwise it is named.
`shadowrootcustomelementregistry` starts the parsed root with a null custom-
element registry instead of the document's global registry. A registry can
subsequently be associated with that root through
[`CustomElementRegistry.initialize()`](https://html.spec.whatwg.org/multipage/custom-elements.html#dom-customelementregistry-initialize).
`shadowrootmode` is an ASCII-case-insensitive enumerated attribute: `OPEN` and
`Closed` attach roots as well. Breeze folds only the parser token used by the
pinned html5ever tree builder's private mode gate, then restores the original
attribute spelling when a template remains in the light DOM. It does not
rewrite source HTML or unrelated attribute values. Closed roots do not become
visible through `Element.shadowRoot`; they remain available to internal
shadow-including traversal and layout.
Parser-inserted scripts in a connected declarative root run at their normal
parser checkpoint; scripts in an ordinary template remain inert. A classic
script running in a shadow tree sees `document.currentScript === null`.

The opt-in is *per parsing context*. `DOMParser.parseFromString()` creates a
detached document without this permission. Fragment parsing for `innerHTML`,
`outerHTML`, and `insertAdjacentHTML` also leaves declarative templates inert.
`Document.parseHTMLUnsafe()` instead creates a detached `about:blank`, UTF-8
HTML document with scripting disabled and a null custom-element registry.
Its default path preserves unsafe markup without executing parsed scripts;
scripts remain inert after adoption. The optional `options.sanitizer` member
is not implemented and fails explicitly rather than silently skipping
sanitization.
Trusted Types policy integration remains outside this slice.
Ordinary template contents use an inert owner document, so nesting a
declarative template inside one does not attach a root. Invalid hosts,
invalid modes, and a second declarative template on one host retain normal
template markup instead of silently losing content.

`HTMLTemplateElement` reflects `htmlFor` and the six declarative shadow-root
attributes independently of parser consumption. Mode and slot-assignment
getters use ASCII-case-insensitive matching and return canonical enum values;
their setters preserve the supplied attribute strings, including invalid
values. Delegates-focus, serializable, and clonable are boolean-by-presence
attributes. `shadowrootcustomelementregistry` is a boolean content attribute,
but its `shadowRootCustomElementRegistry` IDL reflection uses DOMString.
Changing these properties on an existing template does not itself attach a
shadow root.

The [DOM `attachShadow()` algorithm](https://dom.spec.whatwg.org/#dom-element-attachshadow)
allows a same-mode call to reuse a parser-created root, clearing its contents
and its declarative flag. A mismatched mode or a second call after reuse still
throws `NotSupportedError` in JavaScript.

[`Element.getHTML()` and `ShadowRoot.getHTML()`](https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#html-serialization-methods)
use the HTML fragment serializer. Without options, they serialize light/fragment
children like `innerHTML`. `{ serializableShadowRoots: true }` includes roots
whose `serializable` flag is set; `{ shadowRoots: [root] }` includes an explicitly
named root, including a closed or non-serializable one. A selected root is
emitted as a declarative `<template>` before its host's light children, with
its mode, focus, serializable, slot assignment, and clonable attributes. The
serializer also emits `shadowrootcustomelementregistry=""` when the owner
document and root registry kinds require it. This records that reparsing must
start with a null root registry; the markup cannot serialize a scoped registry
object or its definitions, so callers must initialize the reparsed root. The
same options apply recursively to nested roots. `innerHTML` and `outerHTML`
still omit shadow trees by default. `getHTML()` does not sanitize its output.

Coverage is in `src/engine/dom/document/declarative_shadow_tests.rs` and
the `src/engine/script/tests/` modules for declarative parsing, cloning,
serialization, and slot distribution. They check parser consumption, streaming
chunk boundaries, closed and nested roots, inert contexts, JS-visible reuse,
selected-root round trips, and slot assignment. This is not a claim of complete
Shadow DOM conformance; custom-element lifecycle timing still needs broader
interoperability coverage.
