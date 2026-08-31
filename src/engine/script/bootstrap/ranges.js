    // DOM Standard §5. Range boundary points retain wrapper identity and are validated before
    // application script can use them for editing or selection.
    const rangeNodeLength = node => {
        if (!(node instanceof Node)) throw new TypeError('A range boundary requires a Node');
        if (node instanceof DocumentType)
            throw new DOMException('DocumentType cannot be a range boundary', 'InvalidNodeTypeError');
        return node instanceof CharacterData ? node.length : node.childNodes.length;
    };
    const rangeRoot = node => node.getRootNode();
    const nodeIndex = node => {
        const parent = node.parentNode;
        return parent ? Array.from(parent.childNodes).indexOf(node) : -1;
    };
    const ancestorChain = node => {
        const chain = [];
        for (; node; node = node.parentNode) chain.push(node);
        return chain;
    };
    const compareBoundaries = (nodeA, offsetA, nodeB, offsetB) => {
        if (nodeA === nodeB) return Math.sign(offsetA - offsetB);
        if (rangeRoot(nodeA) !== rangeRoot(nodeB)) return null;
        const chainA = ancestorChain(nodeA);
        const chainB = ancestorChain(nodeB);
        const indexA = chainA.indexOf(nodeB);
        if (indexA >= 0) {
            const child = chainA[indexA - 1];
            return nodeIndex(child) < offsetB ? -1 : 1;
        }
        const indexB = chainB.indexOf(nodeA);
        if (indexB >= 0) {
            const child = chainB[indexB - 1];
            return offsetA <= nodeIndex(child) ? -1 : 1;
        }
        let a = chainA.length - 1;
        let b = chainB.length - 1;
        while (a >= 0 && b >= 0 && chainA[a] === chainB[b]) { a--; b--; }
        return Math.sign(nodeIndex(chainA[a]) - nodeIndex(chainB[b]));
    };
    const checkedBoundary = (node, offset) => {
        const length = rangeNodeLength(node);
        offset = Number(offset) >>> 0;
        if (offset > length) throw new DOMException('Offset exceeds node length', 'IndexSizeError');
        return { node, offset };
    };
    const boundaryParent = node => {
        const parent = node.parentNode;
        if (!parent) throw new DOMException('Node has no parent', 'InvalidNodeTypeError');
        return { parent, index: nodeIndex(node) };
    };
    const rangeRect = () => ({
        x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0,
        toJSON() { return this; }
    });

    const abstractRangeToken = {};
    class AbstractRange {
        constructor(token, start, end) {
            if (token !== abstractRangeToken) throw new TypeError('Illegal constructor');
            this.__start = start;
            this.__end = end;
        }
        get startContainer() { return this.__start.node; }
        get startOffset() { return this.__start.offset; }
        get endContainer() { return this.__end.node; }
        get endOffset() { return this.__end.offset; }
        get collapsed() {
            return this.__start.node === this.__end.node && this.__start.offset === this.__end.offset;
        }
    }

    class Range extends AbstractRange {
        constructor() {
            const boundary = { node: document, offset: 0 };
            super(abstractRangeToken, boundary, boundary);
        }
        get commonAncestorContainer() {
            const endAncestors = new Set(ancestorChain(this.endContainer));
            return ancestorChain(this.startContainer).find(node => endAncestors.has(node)) || null;
        }
        setStart(node, offset) {
            const boundary = checkedBoundary(node, offset);
            const order = compareBoundaries(boundary.node, boundary.offset,
                this.endContainer, this.endOffset);
            if (order === null || order > 0) this.__end = boundary;
            this.__start = boundary;
        }
        setEnd(node, offset) {
            const boundary = checkedBoundary(node, offset);
            const order = compareBoundaries(boundary.node, boundary.offset,
                this.startContainer, this.startOffset);
            if (order === null || order < 0) this.__start = boundary;
            this.__end = boundary;
        }
        setStartBefore(node) { const p = boundaryParent(node); this.setStart(p.parent, p.index); }
        setStartAfter(node) { const p = boundaryParent(node); this.setStart(p.parent, p.index + 1); }
        setEndBefore(node) { const p = boundaryParent(node); this.setEnd(p.parent, p.index); }
        setEndAfter(node) { const p = boundaryParent(node); this.setEnd(p.parent, p.index + 1); }
        collapse(toStart = false) {
            if (toStart) this.__end = this.__start;
            else this.__start = this.__end;
        }
        selectNode(node) {
            const p = boundaryParent(node);
            this.__start = { node: p.parent, offset: p.index };
            this.__end = { node: p.parent, offset: p.index + 1 };
        }
        selectNodeContents(node) {
            const length = rangeNodeLength(node);
            this.__start = { node, offset: 0 };
            this.__end = { node, offset: length };
        }
        compareBoundaryPoints(how, sourceRange) {
            if (!(sourceRange instanceof Range)) throw new TypeError('sourceRange must be a Range');
            let a, b;
            if (how === Range.START_TO_START) { a = this.__start; b = sourceRange.__start; }
            else if (how === Range.START_TO_END) { a = this.__end; b = sourceRange.__start; }
            else if (how === Range.END_TO_END) { a = this.__end; b = sourceRange.__end; }
            else if (how === Range.END_TO_START) { a = this.__start; b = sourceRange.__end; }
            else throw new DOMException('Unknown boundary comparison', 'NotSupportedError');
            const order = compareBoundaries(a.node, a.offset, b.node, b.offset);
            if (order === null) throw new DOMException('Ranges have different roots', 'WrongDocumentError');
            return order;
        }
        _sameContainerContents(extract) {
            const fragment = document.createDocumentFragment();
            if (this.collapsed || this.startContainer !== this.endContainer) return fragment;
            const container = this.startContainer;
            if (container instanceof CharacterData) {
                const clone = container.cloneNode(false);
                clone.data = container.data.slice(this.startOffset, this.endOffset);
                fragment.appendChild(clone);
                if (extract) container.replaceData(this.startOffset, this.endOffset - this.startOffset, '');
            } else {
                const selected = Array.from(container.childNodes)
                    .slice(this.startOffset, this.endOffset);
                for (const node of selected) fragment.appendChild(extract ? node : node.cloneNode(true));
            }
            if (extract) this.__end = this.__start;
            return fragment;
        }
        deleteContents() {
            if (this.collapsed || this.startContainer !== this.endContainer) return;
            if (this.startContainer instanceof CharacterData) {
                this.startContainer.replaceData(this.startOffset,
                    this.endOffset - this.startOffset, '');
            } else {
                const selected = Array.from(this.startContainer.childNodes)
                    .slice(this.startOffset, this.endOffset);
                for (const node of selected) node.remove();
            }
            this.__end = this.__start;
        }
        extractContents() { return this._sameContainerContents(true); }
        cloneContents() { return this._sameContainerContents(false); }
        insertNode(node) {
            if (!(node instanceof Node)) throw new TypeError('insertNode requires a Node');
            let parent = this.startContainer;
            let reference;
            if (parent instanceof CharacterData) {
                const suffix = document.createTextNode(parent.data.slice(this.startOffset));
                parent.data = parent.data.slice(0, this.startOffset);
                parent.parentNode.insertBefore(suffix, parent.nextSibling);
                reference = suffix;
                parent = parent.parentNode;
            } else reference = parent.childNodes[this.startOffset] || null;
            parent.insertBefore(node, reference);
        }
        surroundContents(newParent) {
            if (!(newParent instanceof Node)) throw new TypeError('surroundContents requires a Node');
            const fragment = this.extractContents();
            this.insertNode(newParent);
            newParent.appendChild(fragment);
            this.selectNode(newParent);
        }
        cloneRange() {
            const clone = new Range();
            clone.__start = { ...this.__start };
            clone.__end = { ...this.__end };
            return clone;
        }
        detach() {}
        isPointInRange(node, offset) {
            const point = checkedBoundary(node, offset);
            if (rangeRoot(point.node) !== rangeRoot(this.startContainer)) return false;
            return compareBoundaries(point.node, point.offset, this.startContainer, this.startOffset) >= 0 &&
                compareBoundaries(point.node, point.offset, this.endContainer, this.endOffset) <= 0;
        }
        comparePoint(node, offset) {
            const point = checkedBoundary(node, offset);
            if (rangeRoot(point.node) !== rangeRoot(this.startContainer))
                throw new DOMException('Point has a different root', 'WrongDocumentError');
            if (compareBoundaries(point.node, point.offset, this.startContainer, this.startOffset) < 0) return -1;
            return compareBoundaries(point.node, point.offset, this.endContainer, this.endOffset) > 0 ? 1 : 0;
        }
        intersectsNode(node) {
            if (!(node instanceof Node) || rangeRoot(node) !== rangeRoot(this.startContainer)) return false;
            const parent = node.parentNode;
            if (!parent) return true;
            const index = nodeIndex(node);
            return compareBoundaries(parent, index + 1, this.startContainer, this.startOffset) > 0 &&
                compareBoundaries(parent, index, this.endContainer, this.endOffset) < 0;
        }
        createContextualFragment(markup) {
            const fragment = document.createDocumentFragment();
            const context = this.startContainer.nodeType === 1
                ? this.startContainer : this.startContainer.parentElement;
            const holder = document.createElement(context?.localName || 'body');
            holder.innerHTML = String(markup);
            while (holder.firstChild) fragment.appendChild(holder.firstChild);
            return fragment;
        }
        getClientRects() { return []; }
        getBoundingClientRect() { return rangeRect(); }
        toString() {
            if (this.collapsed) return '';
            if (this.startContainer === this.endContainer && this.startContainer instanceof CharacterData)
                return this.startContainer.data.slice(this.startOffset, this.endOffset);
            return this.cloneContents().textContent || '';
        }
    }
    for (const [name, value] of Object.entries({
        START_TO_START: 0, START_TO_END: 1, END_TO_END: 2, END_TO_START: 3
    })) {
        Object.defineProperty(Range, name, { value, enumerable: true });
        Object.defineProperty(Range.prototype, name, { value, enumerable: true });
    }

    class Selection {
        constructor() { this._ranges = []; this._backward = false; }
        get rangeCount() { return this._ranges.length; }
        get type() { return !this.rangeCount ? 'None' : this.isCollapsed ? 'Caret' : 'Range'; }
        get anchorNode() {
            return this.rangeCount
                ? (this._backward ? this._ranges[0].endContainer : this._ranges[0].startContainer)
                : null;
        }
        get anchorOffset() {
            return this.rangeCount
                ? (this._backward ? this._ranges[0].endOffset : this._ranges[0].startOffset)
                : 0;
        }
        get focusNode() {
            return this.rangeCount
                ? (this._backward ? this._ranges[0].startContainer : this._ranges[0].endContainer)
                : null;
        }
        get focusOffset() {
            return this.rangeCount
                ? (this._backward ? this._ranges[0].startOffset : this._ranges[0].endOffset)
                : 0;
        }
        get isCollapsed() { return !this.rangeCount || this._ranges[0].collapsed; }
        getRangeAt(index) {
            index = Number(index) >>> 0;
            if (index >= this.rangeCount) throw new DOMException('No range at index', 'IndexSizeError');
            return this._ranges[index];
        }
        addRange(range) {
            if (!(range instanceof Range)) throw new TypeError('addRange requires a Range');
            if (!this.rangeCount) this._ranges.push(range);
        }
        removeRange(range) {
            const index = this._ranges.indexOf(range);
            if (index < 0) throw new DOMException('Range is not selected', 'NotFoundError');
            this._ranges.splice(index, 1);
        }
        removeAllRanges() { this._ranges.length = 0; this._backward = false; }
        empty() { this.removeAllRanges(); }
        collapse(node, offset = 0) {
            if (node === null) return this.removeAllRanges();
            const range = new Range();
            range.setStart(node, offset);
            range.collapse(true);
            this._ranges = [range];
            this._backward = false;
        }
        setPosition(node, offset = 0) { this.collapse(node, offset); }
        collapseToStart() {
            if (!this.rangeCount) throw new DOMException('Selection is empty', 'InvalidStateError');
            this._ranges[0].collapse(true);
            this._backward = false;
        }
        collapseToEnd() {
            if (!this.rangeCount) throw new DOMException('Selection is empty', 'InvalidStateError');
            this._ranges[0].collapse(false);
            this._backward = false;
        }
        selectAllChildren(node) {
            const range = new Range();
            range.selectNodeContents(node);
            this._ranges = [range];
            this._backward = false;
        }
        setBaseAndExtent(anchorNode, anchorOffset, focusNode, focusOffset) {
            const anchor = checkedBoundary(anchorNode, anchorOffset);
            const focus = checkedBoundary(focusNode, focusOffset);
            const order = compareBoundaries(anchor.node, anchor.offset, focus.node, focus.offset);
            if (order === null)
                throw new DOMException('Selection points have different roots', 'WrongDocumentError');
            const range = new Range();
            if (order <= 0) {
                range.setStart(anchor.node, anchor.offset);
                range.setEnd(focus.node, focus.offset);
            } else {
                range.setStart(focus.node, focus.offset);
                range.setEnd(anchor.node, anchor.offset);
            }
            this._ranges = [range];
            this._backward = order > 0;
        }
        deleteFromDocument() { if (this.rangeCount) this._ranges[0].deleteContents(); }
        containsNode(node, allowPartialContainment = false) {
            if (!this.rangeCount) return false;
            if (allowPartialContainment) return this._ranges[0].intersectsNode(node);
            const parent = node.parentNode;
            if (!parent) return false;
            const index = nodeIndex(node);
            return this._ranges[0].isPointInRange(parent, index) &&
                this._ranges[0].isPointInRange(parent, index + 1);
        }
        toString() { return this.rangeCount ? this._ranges[0].toString() : ''; }
    }
    const documentSelection = new Selection();
    Document.prototype.createRange = function createRange() { return new Range(); };
    Document.prototype.getSelection = function getSelection() { return documentSelection; };
    windowObject.getSelection = () => documentSelection;
    windowObject.AbstractRange = AbstractRange;
    windowObject.Range = Range;
    windowObject.Selection = Selection;
