    // DOM Standard §4.4: tree position, namespace resolution, and text-node
    // normalization. Attribute nodes do not participate in the owner tree.
    // https://dom.spec.whatwg.org/#interface-node
    const documentPosition = {
        DISCONNECTED: 1, PRECEDING: 2, FOLLOWING: 4, CONTAINS: 8,
        CONTAINED_BY: 16, IMPLEMENTATION_SPECIFIC: 32
    };
    for (const [name, value] of Object.entries(documentPosition)) {
        Object.defineProperty(Node, `DOCUMENT_POSITION_${name}`, {value, enumerable: true});
        Object.defineProperty(Node.prototype, `DOCUMENT_POSITION_${name}`, {value, enumerable: true});
    }
    const nodeOrder = new WeakMap();
    let nextNodeOrder = 0;
    const stableNodeOrder = node => {
        if (!nodeOrder.has(node)) nodeOrder.set(node, ++nextNodeOrder);
        return nodeOrder.get(node);
    };
    const nodeAncestors = node => {
        const nodes = [];
        for (; node; node = node.parentNode) nodes.push(node);
        return nodes;
    };
    Node.prototype.compareDocumentPosition = function(other) {
        if (!isNode(other)) throw new TypeError('compareDocumentPosition requires a Node');
        if (this === other) return 0;
        const firstAttribute = other instanceof Attr ? other : null;
        const secondAttribute = this instanceof Attr ? this : null;
        const first = firstAttribute ? firstAttribute.ownerElement : other;
        const second = secondAttribute ? secondAttribute.ownerElement : this;
        if (firstAttribute && secondAttribute && first && first === second) {
            for (const attribute of Array.from(first.attributes)) {
                if (attribute === firstAttribute)
                    return documentPosition.IMPLEMENTATION_SPECIFIC | documentPosition.PRECEDING;
                if (attribute === secondAttribute)
                    return documentPosition.IMPLEMENTATION_SPECIFIC | documentPosition.FOLLOWING;
            }
        }
        if (!first || !second || first.getRootNode() !== second.getRootNode()) {
            const direction = stableNodeOrder(other) < stableNodeOrder(this)
                ? documentPosition.PRECEDING : documentPosition.FOLLOWING;
            return documentPosition.DISCONNECTED | documentPosition.IMPLEMENTATION_SPECIFIC | direction;
        }
        if ((first !== second && first.contains(second) && !firstAttribute) ||
            (first === second && secondAttribute))
            return documentPosition.PRECEDING | documentPosition.CONTAINS;
        if ((first !== second && second.contains(first) && !secondAttribute) ||
            (first === second && firstAttribute))
            return documentPosition.FOLLOWING | documentPosition.CONTAINED_BY;
        const firstPath = nodeAncestors(first).reverse();
        const secondPath = nodeAncestors(second).reverse();
        let depth = 0;
        while (depth < firstPath.length && firstPath[depth] === secondPath[depth]) depth++;
        const siblings = Array.from(firstPath[depth - 1].childNodes);
        return siblings.indexOf(firstPath[depth]) < siblings.indexOf(secondPath[depth])
            ? documentPosition.PRECEDING : documentPosition.FOLLOWING;
    };
    const nodeXmlNamespace = 'http://www.w3.org/XML/1998/namespace';
    const nodeXmlnsNamespace = 'http://www.w3.org/2000/xmlns/';
    const namespaceElement = node => {
        if (node instanceof Document) return node.documentElement;
        if (node instanceof Attr) return node.ownerElement;
        if (node instanceof DocumentFragment || node instanceof DocumentType) return null;
        return node instanceof Element ? node : node.parentElement;
    };
    const namespaceDeclaration = (element, prefix) => {
        for (const attribute of Array.from(element.attributes)) {
            if (attribute.namespaceURI !== nodeXmlnsNamespace) continue;
            if (prefix === null && attribute.prefix === null && attribute.localName === 'xmlns')
                return attribute.value || null;
            if (attribute.prefix === 'xmlns' && attribute.localName === prefix)
                return attribute.value || null;
        }
        return undefined;
    };
    const locateNamespace = (node, prefix) => {
        for (let element = namespaceElement(node); element; element = element.parentElement) {
            if (prefix === 'xml') return nodeXmlNamespace;
            if (prefix === 'xmlns') return nodeXmlnsNamespace;
            if (element.namespaceURI && element.prefix === prefix) return element.namespaceURI;
            const declared = namespaceDeclaration(element, prefix);
            if (declared !== undefined) return declared;
        }
        return null;
    };
    Node.prototype.lookupNamespaceURI = function(prefix) {
        return locateNamespace(this, prefix == null || prefix === '' ? null : String(prefix));
    };
    Node.prototype.isDefaultNamespace = function(namespace) {
        return locateNamespace(this, null) === (namespace == null || namespace === '' ? null : String(namespace));
    };
    Node.prototype.lookupPrefix = function(namespace) {
        if (namespace == null || namespace === '') return null;
        namespace = String(namespace);
        for (let element = namespaceElement(this); element; element = element.parentElement) {
            if (element.namespaceURI === namespace && element.prefix) return element.prefix;
            for (const attribute of Array.from(element.attributes)) {
                if (attribute.namespaceURI === nodeXmlnsNamespace && attribute.prefix === 'xmlns' &&
                    attribute.value === namespace) return attribute.localName;
            }
        }
        return null;
    };
    Node.prototype.normalize = function() {
        const normalizeChildren = parent => {
            let child = parent.firstChild;
            while (child) {
                if (child.nodeType === Node.TEXT_NODE) {
                    if (!child.length) { const next = child.nextSibling; child.remove(); child = next; continue; }
                    let following = child.nextSibling;
                    const merging = [];
                    while (following?.nodeType === Node.TEXT_NODE) {
                        merging.push(following);
                        following = following.nextSibling;
                    }
                    if (merging.length) {
                        const length = child.length;
                        child.appendData(merging.map(node => node.data).join(''));
                        let offset = length;
                        for (const node of merging) {
                            rangeBeforeMergingText(child, node, offset);
                            offset += node.length;
                            node.remove();
                        }
                    }
                    child = following;
                } else {
                    if (child.hasChildNodes()) normalizeChildren(child);
                    child = child.nextSibling;
                }
            }
        };
        normalizeChildren(this);
    };
