    // HTML's focused area is an internal DOM anchor. DocumentOrShadowRoot.activeElement
    // retargets that anchor into the caller's tree; it does not expose a descendant
    // of a closed (or open) shadow root through document.activeElement.
    // https://html.spec.whatwg.org/multipage/interaction.html#dom-documentorshadowroot-activeelement
    const focusedAreas = new WeakMap();
    focusedAreaForDocument = owner => focusedAreas.get(owner) || null;
    setFocusedAreaForDocument = (owner, area) => {
        if (area) focusedAreas.set(owner, area);
        else focusedAreas.delete(owner);
    };
    repairDetachedFocus = () => {
        const focused = focusedAreas.get(document);
        if (!focused || (focused.ownerDocument === document && focused.isConnected)) return;
        host('setFocus', 0);
        focusedAreas.delete(document);
    };
    activeElementForRoot = root => {
        const owner = root instanceof Document ? root : root.ownerDocument;
        const candidate = retarget(focusedAreaForDocument(owner) || owner, root);
        if (!candidate || candidate.getRootNode() !== root) return null;
        return candidate instanceof Document
            ? candidate.body || candidate.documentElement || null : candidate;
    };

    // Inertness follows the flat tree, not merely DOM parentage. An assigned
    // node can be inside an inert slot subtree even for a closed shadow root.
    // https://html.spec.whatwg.org/multipage/interaction.html#inert-subtrees
    const focusTreeParent = node => wrap(host('assignedSlotForTraversal', nodeId(node))) ||
        node.parentNode || (node instanceof ShadowRoot ? node.host : null);
    const isInertOrHiddenForFocus = element => {
        for (let node = element; node; node = focusTreeParent(node)) {
            if (node instanceof Element &&
                (node.hasAttribute('inert') || node.hasAttribute('hidden') ||
                    (node.hasAttribute('popover') && !node.matches(':popover-open')))) return true;
        }
        return false;
    };
    const isFocusableArea = element => {
        if (!(element instanceof Element) || !element.isConnected ||
            !host('isInComposedTree', nodeId(element)) ||
            isInertOrHiddenForFocus(element) ||
            element.matches(':disabled')) return false;
        if (element.localName === 'input' && element.type === 'hidden') return false;
        if (parsedTabindex(element) !== null) return true;
        if (element.localName === 'summary') return isSummaryForParentDetails(element);
        if (['input', 'button', 'select', 'textarea', 'iframe',
            'object', 'embed'].includes(element.localName)) return true;
        if (['a', 'area'].includes(element.localName))
            return element.hasAttribute('href');
        if (['audio', 'video'].includes(element.localName))
            return element.hasAttribute('controls');
        return element instanceof HTMLElement && editingHost(element) === element;
    };
    const focusDelegate = root => {
        const descendants = root.querySelectorAll('*');
        // HTML tries autofocus candidates before ordinary focusable descendants.
        for (const autofocusOnly of [true, false]) {
            for (const descendant of descendants) {
                if (autofocusOnly && !descendant.hasAttribute('autofocus')) continue;
                const nestedRoot = shadowRootForTraversal(descendant);
                const candidate = nestedRoot?.delegatesFocus
                    ? focusDelegate(nestedRoot) : (isFocusableArea(descendant) ? descendant : null);
                if (candidate) return candidate;
            }
        }
        return null;
    };
    const focusEvent = (target, type, relatedTarget, bubbles, trusted) => {
        const event = new FocusEvent(type, {
            bubbles, composed: true, relatedTarget, view: target.ownerDocument.defaultView
        });
        target.dispatchEvent(trusted ? markTrusted(event) : event);
    };
    const dispatchFocusTransition = (previous, next, trusted = false) => {
        if (previous === next) return;
        if (previous) {
            focusEvent(previous, 'blur', next, false, trusted);
            focusEvent(previous, 'focusout', next, true, trusted);
        }
        if (next) {
            focusEvent(next, 'focus', previous, false, trusted);
            focusEvent(next, 'focusin', previous, true, trusted);
        }
    };
    focusTargetForElement = element => {
        if (!(element instanceof Element)) return null;
        const root = shadowRootForTraversal(element);
        if (!root?.delegatesFocus)
            return isFocusableArea(element) ? element : null;
        const previous = focusedAreaForDocument(element.ownerDocument);
        if (previous && shadowIncludingContains(root, previous)) return previous;
        return focusDelegate(root);
    };
    focusElement = element => {
        const owner = element.ownerDocument;
        const previous = focusedAreaForDocument(owner);
        const next = focusTargetForElement(element);
        if (!next || next === previous) return;
        host('setFocus', nodeId(next));
        setFocusedAreaForDocument(owner, next);
        dispatchFocusTransition(previous, next);
    };
    blurElement = element => {
        const owner = element.ownerDocument;
        const previous = focusedAreaForDocument(owner);
        const root = shadowRootForTraversal(element);
        if (previous !== element &&
            !(root?.delegatesFocus && previous && shadowIncludingContains(root, previous)))
            return;
        host('setFocus', 0);
        setFocusedAreaForDocument(owner, null);
        dispatchFocusTransition(previous, null);
    };
