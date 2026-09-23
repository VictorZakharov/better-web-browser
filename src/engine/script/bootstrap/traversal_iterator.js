    // DOM Standard §6.1: the reference is a position before or after a node, not
    // merely the last accepted node. Rejected nodes still advance the candidate.
    // https://dom.spec.whatwg.org/#interface-nodeiterator
    const nodeIteratorToken = {};
    const liveNodeIterators = new Set();
    const traversalNext = (node, root) => {
        const child = traversalChild(node, true);
        if (child) return child;
        while (node && node !== root) {
            const sibling = traversalSibling(node, true);
            if (sibling) return sibling;
            node = traversalParent(node);
        }
        return null;
    };
    const traversalPrevious = (node, root) => {
        if (node === root) return null;
        let sibling = traversalSibling(node, false);
        if (!sibling) return traversalParent(node);
        for (let child; (child = traversalChild(sibling, false));) sibling = child;
        return sibling;
    };
    const traversalFollowingOutside = (node, root) => {
        while (node && node !== root) {
            const sibling = traversalSibling(node, true);
            if (sibling) return sibling;
            node = traversalParent(node);
        }
        return null;
    };
    const adjustedIteratorPointer = (pointer, iterator, removed) => {
        if (!pointer || !removed.contains(pointer.node) || removed.contains(iterator.root))
            return pointer;
        if (pointer.before) {
            const following = traversalFollowingOutside(removed, iterator.root);
            if (following) return {node: following, before: true};
        }
        const previous = traversalSibling(removed, false);
        if (!previous) return {node: traversalParent(removed), before: false};
        let last = previous;
        for (let child; (child = traversalChild(last, false));) last = child;
        return {node: last, before: false};
    };
    const iteratorPreRemove = removed => {
        for (const reference of liveNodeIterators) {
            const iterator = reference.deref();
            if (!iterator) { liveNodeIterators.delete(reference); continue; }
            iterator.__reference = adjustedIteratorPointer(iterator.__reference, iterator, removed);
            iterator.__candidate = adjustedIteratorPointer(iterator.__candidate, iterator, removed);
        }
    };
    class NodeIterator {
        constructor(token, root, whatToShow, filter) {
            if (token !== nodeIteratorToken) throw new TypeError('Illegal constructor');
            Object.defineProperties(this, {
                root: {enumerable: true, value: root},
                whatToShow: {enumerable: true, value: Number(whatToShow) >>> 0},
                filter: {enumerable: true, value: filter}
            });
            this.__reference = {node: root, before: true};
            this.__candidate = null;
            this.__filterActive = false;
            liveNodeIterators.add(new WeakRef(this));
        }
        get referenceNode() { return this.__reference.node; }
        get pointerBeforeReferenceNode() { return this.__reference.before; }
        __traverse(next) {
            if (this.__filterActive)
                throw new DOMException('The traversal filter is already active', 'InvalidStateError');
            this.__candidate = {...this.__reference};
            try {
                while (true) {
                    const pointer = this.__candidate;
                    if (next) {
                        if (pointer.before) pointer.before = false;
                        else {
                            const following = traversalNext(pointer.node, this.root);
                            if (!following) return null;
                            pointer.node = following;
                        }
                    } else if (pointer.before) {
                        const preceding = traversalPrevious(pointer.node, this.root);
                        if (!preceding) return null;
                        pointer.node = preceding;
                    } else pointer.before = true;
                    const node = pointer.node;
                    if (traversalFilterNode(this, node) === FILTER_ACCEPT) {
                        this.__reference = {...this.__candidate};
                        return node;
                    }
                }
            } finally { this.__candidate = null; }
        }
        nextNode() { return this.__traverse(true); }
        previousNode() { return this.__traverse(false); }
        detach() {}
    }
