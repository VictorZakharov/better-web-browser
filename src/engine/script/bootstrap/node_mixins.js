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
            return withScriptMutationBatch(() => {
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
            });
        },
        querySelector(selector) { return wrap(host('query', nodeId(this), String(selector))); },
        querySelectorAll(selector) { return list(host('queryAll', nodeId(this), String(selector))); }
    };
    const installUnscopables = (prototype, names) => {
        // Web IDL gives each interface its own null-prototype unscopables object.
        let excluded = Object.getOwnPropertyDescriptor(prototype, Symbol.unscopables)?.value;
        if (!excluded) Object.defineProperty(prototype, Symbol.unscopables, {
            value: excluded = Object.create(null), configurable: true
        });
        for (const name of names) excluded[name] = true;
    };
    const installParentNodeMembers = prototype => {
        Object.defineProperties(prototype, Object.getOwnPropertyDescriptors(parentNodeMembers));
        installUnscopables(prototype, ['append', 'prepend', 'replaceChildren']);
    };
    const childNodeMembers = {
        before(...items) {
            const parent = this.parentNode;
            if (!parent) return;
            const itemNodes = items.filter(item => isNode(item));
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
            const itemNodes = items.filter(item => isNode(item));
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
            const itemNodes = items.filter(item => isNode(item));
            while (viableNextSibling && itemNodes.includes(viableNextSibling))
                viableNextSibling = viableNextSibling.nextSibling;
            const replacement = convertNodes(items);
            if (this.parentNode === parent) parent.replaceChild(replacement, this);
            else parent.insertBefore(replacement, viableNextSibling);
        }
    };
    const installChildNodeMembers = prototype => {
        Object.defineProperties(prototype, Object.getOwnPropertyDescriptors(childNodeMembers));
        installUnscopables(prototype, ['before', 'after', 'replaceWith', 'remove']);
    };
