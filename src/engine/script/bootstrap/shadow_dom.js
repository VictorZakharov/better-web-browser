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
        get slotAssignment() { return 'named'; }
        get host() { return wrap(host('shadowHost', nodeId(this))); }
        getElementById(id) { return wrap(host('byId', nodeId(this), String(id))); }
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
    }

    const trackedShadowRoots = new Set();
    const rootsByHost = new WeakMap();
    shadowRootForTraversal = node => node instanceof Element ? rootsByHost.get(node) || null : null;
    const slotAssignments = new WeakMap();
    let slotCheckQueued = false;
    const sameNodeList = (left, right) => left.length === right.length &&
        left.every((node, index) => node === right[index]);
    scheduleSlotChangeCheck = () => {
        if (slotCheckQueued) return;
        slotCheckQueued = true;
        host('queueMicrotask', () => {
            slotCheckQueued = false;
            for (const root of trackedShadowRoots) {
                for (const slot of root.querySelectorAll('slot')) {
                    const assigned = slot.assignedNodes();
                    const previous = slotAssignments.get(slot) || [];
                    slotAssignments.set(slot, assigned);
                    if (!sameNodeList(previous, assigned))
                        slot.dispatchEvent(new Event('slotchange', { bubbles: true }));
                }
            }
        });
    };
    const nativeAttachShadow = Element.prototype.attachShadow;
    Element.prototype.attachShadow = function(init) {
        const root = nativeAttachShadow.call(this, init);
        trackedShadowRoots.add(root);
        rootsByHost.set(this, root);
        return root;
    };
