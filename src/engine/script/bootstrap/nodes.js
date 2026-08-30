    const insertionRecords = child => {
        // A standards-compliant fragment insertion moves its children and empties the fragment,
        // so lifecycle state must be captured before crossing the host boundary.
        const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
        return nodes.map(node => ({
            node,
            wasConnected: node.isConnected,
            oldDocument: node.ownerDocument,
            oldParent: node.parentNode
        }));
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
        const fragment = document.createDocumentFragment();
        for (const item of items) {
            fragment.appendChild(item instanceof Node ? item : document.createTextNode(String(item)));
        }
        return fragment;
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
        get children() { return childCollection(this, true); }
        get firstElementChild() { return this.children[0] || null; }
        get lastElementChild() { const children = this.children; return children[children.length - 1] || null; }
        get childElementCount() { return this.children.length; }
        get textContent() { return host('textGet', this.__id); }
        set textContent(value) {
            const characterData = this.nodeType === 3;
            const oldValue = characterData ? this.textContent : null;
            const namedAccessChanged = this.isConnected && this.firstElementChild !== null;
            const removedChildren = !characterData && this.isConnected ? [...this.childNodes] : [];
            host('textSet', this.__id, value == null ? '' : String(value));
            if (!characterData) markChildCollectionsChanged(this);
            for (const child of removedChildren) disconnectCustomElementTree(child);
            if (namedAccessChanged) refreshWindowNamedProperties(removedChildren);
            queueMutationRecord(this, characterData ? 'characterData' : 'childList', { oldValue });
            scheduleSlotChangeCheck();
        }
        get isConnected() {
            return this.getRootNode({ composed: true })?.nodeType === 9;
        }
        appendChild(child) {
            if (!(child instanceof Node)) throw new TypeError('appendChild requires a Node');
            const records = insertionRecords(child);
            const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
            const inserted = nodes.every(node => !!host('appendChild', this.__id, node.__id));
            if (inserted) markChildCollectionsChanged(this, child.nodeType === 11 ? child : null,
                records.map(record => record.oldParent));
            if (inserted) finishInsertion(records);
            if (inserted && this.isConnected) refreshWindowNamedProperties(nodes);
            if (inserted) scheduleSlotChangeCheck();
            return inserted ? child : null;
        }
        append(...items) {
            for (const item of items) this.appendChild(item instanceof Node ? item : document.createTextNode(String(item)));
        }
        prepend(...items) {
            let reference = this.firstChild;
            for (const item of items) {
                const node = item instanceof Node ? item : document.createTextNode(String(item));
                this.insertBefore(node, reference);
                if (!reference) reference = node.nextSibling;
            }
        }
        insertBefore(child, reference) {
            if (!(child instanceof Node)) throw new TypeError('insertBefore requires a Node');
            if (reference != null && !(reference instanceof Node)) throw new TypeError('reference must be a Node');
            const records = insertionRecords(child);
            const nodes = child.nodeType === 11 ? [...child.childNodes] : [child];
            const inserted = nodes.every(node =>
                !!host('insertBefore', this.__id, node.__id, reference?.__id || 0));
            if (inserted) markChildCollectionsChanged(this, child.nodeType === 11 ? child : null,
                records.map(record => record.oldParent));
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
            if (this.insertBefore(child, replaced) === null)
                throw new DOMException('The replacement cannot be inserted here', 'HierarchyRequestError');
            this.removeChild(replaced);
            return replaced;
        }
        removeChild(child) {
            const namedAccessChanged = this.isConnected;
            const wasConnected = child instanceof Node && child.isConnected;
            if (!(child instanceof Node) || !host('removeChild', this.__id, child.__id)) throw new Error('node is not a child');
            markChildCollectionsChanged(this);
            if (wasConnected) disconnectCustomElementTree(child);
            if (namedAccessChanged) refreshWindowNamedProperties(child);
            scheduleSlotChangeCheck();
            return child;
        }
        remove() {
            const namedAccessChanged = this.isConnected;
            const parent = this.parentNode;
            const removed = host('remove', this.__id);
            if (removed) markChildCollectionsChanged(parent);
            if (removed && namedAccessChanged) {
                disconnectCustomElementTree(this);
                refreshWindowNamedProperties(this);
            }
            if (removed) scheduleSlotChangeCheck();
        }
        replaceWith(...items) {
            const parent = this.parentNode;
            if (!parent) return;
            let viableNextSibling = this.nextSibling;
            const itemNodes = items.filter(item => item instanceof Node);
            while (viableNextSibling && itemNodes.includes(viableNextSibling)) {
                viableNextSibling = viableNextSibling.nextSibling;
            }
            const replacement = convertNodes(items);
            if (this.parentNode === parent) {
                parent.insertBefore(replacement, this);
                parent.removeChild(this);
            } else {
                parent.insertBefore(replacement, viableNextSibling);
            }
        }
        contains(other) {
            for (let node = other; node; node = node.parentNode) if (node === this) return true;
            return false;
        }
        hasChildNodes() { return !!this.firstChild; }
        getRootNode(options = {}) { return wrap(host('rootNode', this.__id, !!Object(options).composed)); }
        querySelector(selector) { return wrap(host('query', this.__id, String(selector))); }
        querySelectorAll(selector) { return list(host('queryAll', this.__id, String(selector))); }
        cloneNode(deep = false) {
            const clone = wrap(host('cloneNode', this.__id, !!deep));
            upgradeCustomElementTree(clone);
            return clone;
        }
    }

    class CharacterData extends Node {
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

    class Text extends CharacterData {}
    class Comment extends CharacterData {}
    class DocumentType extends Node {}
    class DocumentFragment extends Node {}

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
        _map() {
            const map = new Map();
            for (const declaration of (this.element.getAttribute('style') || '').split(';')) {
                const split = declaration.indexOf(':');
                if (split > 0) map.set(declaration.slice(0, split).trim().toLowerCase(), declaration.slice(split + 1).trim());
            }
            return map;
        }
        _write(map) { this.element.setAttribute('style', [...map].map(([name, value]) => name + ': ' + value).join('; ')); }
        get cssText() { return this.element.getAttribute('style') || ''; }
        set cssText(value) { this.element.setAttribute('style', String(value)); }
        getPropertyValue(name) { return this._map().get(String(name).toLowerCase()) || ''; }
        setProperty(name, value, priority = '') {
            const map = this._map();
            map.set(String(name).toLowerCase(), String(value) + (priority ? ' !' + priority : ''));
            this._write(map);
        }
        removeProperty(name) {
            const map = this._map();
            const old = map.get(String(name).toLowerCase()) || '';
            map.delete(String(name).toLowerCase());
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
