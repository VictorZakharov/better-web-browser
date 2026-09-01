    const insertionRecords = child => {
        // A standards-compliant fragment insertion moves its children and empties the fragment,
        // so lifecycle state must be captured before crossing the host boundary.
        const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
        return nodes.map(node => ({
            node,
            wasConnected: node.isConnected,
            oldDocument: node.ownerDocument,
            oldParent: node.parentNode,
            oldPreviousSibling: node.previousSibling,
            oldNextSibling: node.nextSibling
        }));
    };
    const queueInsertionMutationRecords = (parent, child, records) => {
        const nodes = records.map(record => record.node);
        if (!nodes.length) return;
        if (child.nodeType === Node.DOCUMENT_FRAGMENT_NODE) {
            queueMutationRecord(child, 'childList', { removedNodes: nodes });
        } else {
            for (const record of records) {
                if (!record.oldParent) continue;
                queueMutationRecord(record.oldParent, 'childList', {
                    removedNodes: [record.node],
                    previousSibling: record.oldPreviousSibling,
                    nextSibling: record.oldNextSibling
                });
            }
        }
        queueMutationRecord(parent, 'childList', {
            addedNodes: nodes,
            previousSibling: nodes[0].previousSibling,
            nextSibling: nodes[nodes.length - 1].nextSibling
        });
    };
    const finishInsertion = records => withCustomElementReactions(() => {
        for (const { node, wasConnected, oldDocument } of records) {
            if (wasConnected) disconnectCustomElementTree(node);
            if (oldDocument !== node.ownerDocument)
                adoptCustomElementTree(node, oldDocument, node.ownerDocument);
            if (node.isConnected) connectCustomElementTree(node);
            else upgradeCustomElementTree(node);
        }
    });
    const convertNodes = items => {
        const nodes = items.map(item =>
            item instanceof Node ? item : document.createTextNode(String(item)));
        if (nodes.length === 1) return nodes[0];
        const fragment = document.createDocumentFragment();
        for (const node of nodes) fragment.appendChild(node);
        return fragment;
    };
    const parentNodeMembers = {
        get children() { return childCollection(this, true); },
        get firstElementChild() { return this.children[0] || null; },
        get lastElementChild() {
            const children = this.children;
            return children[children.length - 1] || null;
        },
        get childElementCount() { return this.children.length; },
        append(...items) { this.appendChild(convertNodes(items)); },
        prepend(...items) { this.insertBefore(convertNodes(items), this.firstChild); },
        replaceChildren(...items) {
            const removedNodes = [...this.childNodes];
            let addedNodes;
            const replacement = convertNodes(items);
            ensurePreInsertionValidity(replacement, this, null, removedNodes);
            withSuppressedMutationRecords(this, () => {
                while (this.firstChild) this.removeChild(this.firstChild);
                if (this.appendChild(replacement) === null)
                    throw new DOMException('The replacement cannot be inserted here', 'HierarchyRequestError');
                addedNodes = [...this.childNodes];
            });
            if (removedNodes.length || addedNodes.length) queueMutationRecord(this, 'childList', {
                addedNodes,
                removedNodes
            });
        },
        querySelector(selector) { return wrap(host('query', this.__id, String(selector))); },
        querySelectorAll(selector) { return list(host('queryAll', this.__id, String(selector))); }
    };
    const installParentNodeMembers = prototype => Object.defineProperties(
        prototype, Object.getOwnPropertyDescriptors(parentNodeMembers));
    const childNodeMembers = {
        before(...items) {
            const parent = this.parentNode;
            if (!parent) return;
            const itemNodes = items.filter(item => item instanceof Node);
            let viablePreviousSibling = this.previousSibling;
            while (viablePreviousSibling && itemNodes.includes(viablePreviousSibling))
                viablePreviousSibling = viablePreviousSibling.previousSibling;
            const insertion = convertNodes(items);
            const reference = viablePreviousSibling
                ? viablePreviousSibling.nextSibling : parent.firstChild;
            parent.insertBefore(insertion, reference);
        },
        after(...items) {
            const parent = this.parentNode;
            if (!parent) return;
            const itemNodes = items.filter(item => item instanceof Node);
            let viableNextSibling = this.nextSibling;
            while (viableNextSibling && itemNodes.includes(viableNextSibling))
                viableNextSibling = viableNextSibling.nextSibling;
            parent.insertBefore(convertNodes(items), viableNextSibling);
        },
        remove() {
            const parent = this.parentNode;
            if (parent) parent.removeChild(this);
        },
        replaceWith(...items) {
            const parent = this.parentNode;
            if (!parent) return;
            let viableNextSibling = this.nextSibling;
            const itemNodes = items.filter(item => item instanceof Node);
            while (viableNextSibling && itemNodes.includes(viableNextSibling))
                viableNextSibling = viableNextSibling.nextSibling;
            const replacement = convertNodes(items);
            if (this.parentNode === parent) parent.replaceChild(replacement, this);
            else parent.insertBefore(replacement, viableNextSibling);
        }
    };
    const installChildNodeMembers = prototype => Object.defineProperties(
        prototype, Object.getOwnPropertyDescriptors(childNodeMembers));
    replaceElementInnerHtml = (element, value) => {
        const isTemplateContents = element.localName === 'template';
        const target = isTemplateContents
            ? wrap(host('templateContent', element.__id)) : element;
        const wasConnected = target.isConnected;
        const removedChildren = [...target.childNodes];
        host('innerHtmlSet', element.__id, value == null ? '' : String(value));
        markChildCollectionsChanged(target);
        const addedChildren = [...target.childNodes];
        if (removedChildren.length || addedChildren.length) queueMutationRecord(target, 'childList', {
            addedNodes: addedChildren,
            removedNodes: removedChildren
        });
        if (wasConnected) for (const child of removedChildren) disconnectCustomElementTree(child);
        for (const child of addedChildren) {
            if (wasConnected) connectCustomElementTree(child);
            // Template contents use an inert template-contents owner document and do not share
            // the host document's custom-element registry. Importing/adopting into a document
            // performs the upgrade against that destination registry.
            else if (!isTemplateContents) upgradeCustomElementTree(child);
        }
        if (wasConnected) refreshWindowNamedProperties(removedChildren.concat(addedChildren));
        scheduleSlotChangeCheck();
    };

    class Node extends EventTarget {
        constructor(id, type, name, localName, namespaceURI) {
            super();
            this.__id = id;
            if (type === undefined) {
                const metadata = host('nodeMetadata', id).split('\u001f');
                type = Number(metadata[0]);
                name = metadata[1];
                localName = metadata[2] || null;
                namespaceURI = metadata[3] || null;
            }
            this.__nodeType = type;
            this.__nodeName = name;
            this.__localName = localName;
            this.__namespaceURI = namespaceURI;
        }
        get nodeType() { return this.__nodeType; }
        get nodeName() { return this.__nodeName; }
        get ownerDocument() { return wrap(host('ownerDocument', this.__id)); }
        get parentNode() { return wrap(host('parent', this.__id)); }
        get parentElement() { const parent = this.parentNode; return parent?.nodeType === 1 ? parent : null; }
        get assignedSlot() { return wrap(host('assignedSlot', this.__id)); }
        get firstChild() { return wrap(host('firstChild', this.__id)); }
        get lastChild() { return wrap(host('lastChild', this.__id)); }
        get nextSibling() { return wrap(host('nextSibling', this.__id)); }
        get previousSibling() { return wrap(host('previousSibling', this.__id)); }
        get childNodes() { return childCollection(this, false); }
        get textContent() { return host('textGet', this.__id); }
        set textContent(value) {
            const characterData = this.nodeType === 3 || this.nodeType === 4 ||
                this.nodeType === 7 || this.nodeType === 8;
            const oldValue = characterData ? this.textContent : null;
            const removedChildren = characterData ? [] : [...this.childNodes];
            const namedAccessChanged = this.isConnected && removedChildren.some(child => child.nodeType === 1);
            host('textSet', this.__id, value == null ? '' : String(value));
            if (!characterData) markChildCollectionsChanged(this);
            const addedChildren = characterData ? [] : [...this.childNodes];
            if (characterData) queueMutationRecord(this, 'characterData', { oldValue });
            else if (removedChildren.length || addedChildren.length) queueMutationRecord(this, 'childList', {
                addedNodes: addedChildren,
                removedNodes: removedChildren
            });
            if (this.isConnected) for (const child of removedChildren) disconnectCustomElementTree(child);
            if (namedAccessChanged) refreshWindowNamedProperties(removedChildren);
            scheduleSlotChangeCheck();
        }
        get isConnected() {
            return this.getRootNode({ composed: true })?.nodeType === 9;
        }
        appendChild(child) {
            if (!(child instanceof Node)) throw new TypeError('appendChild requires a Node');
            ensurePreInsertionValidity(child, this);
            const records = insertionRecords(child);
            const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
            const inserted = nodes.every(node => !!host('appendChild', this.__id, node.__id));
            if (inserted) markChildCollectionsChanged(this, child.nodeType === 11 ? child : null,
                records.map(record => record.oldParent));
            if (inserted) queueInsertionMutationRecords(this, child, records);
            if (inserted) finishInsertion(records);
            if (inserted && this.isConnected) refreshWindowNamedProperties(nodes);
            if (inserted) scheduleSlotChangeCheck();
            return inserted ? child : null;
        }
        insertBefore(child, reference) {
            if (!(child instanceof Node)) throw new TypeError('insertBefore requires a Node');
            if (reference != null && !(reference instanceof Node)) throw new TypeError('reference must be a Node');
            ensurePreInsertionValidity(child, this, reference);
            const records = insertionRecords(child);
            const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
            const inserted = nodes.every(node =>
                !!host('insertBefore', this.__id, node.__id, reference?.__id || 0));
            if (inserted) markChildCollectionsChanged(this, child.nodeType === 11 ? child : null,
                records.map(record => record.oldParent));
            if (inserted) queueInsertionMutationRecords(this, child, records);
            if (inserted) finishInsertion(records);
            if (inserted && this.isConnected) refreshWindowNamedProperties(nodes);
            if (inserted) scheduleSlotChangeCheck();
            return inserted ? child : null;
        }
        replaceChild(child, replaced) {
            if (!(child instanceof Node)) throw new TypeError('replaceChild requires a Node');
            if (!(replaced instanceof Node)) throw new TypeError('replaced child must be a Node');
            if (replaced.parentNode !== this)
                throw new DOMException('The node to replace is not a child', 'NotFoundError');
            if (child === replaced) return replaced;
            ensurePreInsertionValidity(child, this, replaced, [replaced]);
            const addedNodes = child.nodeType === Node.DOCUMENT_FRAGMENT_NODE
                ? [...child.childNodes] : [child];
            const previousSibling = replaced.previousSibling;
            const nextSibling = replaced.nextSibling;
            withSuppressedMutationRecords(this, () => {
                if (this.insertBefore(child, replaced) === null)
                    throw new DOMException('The replacement cannot be inserted here', 'HierarchyRequestError');
                this.removeChild(replaced);
            });
            queueMutationRecord(this, 'childList', {
                addedNodes,
                removedNodes: [replaced],
                previousSibling,
                nextSibling
            });
            return replaced;
        }
        removeChild(child) {
            const namedAccessChanged = this.isConnected;
            const wasConnected = child instanceof Node && child.isConnected;
            const previousSibling = child instanceof Node ? child.previousSibling : null;
            const nextSibling = child instanceof Node ? child.nextSibling : null;
            if (!(child instanceof Node) || !host('removeChild', this.__id, child.__id)) throw new Error('node is not a child');
            markChildCollectionsChanged(this);
            queueMutationRecord(this, 'childList', {
                removedNodes: [child], previousSibling, nextSibling
            });
            if (wasConnected) disconnectCustomElementTree(child);
            if (namedAccessChanged) refreshWindowNamedProperties(child);
            scheduleSlotChangeCheck();
            return child;
        }
        contains(other) {
            for (let node = other; node; node = node.parentNode) if (node === this) return true;
            return false;
        }
        hasChildNodes() { return !!this.firstChild; }
        getRootNode(options = {}) { return wrap(host('rootNode', this.__id, !!Object(options).composed)); }
        cloneNode(deep = false) {
            const clone = wrap(host('cloneNode', this.__id, !!deep));
            upgradeCustomElementTree(clone);
            return clone;
        }
    }
    // Web IDL exposes the DOM node-type constants on both the interface object and its
    // prototype. Compatibility libraries such as ShadyDOM use these symbolic values while
    // walking their logical trees, rather than comparing nodeType to numeric literals.
    // https://dom.spec.whatwg.org/#interface-node
    const nodeTypeConstants = {
        ELEMENT_NODE: 1,
        ATTRIBUTE_NODE: 2,
        TEXT_NODE: 3,
        CDATA_SECTION_NODE: 4,
        ENTITY_REFERENCE_NODE: 5,
        ENTITY_NODE: 6,
        PROCESSING_INSTRUCTION_NODE: 7,
        COMMENT_NODE: 8,
        DOCUMENT_NODE: 9,
        DOCUMENT_TYPE_NODE: 10,
        DOCUMENT_FRAGMENT_NODE: 11,
        NOTATION_NODE: 12
    };
    for (const [name, value] of Object.entries(nodeTypeConstants)) {
        Object.defineProperty(Node, name, { enumerable: true, value });
        Object.defineProperty(Node.prototype, name, { enumerable: true, value });
    }

    class CharacterData extends Node {
        get previousElementSibling() { return elementSibling(this, false); }
        get nextElementSibling() { return elementSibling(this, true); }
        get data() { return this.textContent; }
        set data(value) { this.textContent = value; }
        get nodeValue() { return this.data; }
        set nodeValue(value) { this.data = value == null ? '' : String(value); }
        get length() { return this.data.length; }
        substringData(offset, count) {
            offset = Number(offset) >>> 0;
            count = Number(count) >>> 0;
            if (offset > this.length) throw new DOMException('Offset exceeds data length', 'IndexSizeError');
            return this.data.slice(offset, offset + count);
        }
        appendData(data) { this.data += String(data); }
        insertData(offset, data) { this.replaceData(offset, 0, data); }
        deleteData(offset, count) { this.replaceData(offset, count, ''); }
        replaceData(offset, count, data) {
            offset = Number(offset) >>> 0;
            count = Number(count) >>> 0;
            if (offset > this.length) throw new DOMException('Offset exceeds data length', 'IndexSizeError');
            this.data = this.data.slice(0, offset) + String(data) + this.data.slice(offset + count);
        }
    }
    installChildNodeMembers(CharacterData.prototype);

    class Text extends CharacterData {}
    class CDATASection extends Text {}
    class Comment extends CharacterData {}
    class ProcessingInstruction extends CharacterData {
        get target() { return this.nodeName; }
    }
    class DocumentType extends Node {
        get name() { return this.nodeName; }
        get publicId() { return host('documentTypeMetadata', this.__id).split('\u001f')[0] || ''; }
        get systemId() { return host('documentTypeMetadata', this.__id).split('\u001f')[1] || ''; }
    }
    installChildNodeMembers(DocumentType.prototype);
    class DocumentFragment extends Node {}
    installParentNodeMembers(DocumentFragment.prototype);

    class DOMTokenList {
        constructor(element, attribute) { this.element = element; this.attribute = attribute; }
        _tokens() { return (this.element.getAttribute(this.attribute) || '').split(/\s+/).filter(Boolean); }
        _set(tokens) { this.element.setAttribute(this.attribute, [...new Set(tokens)].join(' ')); }
        contains(token) { return this._tokens().includes(String(token)); }
        add(...tokens) { this._set(this._tokens().concat(tokens.map(String))); }
        remove(...tokens) { const remove = new Set(tokens.map(String)); this._set(this._tokens().filter(token => !remove.has(token))); }
        toggle(token, force) {
            token = String(token);
            const present = this.contains(token);
            if (force === true || (!present && force !== false)) { this.add(token); return true; }
            if (present) this.remove(token);
            return false;
        }
        replace(oldToken, newToken) {
            const tokens = this._tokens();
            const index = tokens.indexOf(String(oldToken));
            if (index < 0) return false;
            tokens[index] = String(newToken);
            this._set(tokens);
            return true;
        }
        get value() { return this.element.getAttribute(this.attribute) || ''; }
        set value(value) { this.element.setAttribute(this.attribute, value); }
        get length() { return this._tokens().length; }
        item(index) { return this._tokens()[index] || null; }
        [Symbol.iterator]() { return this._tokens()[Symbol.iterator](); }
        toString() { return this.value; }
    }

    class CSSStyleDeclaration {
        constructor(element) { this.element = element; }
        _name(name) { name = String(name); return name.startsWith('--') ? name : name.toLowerCase(); }
        _map() {
            const map = new Map();
            for (const declaration of (this.element.getAttribute('style') || '').split(';')) {
                const split = declaration.indexOf(':');
                if (split > 0) {
                    const name = declaration.slice(0, split).trim();
                    map.set(this._name(name), declaration.slice(split + 1).trim());
                }
            }
            return map;
        }
        _write(map) { this.element.setAttribute('style', [...map].map(([name, value]) => name + ': ' + value).join('; ')); }
        get cssText() { return this.element.getAttribute('style') || ''; }
        set cssText(value) { this.element.setAttribute('style', String(value)); }
        getPropertyValue(name) { return this._map().get(this._name(name)) || ''; }
        setProperty(name, value, priority = '') {
            const map = this._map();
            map.set(this._name(name), String(value) + (priority ? ' !' + priority : ''));
            this._write(map);
        }
        removeProperty(name) {
            const map = this._map();
            name = this._name(name);
            const old = map.get(name) || '';
            map.delete(name);
            this._write(map);
            return old;
        }
    }
    const styleProxy = element => new Proxy(new CSSStyleDeclaration(element), {
        get(target, property) {
            if (property in target) {
                const value = target[property];
                return typeof value === 'function' ? value.bind(target) : value;
            }
            return target.getPropertyValue(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()));
        },
        set(target, property, value) {
            if (property === 'cssText') target.cssText = value;
            else target.setProperty(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()), value);
            return true;
        }
    });
