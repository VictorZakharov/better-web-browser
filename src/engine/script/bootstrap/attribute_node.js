    // Attr is not arena-backed. Keep its comparison data private like native nodes;
    // author-defined getters and properties must not change DOM equality.
    const attributeStates = new WeakMap();
    const readAttributeValue = attribute => {
        const state = attributeStates.get(attribute);
        if (state.element) {
            const value = host('attrGetNs', nodeId(state.element), state.namespace || '', state.localName);
            if (value !== null) state.value = value;
        }
        return state.value;
    };
    const synchronizeAttribute = (attribute, record) => Object.assign(attributeStates.get(attribute), record);
    const attachAttribute = (attribute, element) => {
        const state = attributeStates.get(attribute);
        state.element = element;
        state.document = element.ownerDocument;
    };
    const detachAttributeState = (attribute, value) => {
        const state = attributeStates.get(attribute);
        if (value !== undefined) state.value = value;
        state.element = null;
    };
    class Attr extends Node {
        constructor(token, ownerDocument, record, ownerElement = null) {
            if (token !== attributeConstructionToken) throw new TypeError('Illegal constructor');
            super(0, 2, record.qualifiedName, record.localName, record.namespace);
            attributeStates.set(this, { ...record, document: ownerDocument, element: ownerElement });
            host('bindAttributeWrapper', this, () => {
                const state = attributeStates.get(this);
                return [state.namespace, state.localName, readAttributeValue(this)];
            });
        }
        get nodeType() { return 2; }
        get nodeName() { return attributeStates.get(this).qualifiedName; }
        get namespaceURI() { return attributeStates.get(this).namespace; }
        get prefix() { return attributeStates.get(this).prefix; }
        get localName() { return attributeStates.get(this).localName; }
        get name() { return attributeStates.get(this).qualifiedName; }
        get value() { return readAttributeValue(this); }
        set value(value) {
            value = String(value);
            if (!attributeStates.get(this).element) { attributeStates.get(this).value = value; return; }
            setAttachedAttributeValue(this, value);
        }
        get nodeValue() { return this.value; }
        set nodeValue(value) { this.value = value == null ? '' : String(value); }
        get textContent() { return this.value; }
        set textContent(value) { this.value = value == null ? '' : String(value); }
        get ownerElement() { return attributeStates.get(this).element; }
        get ownerDocument() { return attributeStates.get(this).document; }
        get specified() { return true; }
        get parentNode() { return null; }
        get parentElement() { return null; }
        get firstChild() { return null; }
        get lastChild() { return null; }
        get nextSibling() { return null; }
        get previousSibling() { return null; }
        get childNodes() { const nodes = []; nodes.item = () => null; return nodes; }
        get children() { const nodes = []; nodes.item = () => null; return nodes; }
        get isConnected() { return false; }
        hasChildNodes() { return false; }
        contains(other) { return other === this; }
        querySelector() { return null; }
        querySelectorAll() { const nodes = []; nodes.item = () => null; return nodes; }
        appendChild() { throw new DOMException('Attributes cannot have children', 'HierarchyRequestError'); }
        insertBefore() { throw new DOMException('Attributes cannot have children', 'HierarchyRequestError'); }
        removeChild() { throw new DOMException('Attributes do not have children', 'NotFoundError'); }
        cloneNode() {
            const state = attributeStates.get(this);
            return createDetachedAttribute(state.document, state.namespace, state.prefix, state.localName, readAttributeValue(this));
        }
    }
    Object.defineProperty(Attr.prototype, Symbol.toStringTag, { value: 'Attr', configurable: true });
