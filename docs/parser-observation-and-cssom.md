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

The upstream `MutationObserver-document.html` remains in discovery: its parser-insertion and
parent-removal subtests pass, but its dynamic inline script insertion subtest fails because
that separate synchronous script-execution path is not implemented. This is not a claim of
complete MutationObserver or script-scheduling conformance.

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
