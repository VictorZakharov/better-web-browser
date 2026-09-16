# Parser observation and stylesheet ownership

## Parser mutation observers

The live network parser and script-created document streams journal child insertion/removal,
text coalescing, and attributes appended to existing elements. The journal captures ancestor
and sibling identities at mutation time, including foster parenting and reparenting. It does
not reconstruct records by diffing a completed tree. Records are transferred before author
JavaScript resumes; observer callbacks still use the realm's mutation-observer microtask.
Nested written scripts do not introduce a checkpoint inside the surrounding script.

Static/fragment parsing does not duplicate the records queued by DOM bindings. Unobserved
documents do not allocate parser records; old text is copied only while an observer requests
character-data old values. Disconnecting the final registration disables journal collection.
An observer registered on multiple ancestors receives one record, retaining old values if
any matching registration requested them.

Contract: [DOM mutation records](https://dom.spec.whatwg.org/#queue-a-mutation-record) and
[HTML tree construction](https://html.spec.whatwg.org/multipage/parsing.html#creating-and-inserting-nodes).

The original `benchmarks/alpha/fixtures/parser-observers.html` fixture agrees with unified-headless
Chrome 153 on exact record metadata, text coalescing, foster parenting, and observer-before-script
ordering. Hidden integration tests also cover synchronous writes and document replacement.
Upstream `MutationObserver-takeRecords.html` is part of the strict curated gate.

The upstream `MutationObserver-document.html` now passes all four subtests in the strict gate,
including [synchronous dynamic inline-script insertion](inline-scripts-and-table-geometry.md).
This is not a claim of complete MutationObserver or script-scheduling conformance.

## Autonomous custom elements during HTML parsing

Registered autonomous custom-element start tokens suspend tree construction before attributes
and connection. Author constructors run outside native DOM borrows. The parser then installs
all attributes, invokes attribute reactions, inserts the constructed element, and invokes
connection reactions before consuming child tokens. A constructor can return another valid
element; a failed constructor inserts a distinct failed HTMLUnknownElement, not a leaked object.
Template contents remain inert and foreign-namespace elements do not run HTML constructors.

Network parsing performs microtask checkpoints when the author JavaScript stack is empty.
Synchronous document.write keeps those jobs deferred until the surrounding author script
returns. Attribute reactions retain the throw-on-dynamic-markup-insertion guard; connection
callbacks can write at the active parser insertion point.

Contract: [HTML create an element for a token](https://html.spec.whatwg.org/multipage/parsing.html#creating-and-inserting-nodes)
and [DOM create an element](https://dom.spec.whatwg.org/#concept-create-element).
The original parser-custom-elements.html fixture matches hidden Chrome 153's exact constructor,
attribute, connection, and microtask trace for network HTML and synchronous writes. Seven upstream
parser tests (17 subtests) and six hidden integration regressions exercise this slice.
Customized built-ins, scoped registries, and XML parser construction are not claimed here.

## Stylesheet sets and imported CSSOM

Titled document sheets choose the first preferred non-alternate set at
association, not when a later JavaScript query happens to inspect the tree. Persistent sheets
remain active, title matching is case-sensitive, and alternate sheets with the preferred title
participate. Shadow-root sheets have no stylesheet-set title. Individual CSSStyleSheet.disabled changes update the native cascade without adding
a disabled content attribute. Owner state is allocated lazily; ordinary DOM nodes do not carry
an inline stylesheet graph.

Each linked owner and import occurrence has independent CSSOM identity. CSSImportRule exposes
href, media, supports/layer metadata, its parent sheet, and the imported sheet with ownerRule and
parentStyleSheet links. Final response URLs are the bases for nested imports; response origins
gate cssRules access. Removing a rule unlinks its parent, while existing JavaScript references
retain the removed objects. Replacing a style element's text creates a new associated stylesheet.

Ordinary and imported insertRule/deleteRule, declaration edits, media changes, and disabled flags
invalidate computed style and presentation. They do not rewrite DOM text or the shared response
cache. Bounded native snapshots identify import occurrences by path beneath their owner; another
owner using identical network bytes remains independent. Dynamically inserted imports use the
existing dependency loader, MIME checks, cycle/depth/occurrence limits, and completion pipeline.
Downloaded sources are synchronized to the retained realm even after the owner's initial load
event has fired. Constructed sheets retain their existing no-import restriction.

Contract: [CSSOM stylesheet collections](https://drafts.csswg.org/cssom/#css-style-sheet-collections),
[CSSImportRule](https://drafts.csswg.org/cssom/#the-cssimportrule-interface), and
[CSS cascade imports](https://www.w3.org/TR/css-cascade-5/#at-import).

The original stylesheet-ownership.html fixture agrees with unified-headless Chrome 153 on
independent identities, parent links, imported-rule edits, media switching, deletion, insertion,
source replacement, and the rendered preferred-set color. Two measured reference differences
are retained explicitly: Chrome reports disabled=false for its inactive titled sheet, and setting
disabled on its imported sheet does not remove that sheet's rendered rules. Breeze follows the
CSSOM disabled-flag contract for both; this fixture is not an exact all-fields parity claim.
Strict WPT includes both preferred-stylesheet insertion-order cases and imported-sheet identity.
A hidden production-browser test inserts an import after load, waits for its network completion,
then checks independent mutation and computed color.

Remaining boundaries include HTTP Default-Style/meta selection, user-facing set selection,
non-HTTP(S) stylesheet loading, full CSS encoding/CORS metadata, cascade-layer ordering, and
the complete grouping/namespace/page-rule CSSOM. Layer metadata is exposed but layered imports
are not incorrectly applied as unlayered CSS. This does not claim complete CSSOM conformance or
a page-loading speedup.
