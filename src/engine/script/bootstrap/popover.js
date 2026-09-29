    // HTML Popover API: the attribute selects behavior, while showing is separate state.
    // https://html.spec.whatwg.org/multipage/popover.html
    const openPopovers = [];
    const popoverMetadata = new WeakMap();
    const popoverTransitions = new WeakSet();
    const pendingPopoverToggles = new WeakMap();
    let nextPopoverOrder = 1;
    let pointerDownPopover = null;
    let showingPopover = false;
    let hidingPopoverDepth = 0;

    const popoverModeFromValue = raw => {
        if (raw === null) return null;
        const value = raw.toLowerCase();
        return value === '' || value === 'auto' ? 'auto'
            : value === 'hint' ? 'hint' : 'manual';
    };
    const popoverMode = element => popoverModeFromValue(element.getAttribute('popover'));
    const popoverIsOpen = element => openPopovers.includes(element);
    const openedPopoverMode = element => popoverMetadata.get(element)?.mode ?? null;
    const popoverContains = (ancestor, node) => node instanceof Node && shadowIncludingContains(ancestor, node);
    const popoverStackContains = (ancestor, element) => {
        for (let current = element; current; current = popoverMetadata.get(current)?.parent)
            if (current === ancestor) return true;
        return false;
    };
    const topmostPopoverAncestor = (element, source) =>
        [...openPopovers].reverse().find(other => openedPopoverMode(other) !== 'manual' &&
            (popoverContains(other, element) || popoverContains(other, source))) || null;
    const popoverTopmostFor = node => {
        let clicked = null;
        for (let index = openPopovers.length - 1; index >= 0; index--)
            if (openedPopoverMode(openPopovers[index]) !== 'manual' &&
                popoverContains(openPopovers[index], node)) {
                clicked = openPopovers[index];
                break;
            }
        // The invoker of an open popover counts as clicking inside that popover.
        const invoker = node instanceof Element ? node.closest('button[popovertarget],input[popovertarget]') : null;
        const target = invoker ? popoverTargetFor(invoker) : null;
        return target && popoverIsOpen(target) && openedPopoverMode(target) !== 'manual' &&
            openPopovers.indexOf(target) > openPopovers.indexOf(clicked) ? target : clicked;
    };
    const popoverStateError = (element, showing, expectedMode = popoverMode(element)) => {
        if (expectedMode === null)
            throw new DOMException('Element has no popover attribute', 'NotSupportedError');
        if (popoverTransitions.has(element))
            throw new DOMException('Popover is already changing state', 'InvalidStateError');
        if (popoverIsOpen(element) === showing) return false;
        if (!element.isConnected || element.ownerDocument !== document ||
            (element instanceof HTMLDialogElement && element.__isModal) ||
            element === document.fullscreenElement)
            throw new DOMException('Popover cannot change state in this document', 'InvalidStateError');
        return true;
    };
    const checkContinuingShow = (element, mode) => {
        if (!element.isConnected || element.ownerDocument !== document ||
            popoverMode(element) !== mode || popoverIsOpen(element) ||
            (element instanceof HTMLDialogElement && element.__isModal) ||
            element === document.fullscreenElement)
            throw new DOMException('Popover changed while opening', 'InvalidStateError');
    };
    const queuePopoverToggle = (element, oldState, newState, source) => {
        const pending = pendingPopoverToggles.get(element);
        if (pending) {
            pending.newState = newState;
            pending.source = source;
            return;
        }
        const task = { oldState, newState, source };
        pendingPopoverToggles.set(element, task);
        queueMediaTask(() => {
            if (pendingPopoverToggles.get(element) !== task) return;
            pendingPopoverToggles.delete(element);
            element.dispatchEvent(new ToggleEvent('toggle', task));
        });
    };
    const popoverFocus = element => {
        const candidates = element.hasAttribute('autofocus')
            ? [element] : element.querySelectorAll('[autofocus]');
        for (const candidate of candidates) {
            if (focusTargetForElement(candidate)) {
                focusElement(candidate);
                break;
            }
        }
    };
    const popoverSource = options => {
        const source = options == null ? null : (Object(options).source ?? null);
        if (source !== null && !(source instanceof HTMLElement))
            throw new TypeError('Popover source must be an HTMLElement or null');
        return source;
    };
    const popoverHide = (element, { restoreFocus = true, fireEvents = true, source = null } = {}) => {
        if (!popoverIsOpen(element) || popoverTransitions.has(element)) return;
        popoverTransitions.add(element);
        hidingPopoverDepth++;
        try {
            // Stack ancestry is fixed when each popover opens. Moving an invoker
            // afterward must not detach its popover from the dismissible stack.
            if (openedPopoverMode(element) !== 'manual')
                for (const other of [...openPopovers].reverse())
                    if (other !== element && openedPopoverMode(other) !== 'manual' &&
                        popoverStackContains(element, other))
                        popoverHide(other, { restoreFocus: false, fireEvents });
            if (fireEvents) element.dispatchEvent(new ToggleEvent('beforetoggle', {
                oldState: 'open', newState: 'closed', source
            }));
            const previous = popoverMetadata.get(element)?.previousFocus;
            const focused = focusedAreaForDocument(element.ownerDocument);
            openPopovers.splice(openPopovers.indexOf(element), 1);
            popoverMetadata.delete(element);
            host('popoverSet', nodeId(element), 0);
            if (restoreFocus && previous?.isConnected && popoverContains(element, focused))
                focusElement(previous);
            if (fireEvents) queuePopoverToggle(element, 'open', 'closed', source);
        } finally {
            hidingPopoverDepth--;
            popoverTransitions.delete(element);
        }
    };
    const popoverShow = (element, source = null) => {
        if (showingPopover || hidingPopoverDepth)
            throw new DOMException('A popover is already changing state', 'InvalidStateError');
        const mode = popoverMode(element);
        if (!popoverStateError(element, true, mode)) return;
        popoverTransitions.add(element);
        showingPopover = true;
        try {
            if (!element.dispatchEvent(new ToggleEvent('beforetoggle', {
                cancelable: true, oldState: 'closed', newState: 'open', source
            }))) return;
            checkContinuingShow(element, mode);
            const ancestor = mode === 'manual' ? null : topmostPopoverAncestor(element, source);
            // An auto popover opened from a hint belongs to the hint stack.
            // Its unrelated auto peers stay open until that stack closes.
            const effectiveMode = mode === 'auto' && openedPopoverMode(ancestor) === 'hint'
                ? 'hint' : mode;
            if (mode !== 'manual') {
                for (const other of [...openPopovers].reverse()) {
                    const otherMode = openedPopoverMode(other);
                    if (otherMode === 'manual') continue;
                    if (effectiveMode === 'hint' && otherMode !== 'hint') continue;
                    if (ancestor && popoverStackContains(other, ancestor)) continue;
                    popoverHide(other, { restoreFocus: false });
                }
            }
            // HTML stores the previous focus only on the first dismissible
            // popover in a stack. Manual and nested popovers never restore it.
            checkContinuingShow(element, mode);
            const shouldRestoreFocus = mode !== 'manual' &&
                !openPopovers.some(other => openedPopoverMode(other) !== 'manual');
            const previousFocus = shouldRestoreFocus
                ? focusedAreaForDocument(element.ownerDocument) : null;
            openPopovers.push(element);
            popoverMetadata.set(element, { source, previousFocus, mode: effectiveMode, parent: ancestor });
            host('popoverSet', nodeId(element), nextPopoverOrder++);
            popoverFocus(element);
            queuePopoverToggle(element, 'closed', 'open', source);
        } finally {
            showingPopover = false;
            popoverTransitions.delete(element);
        }
    };
    function popoverAttributeChanged(element, namespace, name, oldValue, newValue) {
        if (namespace !== null || name !== 'popover' || oldValue === newValue ||
            !popoverIsOpen(element)) return;
        if (newValue === null || popoverMode(element) !== popoverModeFromValue(oldValue))
            popoverHide(element);
    }
    function disconnectPopoverTree(root) {
        for (const element of inclusiveElementDescendants(root))
            if (popoverIsOpen(element)) popoverHide(element, { restoreFocus: false, fireEvents: false });
    }

    Object.defineProperties(HTMLElement.prototype, {
        popover: {
            configurable: true, enumerable: true,
            get() { return popoverMode(this); },
            set(value) {
                if (value == null) this.removeAttribute('popover');
                else this.setAttribute('popover', String(value));
            }
        }
    });
    HTMLElement.prototype.showPopover = function(options = {}) {
        if (!(this instanceof HTMLElement)) throw new TypeError('Invalid popover receiver');
        popoverShow(this, popoverSource(options));
    };
    HTMLElement.prototype.hidePopover = function() {
        if (!(this instanceof HTMLElement)) throw new TypeError('Invalid popover receiver');
        if (popoverStateError(this, false)) popoverHide(this);
    };
    HTMLElement.prototype.togglePopover = function(forceOrOptions) {
        if (!(this instanceof HTMLElement)) throw new TypeError('Invalid popover receiver');
        const options = forceOrOptions !== null && typeof forceOrOptions === 'object'
            ? forceOrOptions : { force: forceOrOptions };
        const force = options.force === undefined ? !popoverIsOpen(this) : !!options.force;
        if (force) popoverShow(this, popoverSource(options));
        else if (popoverStateError(this, false)) popoverHide(this);
        return popoverIsOpen(this);
    };

    function popoverPointerEvent(target, event) {
        if (!event.isTrusted) return;
        if (event.type === 'pointerdown') {
            pointerDownPopover = popoverTopmostFor(target);
        } else if (event.type === 'pointerup') {
            const upPopover = popoverTopmostFor(target);
            const downPopover = pointerDownPopover;
            pointerDownPopover = null;
            if (upPopover !== downPopover) return;
            for (const element of [...openPopovers].reverse()) {
                if (element === upPopover) break;
                if (openedPopoverMode(element) !== 'manual')
                    popoverHide(element, { restoreFocus: false });
            }
        } else if (event.type === 'keydown' && event.key === 'Escape' && !event.defaultPrevented) {
            const top = [...openPopovers].reverse().find(element => openedPopoverMode(element) !== 'manual');
            if (top) popoverHide(top);
        }
    }
