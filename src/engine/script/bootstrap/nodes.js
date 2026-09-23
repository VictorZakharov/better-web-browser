    const insertionRecords = child => {
        // A standards-compliant fragment insertion moves its children and empties the fragment,
        // so lifecycle state must be captured before crossing the host boundary.
        const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
        return nodes.map(node => ({
            node,
            wasConnected: node.isConnected,
            oldDocument: node.ownerDocument,
            oldParent: node.parentNode,
            oldIndex: node.parentNode ? Array.from(node.parentNode.childNodes).indexOf(node) : -1,
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
    const finishInsertion = (parent, records) => withCustomElementReactions(() => {
        for (const { node, wasConnected, oldDocument } of records) {
            if (wasConnected) disconnectElementTree(node);
            if (oldDocument !== node.ownerDocument)
                adoptCustomElementTree(node, oldDocument, node.ownerDocument);
            if (node.isConnected) connectElementTree(node);
            // DOM insertion steps only try upgrading nodes that are connected. Inert template
            // fragments may be adopted into a detached cache without running constructors.
        }
        insertedScriptSteps(parent, records.map(record => record.node));
    });
    const convertNodes = items => {
        const nodes = items.map(item =>
            isNode(item) ? item : document.createTextNode(String(item)));
        if (nodes.length === 1) return nodes[0];
        const fragment = document.createDocumentFragment();
        for (const node of nodes) fragment.appendChild(node);
        return fragment;
    };
    replaceElementInnerHtml = (element, value) => {
        const isTemplateContents = element.localName === 'template';
        const target = isTemplateContents
            ? wrap(host('templateContent', nodeId(element))) : element;
        const wasConnected = target.isConnected;
        const removedChildren = [...target.childNodes];
        for (const child of removedChildren) iteratorPreRemove(child);
        host('innerHtmlSet', nodeId(element), value == null ? '' : String(value));
        for (let index = removedChildren.length - 1; index >= 0; index--)
            rangeAfterRemovingNode(removedChildren[index], target, index);
        markChildCollectionsChanged(target);
        const addedChildren = [...target.childNodes];
        rangeAfterInsertingNodes(target, 0, addedChildren.length);
        if (removedChildren.length || addedChildren.length) queueMutationRecord(target, 'childList', {
            addedNodes: addedChildren,
            removedNodes: removedChildren
        });
        if (wasConnected) for (const child of removedChildren) disconnectElementTree(child);
        for (const child of addedChildren) {
            if (wasConnected) connectElementTree(child);
            // Template contents use an inert template-contents owner document and do not share
            // the host document's custom-element registry. Importing or connecting to the live
            // document performs the upgrade against that destination registry.
            else if (!isTemplateContents) upgradeCustomElementTree(child);
        }
        if (wasConnected) refreshWindowNamedProperties(removedChildren.concat(addedChildren));
        scheduleSlotChangeCheck();
        scriptChildrenChanged(target);
    };

    const rangeTextEdit = new WeakSet();
    class Node extends EventTarget {
        constructor(id, type, name, localName, namespaceURI) {
            super();
            if (type !== 2 && !host('bindNodeWrapper', this, id)) throw new TypeError('Illegal constructor');
            nodeHandles.set(this, id);
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
        get ownerDocument() { return wrap(host('ownerDocument', nodeId(this))); }
        get baseURI() { return this.ownerDocument?.baseURI || null; }
        get parentNode() { return wrap(host('parent', nodeId(this))); }
        get parentElement() { const parent = this.parentNode; return parent?.nodeType === 1 ? parent : null; }
        get assignedSlot() { return wrap(host('assignedSlot', nodeId(this))); }
        get firstChild() { return wrap(host('firstChild', nodeId(this))); }
        get lastChild() { return wrap(host('lastChild', nodeId(this))); }
        get nextSibling() { return wrap(host('nextSibling', nodeId(this))); }
        get previousSibling() { return wrap(host('previousSibling', nodeId(this))); }
        get childNodes() { return childCollection(this, false); }
        get textContent() { return host('textGet', nodeId(this)); }
        set textContent(value) {
            const characterData = this.nodeType === 3 || this.nodeType === 4 ||
                this.nodeType === 7 || this.nodeType === 8;
            const oldValue = characterData ? this.textContent : null;
            const removedChildren = characterData ? [] : [...this.childNodes];
            const nextText = value == null ? '' : String(value);
            const namedAccessChanged = this.isConnected && removedChildren.some(child => child.nodeType === 1);
            for (const child of removedChildren) iteratorPreRemove(child);
            host('textSet', nodeId(this), nextText);
            if (characterData) {
                if (!rangeTextEdit.has(this))
                    rangeAfterReplacingData(this, 0, oldValue.length, nextText.length);
            } else {
                for (let index = removedChildren.length - 1; index >= 0; index--)
                    rangeAfterRemovingNode(removedChildren[index], this, index);
            }
            if (!characterData) markChildCollectionsChanged(this);
            const addedChildren = characterData ? [] : [...this.childNodes];
            if (!characterData) rangeAfterInsertingNodes(this, 0, addedChildren.length);
            if (characterData) queueMutationRecord(this, 'characterData', { oldValue });
            else if (removedChildren.length || addedChildren.length) queueMutationRecord(this, 'childList', {
                addedNodes: addedChildren,
                removedNodes: removedChildren
            });
            if (this.isConnected) for (const child of removedChildren) disconnectElementTree(child);
            if (namedAccessChanged) refreshWindowNamedProperties(removedChildren);
            scheduleSlotChangeCheck();
            scriptChildrenChanged(characterData ? this.parentNode : this);
        }
        get isConnected() {
            return this.getRootNode({ composed: true })?.nodeType === 9;
        }
        appendChild(child) {
            if (!(isNode(child))) throw new TypeError('appendChild requires a Node');
            ensurePreInsertionValidity(child, this);
            const records = insertionRecords(child);
            for (const record of records) if (record.oldParent) iteratorPreRemove(record.node);
            let insertionIndex = this.childNodes.length;
            insertionIndex -= records.filter(record => record.oldParent === this).length;
            const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
            const inserted = nodes.every(node => !!host('appendChild', nodeId(this), nodeId(node)));
            if (inserted) {
                rangeAfterRemovingRecords(records);
                rangeAfterInsertingNodes(this, insertionIndex, nodes.length);
            }
            if (inserted) markChildCollectionsChanged(this, child.nodeType === 11 ? child : null,
                records.map(record => record.oldParent));
            if (inserted) queueInsertionMutationRecords(this, child, records);
            if (inserted && this.isConnected) refreshWindowNamedProperties(nodes);
            if (inserted) scheduleSlotChangeCheck();
            if (inserted) finishInsertion(this, records);
            return inserted ? child : null;
        }
        insertBefore(child, reference) {
            if (!(isNode(child))) throw new TypeError('insertBefore requires a Node');
            if (reference != null && !(isNode(reference))) throw new TypeError('reference must be a Node');
            ensurePreInsertionValidity(child, this, reference);
            const records = insertionRecords(child);
            for (const record of records) if (record.oldParent && child !== reference)
                iteratorPreRemove(record.node);
            let insertionIndex = reference ? Array.from(this.childNodes).indexOf(reference) :
                this.childNodes.length;
            insertionIndex -= records.filter(record => record.oldParent === this &&
                record.oldIndex < insertionIndex).length;
            const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
            const inserted = nodes.every(node =>
                !!host('insertBefore', nodeId(this), nodeId(node), nodeId(reference) || 0));
            if (inserted && child !== reference) {
                rangeAfterRemovingRecords(records);
                rangeAfterInsertingNodes(this, insertionIndex, nodes.length);
            }
            if (inserted) markChildCollectionsChanged(this, child.nodeType === 11 ? child : null,
                records.map(record => record.oldParent));
            if (inserted) queueInsertionMutationRecords(this, child, records);
            if (inserted && this.isConnected) refreshWindowNamedProperties(nodes);
            if (inserted) scheduleSlotChangeCheck();
            if (inserted) finishInsertion(this, records);
            return inserted ? child : null;
        }
        replaceChild(child, replaced) {
            return withScriptMutationBatch(() => {
                if (!(isNode(child))) throw new TypeError('replaceChild requires a Node');
                if (!(isNode(replaced))) throw new TypeError('replaced child must be a Node');
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
            });
        }
        removeChild(child) {
            const namedAccessChanged = this.isConnected;
            const wasConnected = isNode(child) && child.isConnected;
            const previousSibling = isNode(child) ? child.previousSibling : null;
            const nextSibling = isNode(child) ? child.nextSibling : null;
            const removedIndex = isNode(child) ? Array.from(this.childNodes).indexOf(child) : -1;
            if (removedIndex >= 0) iteratorPreRemove(child);
            if (!(isNode(child)) || !host('removeChild', nodeId(this), nodeId(child))) throw new Error('node is not a child');
            rangeAfterRemovingNode(child, this, removedIndex);
            markChildCollectionsChanged(this);
            queueMutationRecord(this, 'childList', {
                removedNodes: [child], previousSibling, nextSibling
            });
            if (wasConnected) disconnectElementTree(child);
            if (namedAccessChanged) refreshWindowNamedProperties(child);
            scheduleSlotChangeCheck();
            scriptChildrenChanged(this);
            return child;
        }
        contains(other) {
            for (let node = other; node; node = node.parentNode) if (node === this) return true;
            return false;
        }
        hasChildNodes() { return !!this.firstChild; }
        isEqualNode(other = null) { return compareNodes(this, other, false); }
        isSameNode(other = null) { return compareNodes(this, other, true); }
        getRootNode(options = {}) { return wrap(host('rootNode', nodeId(this), !!Object(options).composed)); }
        cloneNode(deep = false) {
            const clone = wrap(host('cloneNode', nodeId(this), !!deep));
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
        appendData(data) { this.replaceData(this.length, 0, data); }
        insertData(offset, data) { this.replaceData(offset, 0, data); }
        deleteData(offset, count) { this.replaceData(offset, count, ''); }
        replaceData(offset, count, data) {
            offset = Number(offset) >>> 0;
            count = Number(count) >>> 0;
            if (offset > this.length) throw new DOMException('Offset exceeds data length', 'IndexSizeError');
            const original = this.data;
            const inserted = String(data);
            rangeTextEdit.add(this);
            try { this.data = original.slice(0, offset) + inserted + original.slice(offset + count); }
            finally { rangeTextEdit.delete(this); }
            rangeAfterReplacingData(this, offset, Math.min(count, original.length - offset), inserted.length);
        }
    }
    installChildNodeMembers(CharacterData.prototype);

    class Text extends CharacterData {
        splitText(offset) {
            offset = Number(offset) >>> 0;
            if (offset > this.length)
                throw new DOMException('Offset exceeds data length', 'IndexSizeError');
            const newNode = document.createTextNode(this.data.slice(offset));
            rangeTextEdit.add(this);
            try { this.data = this.data.slice(0, offset); }
            finally { rangeTextEdit.delete(this); }
            if (this.parentNode) this.parentNode.insertBefore(newNode, this.nextSibling);
            rangeAfterSplittingText(this, newNode, offset);
            return newNode;
        }
    }
    class CDATASection extends Text {}
    class Comment extends CharacterData {}
    class ProcessingInstruction extends CharacterData {
        get target() { return this.nodeName; }
    }
    class DocumentType extends Node {
        get name() { return this.nodeName; }
        get publicId() { return host('documentTypeMetadata', nodeId(this)).split('\u001f')[0] || ''; }
        get systemId() { return host('documentTypeMetadata', nodeId(this)).split('\u001f')[1] || ''; }
    }
    installChildNodeMembers(DocumentType.prototype);
    class DocumentFragment extends Node {}
    installParentNodeMembers(DocumentFragment.prototype);

    class DOMTokenList {
        constructor(element, attribute) {
            this.element = element; this.attribute = attribute;
            return new Proxy(this, {
                get(target, property, receiver) {
                    const index = collectionIndex(property);
                    if (index !== null && index < target.length) return target.item(index);
                    return Reflect.get(target, property, receiver);
                },
                has(target, property) {
                    const index = collectionIndex(property);
                    return (index !== null && index < target.length) || Reflect.has(target, property);
                },
                ownKeys(target) {
                    return [...Array(target.length).keys()].map(String).concat(Reflect.ownKeys(target));
                },
                getOwnPropertyDescriptor(target, property) {
                    const index = collectionIndex(property);
                    return index !== null && index < target.length
                        ? { value: target.item(index), writable: false, enumerable: true, configurable: true }
                        : Reflect.getOwnPropertyDescriptor(target, property);
                },
                set(target, property, value, receiver) {
                    return collectionIndex(property) === null && Reflect.set(target, property, value, receiver);
                },
                defineProperty(target, property, descriptor) {
                    return collectionIndex(property) === null && Reflect.defineProperty(target, property, descriptor);
                },
                deleteProperty(target, property) {
                    const index = collectionIndex(property);
                    return !(index !== null && index < target.length) && Reflect.deleteProperty(target, property);
                }
            });
        }
        _tokens() { return [...new Set((this.element.getAttribute(this.attribute) || '').split(/[\t\n\f\r ]+/).filter(Boolean))]; }
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
        toString() { return this.value; }
    }
    installIndexedIterator(DOMTokenList.prototype, true);
