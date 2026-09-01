    // DOM Traversal uses the renderer-owned tree directly. This keeps walkers valid even when a
    // compatibility library replaces convenience accessors on Node.prototype.
    // https://dom.spec.whatwg.org/#traversal
    const FILTER_ACCEPT = 1;
    const FILTER_REJECT = 2;
    const FILTER_SKIP = 3;
    const traversalConstants = {
        FILTER_ACCEPT, FILTER_REJECT, FILTER_SKIP,
        SHOW_ALL: 0xFFFFFFFF,
        SHOW_ELEMENT: 0x1,
        SHOW_ATTRIBUTE: 0x2,
        SHOW_TEXT: 0x4,
        SHOW_CDATA_SECTION: 0x8,
        SHOW_ENTITY_REFERENCE: 0x10,
        SHOW_ENTITY: 0x20,
        SHOW_PROCESSING_INSTRUCTION: 0x40,
        SHOW_COMMENT: 0x80,
        SHOW_DOCUMENT: 0x100,
        SHOW_DOCUMENT_TYPE: 0x200,
        SHOW_DOCUMENT_FRAGMENT: 0x400,
        SHOW_NOTATION: 0x800
    };
    class NodeFilter { constructor() { throw new TypeError('Illegal constructor'); } }
    for (const [name, value] of Object.entries(traversalConstants)) {
        Object.defineProperty(NodeFilter, name, { enumerable: true, value });
        Object.defineProperty(NodeFilter.prototype, name, { enumerable: true, value });
    }

    const traversalChildren = node => list(host('children', node.__id));
    const traversalParent = node => wrap(host('parent', node.__id));
    const traversalChild = (node, first) => {
        const children = traversalChildren(node);
        return children.length ? children[first ? 0 : children.length - 1] : null;
    };
    const traversalSibling = (node, next) => {
        const parent = traversalParent(node);
        if (!parent) return null;
        const siblings = traversalChildren(parent);
        const index = siblings.findIndex(sibling => sibling === node);
        if (index < 0) return null;
        return siblings[index + (next ? 1 : -1)] || null;
    };

    const treeWalkerToken = {};
    class TreeWalker {
        constructor(token, root, whatToShow, filter) {
            if (token !== treeWalkerToken) throw new TypeError('Illegal constructor');
            this.root = root;
            this.whatToShow = Number(whatToShow) >>> 0;
            this.filter = filter;
            this.__current = root;
            this.__filterActive = false;
        }
        get currentNode() { return this.__current; }
        set currentNode(node) {
            if (!(node instanceof Node)) throw new TypeError('currentNode must be a Node');
            this.__current = node;
        }
        __accept(node) {
            const mask = node.nodeType > 0 && node.nodeType <= 32
                ? (1 << (node.nodeType - 1)) >>> 0 : 0;
            if ((this.whatToShow & mask) === 0) return FILTER_SKIP;
            if (this.filter == null) return FILTER_ACCEPT;
            if (this.__filterActive)
                throw new DOMException('The traversal filter is already active', 'InvalidStateError');
            this.__filterActive = true;
            try {
                const callback = typeof this.filter === 'function'
                    ? this.filter : this.filter.acceptNode;
                if (typeof callback !== 'function') throw new TypeError('filter.acceptNode is not callable');
                return Number(callback.call(this.filter, node)) & 0xFFFF;
            } finally {
                this.__filterActive = false;
            }
        }
        parentNode() {
            let node = this.__current;
            while (node && node !== this.root) {
                node = traversalParent(node);
                if (node && this.__accept(node) === FILTER_ACCEPT) return this.__current = node;
            }
            return null;
        }
        __traverseChildren(first) {
            const origin = this.__current;
            let node = traversalChild(origin, first);
            while (node) {
                const result = this.__accept(node);
                if (result === FILTER_ACCEPT) return this.__current = node;
                if (result === FILTER_SKIP) {
                    const child = traversalChild(node, first);
                    if (child) { node = child; continue; }
                }
                while (node) {
                    const sibling = traversalSibling(node, first);
                    if (sibling) { node = sibling; break; }
                    const parent = traversalParent(node);
                    if (!parent || parent === this.root || parent === origin) return null;
                    node = parent;
                }
            }
            return null;
        }
        firstChild() { return this.__traverseChildren(true); }
        lastChild() { return this.__traverseChildren(false); }
        __traverseSiblings(next) {
            let node = this.__current;
            if (node === this.root) return null;
            while (true) {
                let sibling = traversalSibling(node, next);
                while (sibling) {
                    node = sibling;
                    const result = this.__accept(node);
                    if (result === FILTER_ACCEPT) return this.__current = node;
                    sibling = traversalChild(node, next);
                    if (result === FILTER_REJECT || !sibling) sibling = traversalSibling(node, next);
                }
                node = traversalParent(node);
                if (!node || node === this.root) return null;
                if (this.__accept(node) === FILTER_ACCEPT) return null;
            }
        }
        previousSibling() { return this.__traverseSiblings(false); }
        nextSibling() { return this.__traverseSiblings(true); }
        previousNode() {
            let node = this.__current;
            while (node !== this.root) {
                let sibling = traversalSibling(node, false);
                while (sibling) {
                    node = sibling;
                    let result = this.__accept(node);
                    let child = traversalChild(node, false);
                    while (result !== FILTER_REJECT && child) {
                        node = child;
                        result = this.__accept(node);
                        child = traversalChild(node, false);
                    }
                    if (result === FILTER_ACCEPT) return this.__current = node;
                    sibling = traversalSibling(node, false);
                }
                if (node === this.root || !traversalParent(node)) return null;
                node = traversalParent(node);
                if (node === this.root) return null;
                if (this.__accept(node) === FILTER_ACCEPT) return this.__current = node;
            }
            return null;
        }
        nextNode() {
            let node = this.__current;
            let result = FILTER_ACCEPT;
            while (true) {
                let child = traversalChild(node, true);
                while (result !== FILTER_REJECT && child) {
                    node = child;
                    result = this.__accept(node);
                    if (result === FILTER_ACCEPT) return this.__current = node;
                    child = traversalChild(node, true);
                }
                let sibling = null;
                let temporary = node;
                while (temporary) {
                    if (temporary === this.root) return null;
                    sibling = traversalSibling(temporary, true);
                    if (sibling) break;
                    temporary = traversalParent(temporary);
                }
                if (!sibling) return null;
                node = sibling;
                result = this.__accept(node);
                if (result === FILTER_ACCEPT) return this.__current = node;
            }
        }
    }
