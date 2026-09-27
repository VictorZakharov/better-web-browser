# Scoped custom-element registry contract

Breeze follows the [HTML custom-element registry algorithms](https://html.spec.whatwg.org/multipage/custom-elements.html#custom-element-registry)
and the [DOM clone/adoption algorithms](https://dom.spec.whatwg.org/#concept-node-clone)
for autonomous custom elements. `customElements` is the active document's
global registry; `new CustomElementRegistry()` creates a distinct scoped
registry. A definition is looked up using the element's assigned registry,
not merely its name in the global registry. Accordingly, the same name may
have different constructors in different registries.

`Document.createElement()` and `createElementNS()` accept
`{ customElementRegistry }`; `Element.attachShadow()` accepts the same option,
and `Element`, `Document`, and `ShadowRoot` expose their registry. An explicit
null registry stays null until `CustomElementRegistry.initialize(root)` fills
it. `initialize()` visits ordinary descendants only; `upgrade()` visits
shadow-including descendants but only upgrades elements belonging to that
registry. A declarative shadow root bearing
`shadowrootcustomelementregistry` begins with a null registry and can be
initialized later, including when a global definition already exists for a
name in its subtree. The parser chooses the intended parent's registry before
running any custom-element constructor or constructor-only parser guard.

`cloneNode()` and `importNode()` preserve scoped registry identity, map a
global registry to the destination document's effective global registry, and
apply the import fallback only to null ordinary descendants. A clonable
shadow root is copied even for a shallow host clone; its descendants do not
inherit the import fallback. Omitting the import option uses the destination
document's registry; the dictionary's registry member is non-nullable. Closed
roots remain closed. Cross-document
adoption retains scoped registries while remapping global and eligible null
registries according to the destination document and parent. Native shadow
root metadata tracks global/null/scoped kind and the declarative keep-null
flag for parser, adoption, and serialization; JavaScript retains the actual
scoped registry object identity.

Customized built-in elements are still unsupported and reject their `is` or
`extends` options explicitly. The implementation does not claim complete
custom-element conformance across realms or all lifecycle timing edge cases;
focused tests are in `src/engine/script/tests/custom_element_scopes.rs`.
