    // Geometry Interfaces values are snapshots, not handles into retained layout.
    class DOMRect extends DOMRectReadOnly {
        static fromRect(other = {}) {
            if (other != null && typeof other !== 'object' && typeof other !== 'function')
                throw new TypeError('DOMRectInit must be a dictionary');
            return new DOMRect(other?.x, other?.y, other?.width, other?.height);
        }
    }
    for (const field of ['x', 'y', 'width', 'height']) {
        Object.defineProperty(DOMRect.prototype, field, {
            configurable: true, enumerable: true,
            get() { return branded(domRects, this)[field]; },
            set(value) { branded(domRects, this)[field] = +value; }
        });
    }
    Object.defineProperty(DOMRect.prototype, Symbol.toStringTag,
        {value: 'DOMRect', configurable: true});
    const rectLists = new WeakMap();
    class DOMRectList {
        constructor() { throw new TypeError('Illegal constructor'); }
        get length() { return branded(rectLists, this).length; }
        item(index) {
            const list = branded(rectLists, this);
            if (arguments.length === 0) throw new TypeError('item requires an index');
            return list[Number(index) >>> 0] ?? null;
        }
        [Symbol.iterator]() { return branded(rectLists, this)[Symbol.iterator](); }
    }
    Object.defineProperty(DOMRectList.prototype, Symbol.toStringTag,
        {value: 'DOMRectList', configurable: true});
    for (const name of ['length', 'item']) Object.defineProperty(DOMRectList.prototype, name,
        {...Object.getOwnPropertyDescriptor(DOMRectList.prototype, name), enumerable: true});
    function makeRectList(rects) {
        const list = Object.create(DOMRectList.prototype);
        rectLists.set(list, rects);
        for (let i = 0; i < rects.length; i++) Object.defineProperty(list, i, {
            value: rects[i], enumerable: true, configurable: true
        });
        return list;
    }
    function clientRectsFromHost(operation, node, start = 0, end = 0) {
        const values = host(operation, nodeId(node), start, end,
            viewportScrollX !== 0 || viewportScrollY !== 0) || [];
        return values.map(([x, y, width, height, fixed]) => new DOMRect(
            x - (fixed ? 0 : viewportScrollX), y - (fixed ? 0 : viewportScrollY), width, height));
    }
    function elementClientRects(element) {
        if (!(element instanceof Element)) throw new TypeError('Illegal invocation');
        return clientRectsFromHost('clientRects', element);
    }
    function boundingRect(rects) {
        if (!rects.length) return new DOMRect();
        const nonempty = rects.filter(rect => rect.width !== 0 && rect.height !== 0);
        if (!nonempty.length) return DOMRect.fromRect(rects[0]);
        let left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
        for (const rect of nonempty) {
            left = Math.min(left, rect.left); top = Math.min(top, rect.top);
            right = Math.max(right, rect.right); bottom = Math.max(bottom, rect.bottom);
        }
        return new DOMRect(left, top, right - left, bottom - top);
    }
    function rangeClientRects(range) {
        if (!(range instanceof Range)) throw new TypeError('Illegal invocation');
        if (!range.startContainer.isConnected || !range.endContainer.isConnected) return [];
        const root = rangeRoot(range.startContainer);
        if (root !== rangeRoot(range.endContainer)) return [];
        const selected = node => {
            if (!node) return false;
            const parent = node.parentNode;
            if (!parent) return false;
            const index = nodeIndex(node);
            return compareBoundaries(range.startContainer, range.startOffset, parent, index) <= 0 &&
                compareBoundaries(parent, index + 1, range.endContainer, range.endOffset) <= 0;
        };
        const rects = [];
        const stack = [range.commonAncestorContainer];
        while (stack.length) {
            const node = stack.pop();
            if (node instanceof Element && selected(node)) {
                // A display:contents parent contributes no box; retain its selected child boxes.
                const parentHasSelectedBox = node.parentNode instanceof Element && selected(node.parentNode) &&
                    elementClientRects(node.parentNode).length !== 0;
                if (!parentHasSelectedBox) rects.push(...elementClientRects(node));
            }
            if (node.nodeType === 3) {
                const startsBeforeEnd = compareBoundaries(range.startContainer, range.startOffset,
                    node, node.length) <= 0;
                const endsAfterStart = compareBoundaries(node, 0,
                    range.endContainer, range.endOffset) <= 0;
                if (startsBeforeEnd && endsAfterStart) rects.push(...clientRectsFromHost(
                    'rangeTextRects', node,
                    node === range.startContainer ? range.startOffset : 0,
                    node === range.endContainer ? range.endOffset : node.length));
            }
            for (let i = node.childNodes.length - 1; i >= 0; i--) stack.push(node.childNodes[i]);
        }
        return rects;
    }
    Object.defineProperties(Element.prototype, {
        getClientRects: { configurable: true, enumerable: true,
            writable: true,
            value() { return makeRectList(elementClientRects(this)); } },
        getBoundingClientRect: { configurable: true, enumerable: true,
            writable: true,
            value() { return boundingRect(elementClientRects(this)); } }
    });
    Object.assign(windowObject, {DOMRect, DOMRectList});
