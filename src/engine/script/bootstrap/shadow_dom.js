    // Shadow roots remain separate node trees for DOM APIs. Rendering uses the native composed
    // tree, while these wrappers expose the DOM Standard identity and slot-distribution contract.
    // https://dom.spec.whatwg.org/#shadow-trees
    const shadowRootConstructionToken = {};
    class ShadowRoot extends DocumentFragment {
        constructor(id, type, name, localName, namespaceURI, token) {
            if (token !== shadowRootConstructionToken) throw new TypeError('Illegal constructor');
            super(id, type, name, localName, namespaceURI);
        }
        get mode() { return host('shadowMode', nodeId(this)); }
        get delegatesFocus() { return !!host('shadowDelegatesFocus', nodeId(this)); }
        get serializable() { return !!host('shadowSerializable', nodeId(this)); }
        get clonable() { return !!host('shadowClonable', nodeId(this)); }
        get slotAssignment() { return host('shadowSlotAssignment', nodeId(this)); }
        get customElementRegistry() { return shadowRegistryFor(this); }
        get host() { return wrap(host('shadowHost', nodeId(this))); }
        get activeElement() { return activeElementForRoot(this); }
        getElementById(id) { return wrap(host('byId', nodeId(this), String(id))); }
        elementFromPoint(x, y) {
            if (arguments.length < 2) throw new TypeError('elementFromPoint requires two coordinates');
            return pointElements(this, x, y)[0] || null;
        }
        elementsFromPoint(x, y) {
            if (arguments.length < 2) throw new TypeError('elementsFromPoint requires two coordinates');
            return pointElements(this, x, y);
        }
        get innerHTML() { return host('innerHtmlGet', nodeId(this)); }
        set innerHTML(value) { replaceElementInnerHtml(this, value); }
    }
    Object.defineProperty(ShadowRoot.prototype, Symbol.toStringTag,
        { value: 'ShadowRoot', configurable: true });
    installEventHandlerAttributes(ShadowRoot.prototype);

    class HTMLSlotElement extends HTMLElement {
        get name() { return this.getAttribute('name') || ''; }
        set name(value) { this.setAttribute('name', value); }
        assignedNodes(options = {}) {
            return list(host('assignedNodes', nodeId(this), !!Object(options).flatten));
        }
        assignedElements(options = {}) {
            return this.assignedNodes(options).filter(node => node.nodeType === 1);
        }
        assign(...nodes) {
            if (nodes.some(node => !(node instanceof Element || node instanceof Text)))
                throw new TypeError('slot.assign requires Element or Text nodes');
            // The native assignment can evict a node from a different manual
            // slot (possibly in another root); inspect every affected slot.
            for (const slot of list(host('assignSlot', nodeId(this), ...nodes.map(nodeId))))
                checkSlotAssignment(slot);
        }
    }

    const rootsByHost = new WeakMap();
    shadowRootForTraversal = node => nativeNodeType(node) === 1
        ? rootsByHost.get(node) || null : null;
    const parserFallbackInitialized = new WeakSet();
    const registerShadowRootForTraversal = (element, rootId, parserCreated = false) => {
        if (!rootId || nativeNodeType(element) !== 1) return;
        const root = wrap(rootId);
        if (!root) return;
        rootsByHost.set(element, root);
        if (!observedSlots.has(root)) scheduleSlotChangeCheck(root);
        if (parserCreated && !parserFallbackInitialized.has(root)) {
            parserFallbackInitialized.add(root);
            // The parser inserts fallback children after attaching a declarative
            // root. They may already exist when its host is first wrapped.
            for (const slot of shadowSlots(root)) {
                if (!slotNodes(slot).length && slotChildren(slot).length) signalSlot(slot);
            }
        }
    };
    const slotAssignments = new WeakMap();
    const slotFallbackChildren = new WeakMap();
    const observedSlots = new WeakMap();
    // The document's native node quota also bounds distinct slot identities in
    // this realm. Fail explicitly if a future document limit outgrows this set.
    const maxSignalSlots = 100000;
    const signalSlots = new Set();
    let queueSignalSlotsMicrotask = () => {};
    // Internal distribution must not call author-replaceable DOM methods. The
    // bridge operations below use native node IDs even for closed/detached roots.
    const shadowSlots = root => list(host('queryAll', nodeId(root), 'slot'));
    const slotNodes = slot => list(host('assignedNodes', nodeId(slot), false));
    const slotChildren = slot => list(host('children', nodeId(slot)));
    const nativeParent = node => wrap(host('parent', nodeId(node)));
    const nativeRoot = node => wrap(host('rootNode', nodeId(node), false));
    const nativeEventDispatch = EventTarget.prototype.dispatchEvent;
    const applyNative = Reflect.apply;
    const sameNodeList = (left, right) => left.length === right.length &&
        left.every((node, index) => node === right[index]);
    const signalSlot = slot => {
        if (signalSlots.has(slot)) return;
        if (signalSlots.size >= maxSignalSlots)
            throw new RangeError('Slot signal budget exceeded');
        signalSlots.add(slot);
        queueSignalSlotsMicrotask();
    };
    const checkSlotAssignment = slot => {
        const assigned = slotNodes(slot);
        const previous = slotAssignments.get(slot) || [];
        const fallback = slotChildren(slot);
        const hadFallbackSnapshot = slotFallbackChildren.has(slot);
        const previousFallback = slotFallbackChildren.get(slot) || [];
        // A fallback child insertion/removal signals a slot only while it has no
        // assigned nodes. Compare now, not at delivery: an assignment changed
        // and restored in one task must still yield one coalesced slotchange.
        if (!sameNodeList(previous, assigned) ||
            (hadFallbackSnapshot && !assigned.length &&
                !sameNodeList(previousFallback, fallback)))
            signalSlot(slot);
        slotAssignments.set(slot, assigned);
        slotFallbackChildren.set(slot, fallback);
    };
    const checkRootSlots = root => {
        const current = shadowSlots(root);
        const currentSet = new Set(current);
        // Removing a slot from a shadow tree can clear its old assignment;
        // the removed slot itself is still the signal target.
        for (const slot of observedSlots.get(root) || []) {
            if (!currentSet.has(slot)) checkSlotAssignment(slot);
        }
        observedSlots.set(root, current);
        for (const slot of current) checkSlotAssignment(slot);
    };
    const subtreeContainsSlot = nodes => nodes.some(node =>
        host('matches', nodeId(node), 'slot') || !!host('queryAll', nodeId(node), 'slot'));
    scheduleSlotChangeCheck = (target, type, details = {}) => {
        if (!target || type === 'characterData') return;
        if (type === 'attributes') {
            if (details.attributeNamespace != null) return;
            if (details.attributeName === 'slot') {
                const root = shadowRootForTraversal(nativeParent(target));
                if (root) checkRootSlots(root);
            }
            else if (details.attributeName === 'name' && target instanceof HTMLSlotElement) {
                const root = nativeRoot(target);
                if (root instanceof ShadowRoot) checkRootSlots(root);
            }
            return;
        }
        if (type === 'childList') {
            // Mutating a host's direct light children can redistribute all its
            // slots. A fallback child mutation can affect only its parent slot.
            const hosted = shadowRootForTraversal(target);
            if (hosted) checkRootSlots(hosted);
            const containing = target instanceof ShadowRoot ? target : nativeRoot(target);
            if (!(containing instanceof ShadowRoot)) return;
            if (target instanceof HTMLSlotElement) checkSlotAssignment(target);
            // Other deep mutations cannot affect assignment unless they insert
            // or remove a slot. Query only the changed subtrees, using native
            // operations so author overrides cannot hide a slot.
            if (subtreeContainsSlot(details.addedNodes || []) ||
                subtreeContainsSlot(details.removedNodes || []))
                checkRootSlots(containing);
            return;
        }
        // DOM mutation records identify the old and new parents separately for
        // moves. Inspect only their associated roots, including detached and
        // closed roots, instead of retaining/scanning every shadow tree.
        if (target instanceof ShadowRoot) {
            checkRootSlots(target);
            return;
        }
        const hosted = shadowRootForTraversal(target);
        if (hosted) checkRootSlots(hosted);
        const containing = nativeRoot(target);
        if (containing instanceof ShadowRoot && containing !== hosted)
            checkRootSlots(containing);
    };
    const nativeAttachShadow = Element.prototype.attachShadow;
    Element.prototype.attachShadow = function(init) {
        const root = nativeAttachShadow.call(this, init);
        registerShadowRootForTraversal(this, nodeId(root));
        return root;
    };
